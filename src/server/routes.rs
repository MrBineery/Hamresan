use axum::{
    body::Body,
    extract::{DefaultBodyLimit, Multipart, Path, Request, State},
    http::{header, HeaderMap, StatusCode},
    middleware::{self, Next},
    response::{Html, IntoResponse, Response},
    routing::{delete, get, post},
    Json, Router,
};
use futures_util::TryStreamExt;
use qrcode::render::svg;
use qrcode::QrCode;
use serde::Deserialize;
use serde_json::json;
use tokio::fs::File;
use tokio_util::io::{ReaderStream, StreamReader};

use crate::server::AppState;

const COOKIE_NAME: &str = "hr-session";

pub fn router(state: AppState) -> Router {
    let public = Router::new()
        .route("/", get(index))
        .route("/api/login", post(login))
        .route("/api/auth-status", get(auth_status))
        .route("/api/qr", get(qr_svg));

    let protected = Router::new()
        .route("/api/info", get(info))
        .route("/api/files", get(list_files))
        .route(
            "/api/upload",
            post(upload).layer(DefaultBodyLimit::disable()),
        )
        .route("/api/files/:name", delete(delete_file))
        .route("/download/*path", get(download)) // Changed to *path for subfolders
        .route(
            "/api/clipboard",
            get(get_clipboard).post(set_clipboard),
        )
        .route_layer(middleware::from_fn_with_state(state.clone(), auth));

    public.merge(protected).with_state(state)
}

// ---------------- Auth ----------------

fn is_localhost(headers: &HeaderMap) -> bool {
    headers
        .get(header::HOST)
        .and_then(|v| v.to_str().ok())
        .map(|h| {
            let h = h.to_ascii_lowercase();
            let host = h.split(':').next().unwrap_or("");
            host == "localhost" || host == "127.0.0.1"
        })
        .unwrap_or(false)
}

fn has_session(headers: &HeaderMap, token: &str) -> bool {
    if token.is_empty() {
        return false;
    }
    headers
        .get(header::COOKIE)
        .and_then(|v| v.to_str().ok())
        .map(|cookies| {
            cookies.split(';').any(|c| {
                let c = c.trim();
                match c.strip_prefix(&format!("{}=", COOKIE_NAME)) {
                    Some(val) => val == token,
                    None => false,
                }
            })
        })
        .unwrap_or(false)
}

async fn auth(State(state): State<AppState>, req: Request, next: Next) -> Response {
    if state.pin.is_none() {
        return next.run(req).await;
    }
    if is_localhost(req.headers()) {
        return next.run(req).await;
    }
    if has_session(req.headers(), &state.token) {
        next.run(req).await
    } else {
        (StatusCode::UNAUTHORIZED, "unauthorized").into_response()
    }
}

#[derive(Deserialize)]
struct LoginReq {
    pin: String,
}

async fn login(State(state): State<AppState>, Json(payload): Json<LoginReq>) -> Response {
    let Some(expected) = &state.pin else {
        return Json(json!({ "ok": true })).into_response();
    };

    if &payload.pin == expected {
        let cookie = format!(
            "{}={}; Path=/; HttpOnly; SameSite=Lax; Max-Age={}",
            COOKIE_NAME,
            state.token,
            60 * 60 * 24 * 30
        );
        Response::builder()
            .status(StatusCode::OK)
            .header(header::SET_COOKIE, cookie)
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(r#"{"ok":true}"#))
            .unwrap()
    } else {
        (
            StatusCode::UNAUTHORIZED,
            Json(json!({ "ok": false, "error": "invalid pin" })),
        )
            .into_response()
    }
}

async fn auth_status(State(state): State<AppState>, headers: HeaderMap) -> impl IntoResponse {
    let needs_pin = state.pin.is_some();
    let authed = !needs_pin || is_localhost(&headers) || has_session(&headers, &state.token);
    Json(json!({ "needs_pin": needs_pin, "authenticated": authed }))
}

// ---------------- Static pages ----------------

async fn index() -> Html<&'static str> {
    Html(include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/frontend/dist/index.html")))
}

async fn info(State(state): State<AppState>) -> impl IntoResponse {
    Json(json!({
        "url": state.url,
        "pin": state.pin_display.clone(),
    }))
}

async fn qr_svg(State(state): State<AppState>) -> Response {
    let code = match QrCode::new(state.url.as_bytes()) {
        Ok(c) => c,
        Err(_) => return (StatusCode::INTERNAL_SERVER_ERROR, "qr error").into_response(),
    };
    let svg_str = code
        .render::<svg::Color>()
        .min_dimensions(240, 240)
        .quiet_zone(true)
        .dark_color(svg::Color("#0f131d"))
        .light_color(svg::Color("#ffffff"))
        .build();
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "image/svg+xml; charset=utf-8")
        .header(header::CACHE_CONTROL, "no-cache")
        .body(Body::from(svg_str))
        .unwrap()
}

