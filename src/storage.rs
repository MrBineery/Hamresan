use anyhow::Result;
use serde::Serialize;
use std::path::{Path, PathBuf};
use tokio::fs;

#[derive(Debug, Serialize, Clone)]
pub struct FileInfo {
    pub name: String,
    pub size: u64,
    pub modified: Option<u64>,
    pub mime: String,
}

#[derive(Clone)]
pub struct Storage {
    root: PathBuf,
}

impl Storage {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    pub async fn list(&self) -> Result<Vec<FileInfo>> {
        let mut entries = fs::read_dir(&self.root).await?;
        let mut files = Vec::new();

        while let Some(entry) = entries.next_entry().await? {
            let meta = entry.metadata().await?;
            if !meta.is_file() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().to_string();
            let modified = meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs());
            let mime = mime_guess::from_path(&name)
                .first_or_octet_stream()
                .to_string();

            files.push(FileInfo {
                name,
                size: meta.len(),
                modified,
                mime,
            });
        }

        files.sort_by(|a, b| b.modified.cmp(&a.modified));
        Ok(files)
    }

    pub async fn save_stream<R>(&self, name: &str, mut reader: R) -> Result<FileInfo>
    where
        R: tokio::io::AsyncRead + Unpin,
    {
        use tokio::io::AsyncWriteExt;
        let safe = sanitize(name);
        let path = self.unique_path(&safe).await?;
        let mut file = fs::File::create(&path).await?;
        tokio::io::copy(&mut reader, &mut file).await?;
        file.flush().await?;
        file.sync_all().await.ok();
        self.info(&path).await
    }

    /// Resolve a filename to an absolute path, but only if it's inside our root.
    pub fn resolve(&self, name: &str) -> Option<PathBuf> {
        let safe = sanitize(name);
        let path = self.root.join(&safe);
        let canonical = path.canonicalize().ok()?;
        let canonical = strip_extended_prefix(canonical);
        if !canonical.starts_with(&self.root) {
            return None;
        }
        if !canonical.is_file() {
            return None;
        }
        Some(canonical)
    }

    pub async fn delete(&self, name: &str) -> Result<bool> {
        if let Some(path) = self.resolve(name) {
            fs::remove_file(path).await?;
            return Ok(true);
        }
        Ok(false)
    }

    async fn info(&self, path: &Path) -> Result<FileInfo> {
        let meta = fs::metadata(path).await?;
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        let modified = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs());
        let mime = mime_guess::from_path(&name)
            .first_or_octet_stream()
            .to_string();

        Ok(FileInfo {
            name,
            size: meta.len(),
            modified,
            mime,
        })
    }

    async fn unique_path(&self, name: &str) -> Result<PathBuf> {
        let path = self.root.join(name);
        if !path.exists() {
            return Ok(path);
        }

        let path_obj = Path::new(name);
        let stem = path_obj
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "file".into());
        let ext = path_obj
            .extension()
            .map(|s| format!(".{}", s.to_string_lossy()))
            .unwrap_or_default();

        for i in 1..10000 {
            let candidate = self.root.join(format!("{}-{}{}", stem, i, ext));
            if !candidate.exists() {
                return Ok(candidate);
            }
        }
        anyhow::bail!("could not find unique name for {}", name)
    }
}

fn sanitize(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| match c {
            '/' | '\\' | '\0' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            c => c,
        })
        .collect();
    let trimmed = cleaned.trim_start_matches('.').to_string();
    if trimmed.is_empty() {
        "file".into()
    } else {
        trimmed
    }
}

fn strip_extended_prefix(p: PathBuf) -> PathBuf {
    let s = p.to_string_lossy();
    if let Some(rest) = s.strip_prefix(r"\\?\") {
        PathBuf::from(rest)
    } else {
        p
    }
}