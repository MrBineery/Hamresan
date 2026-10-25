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
    pub is_dir: bool,
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
            let name = entry.file_name().to_string_lossy().to_string();
            
            // Skip hidden files/folders
            if name.starts_with('.') { continue; }

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
                size: if meta.is_file() { meta.len() } else { 0 },
                modified,
                mime,
                is_dir: meta.is_dir(),
            });
        }

        files.sort_by(|a, b| b.modified.cmp(&a.modified));
        Ok(files)
    }

    pub async fn save_stream<R>(&self, relative_path: &str, mut reader: R) -> Result<FileInfo>
    where
        R: tokio::io::AsyncRead + Unpin,
    {
        use tokio::io::AsyncWriteExt;
        
        let safe_path = sanitize_path(relative_path);
        let full_path = self.unique_path(&safe_path).await?;
        
        // Create parent directories if they don't exist (for folder uploads)
        if let Some(parent) = full_path.parent() {
            fs::create_dir_all(parent).await?;
        }

        let mut file = fs::File::create(&full_path).await?;
        tokio::io::copy(&mut reader, &mut file).await?;
        file.flush().await?;
        file.sync_all().await.ok();
        
        self.info(&full_path).await
    }

    pub fn resolve(&self, name: &str) -> Option<PathBuf> {
        let safe = sanitize_path(name);
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
        let safe = sanitize_path(name);
        let path = self.root.join(&safe);
        
        if let Ok(canonical) = path.canonicalize() {
            let canonical = strip_extended_prefix(canonical);
            if !canonical.starts_with(&self.root) {
                return Ok(false);
            }
            if canonical.is_file() {
                fs::remove_file(canonical).await?;
                return Ok(true);
            } else if canonical.is_dir() {
                fs::remove_dir_all(canonical).await?;
                return Ok(true);
            }
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
            size: if meta.is_file() { meta.len() } else { 0 },
            modified,
            mime,
            is_dir: meta.is_dir(),
        })
    }

    async fn unique_path(&self, relative_path: &str) -> Result<PathBuf> {
        let path = self.root.join(relative_path);
        if !path.exists() {
            return Ok(path);
        }

        let path_obj = Path::new(relative_path);
        let parent = path_obj.parent().unwrap_or_else(|| Path::new(""));
        let stem = path_obj
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "file".into());
        let ext = path_obj
            .extension()
            .map(|s| format!(".{}", s.to_string_lossy()))
            .unwrap_or_default();

        for i in 1..10000 {
            let candidate_name = format!("{}-{}{}", stem, i, ext);
            let candidate = self.root.join(parent).join(candidate_name);
            if !candidate.exists() {
                return Ok(candidate);
            }
        }
        anyhow::bail!("could not find unique name for {}", relative_path)
    }
}

fn sanitize_path(path: &str) -> String {
    let parts: Vec<String> = path
        .replace('\\', "/") // Normalize Windows paths
        .split('/')
        .filter(|s| !s.is_empty() && *s != "." && *s != "..")
        .map(|s| {
            s.chars()
                .map(|c| match c {
                    '\0' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
                    c => c,
                })
                .collect::<String>()
        })
        .collect();

    if parts.is_empty() {
        "unnamed".into()
    } else {
        parts.join("/")
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