// ---------------- Clipboard ----------------

#[derive(Deserialize)]
struct ClipboardIn {
    text: String,
}

async fn get_clipboard(State(state): State<AppState>) -> impl IntoResponse {
    let cb = state.clipboard.read().await;
    Json(json!({ "text": cb.clone().unwrap_or_default() }))
}

async fn set_clipboard(
    State(state): State<AppState>,
    Json(payload): Json<ClipboardIn>,
) -> impl IntoResponse {
    let mut cb = state.clipboard.write().await;
    let text = payload.text.trim_end().to_string();
    *cb = if text.is_empty() { None } else { Some(text) };
    Json(json!({ "ok": true }))
}

// ---------------- Files ----------------

async fn list_files(State(state): State<AppState>) -> impl IntoResponse {
    match state.storage.list().await {
        Ok(files) => Json(json!({ "files": files })).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

async fn upload(State(state): State<AppState>, mut multipart: Multipart) -> impl IntoResponse {
    let mut uploaded = Vec::new();
    loop {
        let field = match multipart.next_field().await {
            Ok(Some(f)) => f,
            Ok(None) => break,
            Err(e) => {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(json!({ "error": format!("multipart error: {}", e) })),
                )
                    .into_response();
            }
        };
        
        // Use file_name() which can contain relative paths for folders (e.g., "my_folder/photo.jpg")
        let filename = field
            .file_name()
            .map(|s| s.to_string())
            .unwrap_or_else(|| "upload.bin".into());
            
        let reader = StreamReader::new(
            field.map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e)),
        );
        
        match state.storage.save_stream(&filename, reader).await {
            Ok(info) => {
                tracing::info!("saved {} ({} bytes)", info.name, info.size);
                uploaded.push(info);
            }
            Err(e) => {
                tracing::error!("save error: {:#}", e);
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({ "error": format!("save error: {}", e) })),
                )
                    .into_response();
            }
        }
    }
    Json(json!({ "uploaded": uploaded })).into_response()
}

async fn delete_file(State(state): State<AppState>, Path(name): Path<String>) -> impl IntoResponse {
    match state.storage.delete(&name).await {
        Ok(true) => Json(json!({ "ok": true })).into_response(),
        Ok(false) => (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "not found" })),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

async fn download(State(state): State<AppState>, Path(name): Path<String>) -> Response {
    let Some(path) = state.storage.resolve(&name) else {
        return (StatusCode::NOT_FOUND, "not found").into_response();
    };
    let meta = match tokio::fs::metadata(&path).await {
        Ok(m) => m,
        Err(_) => return (StatusCode::NOT_FOUND, "not found").into_response(),
    };
    let file = match File::open(&path).await {
        Ok(f) => f,
        Err(_) => return (StatusCode::NOT_FOUND, "not found").into_response(),
    };
    let stream = ReaderStream::new(file);
    let body = Body::from_stream(stream);
    let safe_name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "file".into());
    let mime = mime_guess::from_path(&safe_name).first_or_octet_stream();
    let content_disposition = format!(
        "attachment; filename=\"{}\"; filename*=UTF-8''{}",
        safe_name.replace('"', "_"),
        urlencoding::encode(&safe_name)
    );
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, mime.as_ref())
        .header(header::CONTENT_LENGTH, meta.len())
        .header(header::CONTENT_DISPOSITION, content_disposition)
        .body(body)
        .unwrap()
}
