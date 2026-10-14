//! A small local API for editors (the HeliLingo VS Code extension): HTTP on
//! 127.0.0.1, a random port, and a random token. Both are written to
//! `%LOCALAPPDATA%\HeliLingo\api.json`, which the extension reads; nothing
//! listens on other interfaces, and every request needs the token.
//!
//! - `GET  /status` → `{ version, target, secondLang }`
//! - `GET  /engines` → `[{ id, name, group }]` (switched-on engines, the
//!   offline models included; nothing about keys)
//! - `POST /translate` `{ text, source?, target?, engine? }` →
//!   `{ text, sourceLang, target, engine }`
//!
//! Translation goes through the usual pipeline and engine chain, with the
//! user's «Если текст уже на …» rule: a Russian comment comes back in
//! English. Settings → «Контекст и режимы» → «Расширение VS Code» turns
//! it off.

use std::hash::{BuildHasher, Hasher};
use std::sync::{Arc, Mutex};

use hl_core::RequestId;
use hl_pipeline::Request;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

use crate::state::AppState;

/// Longest request body (a long comment or a file's worth of text).
const MAX_BODY: usize = 256 * 1024;

static SERVER: Mutex<Option<tauri::async_runtime::JoinHandle<()>>> = Mutex::new(None);

fn info_file() -> std::path::PathBuf {
    crate::settings::local_dir().join("api.json")
}

/// 128 random bits as hex (the std hasher is seeded from the OS).
fn token() -> String {
    let part = || {
        let mut h = std::collections::hash_map::RandomState::new().build_hasher();
        h.write_u128(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos()));
        h.finish()
    };
    format!("{:016x}{:016x}", part(), part())
}

#[derive(Serialize)]
struct Info<'a> {
    port: u16,
    token: &'a str,
    version: &'a str,
    pid: u32,
}

/// Starts or stops the API to match the setting.
pub fn apply(app: &AppHandle, enabled: bool) {
    let mut server = SERVER.lock().unwrap();
    match (enabled, server.is_some()) {
        (true, false) => {
            let app = app.clone();
            *server = Some(tauri::async_runtime::spawn(async move {
                if let Err(e) = serve(app).await {
                    tracing::warn!("editor API: {e}");
                }
            }));
        }
        (false, true) => {
            if let Some(task) = server.take() {
                task.abort();
            }
            let _ = std::fs::remove_file(info_file());
        }
        _ => {}
    }
}

/// On exit: the extension shouldn't find a dead port.
pub fn shutdown() {
    let _ = std::fs::remove_file(info_file());
}

async fn serve(app: AppHandle) -> std::io::Result<()> {
    let listener = TcpListener::bind(("127.0.0.1", 0)).await?;
    let port = listener.local_addr()?.port();
    let token = token();
    let info = Info { port, token: &token, version: env!("CARGO_PKG_VERSION"), pid: std::process::id() };
    std::fs::create_dir_all(crate::settings::local_dir())?;
    std::fs::write(info_file(), serde_json::to_vec(&info).unwrap_or_default())?;
    tracing::info!(port, "editor API listening");
    let token = Arc::new(token);
    loop {
        let (stream, peer) = listener.accept().await?;
        if !peer.ip().is_loopback() {
            continue;
        }
        let (app, token) = (app.clone(), token.clone());
        tauri::async_runtime::spawn(async move {
            let _ = handle(app, stream, &token).await;
        });
    }
}

struct Http {
    method: String,
    path: String,
    auth: Option<String>,
    body: Vec<u8>,
}

/// Reads one request: the head, then a `Content-Length` body.
async fn read_request(stream: &mut TcpStream) -> Option<Http> {
    let mut buf = Vec::with_capacity(4096);
    let mut chunk = [0u8; 4096];
    let head_end = loop {
        let n = stream.read(&mut chunk).await.ok()?;
        if n == 0 {
            return None;
        }
        buf.extend_from_slice(&chunk[..n]);
        if let Some(i) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            break i;
        }
        if buf.len() > 16 * 1024 {
            return None;
        }
    };
    let head = String::from_utf8_lossy(&buf[..head_end]).into_owned();
    let mut lines = head.split("\r\n");
    let mut first = lines.next()?.split(' ');
    let (method, path) = (first.next()?.to_owned(), first.next()?.to_owned());
    let (mut length, mut auth) = (0usize, None);
    for l in lines {
        let Some((k, v)) = l.split_once(':') else { continue };
        match k.trim().to_ascii_lowercase().as_str() {
            "content-length" => length = v.trim().parse().ok()?,
            "authorization" => auth = v.trim().strip_prefix("Bearer ").map(str::to_owned),
            _ => {}
        }
    }
    if length > MAX_BODY {
        return None;
    }
    let mut body = buf[head_end + 4..].to_vec();
    while body.len() < length {
        let n = stream.read(&mut chunk).await.ok()?;
        if n == 0 {
            return None;
        }
        body.extend_from_slice(&chunk[..n]);
    }
    body.truncate(length);
    Some(Http { method, path, auth, body })
}

async fn respond(stream: &mut TcpStream, status: &str, body: &serde_json::Value) -> std::io::Result<()> {
    let body = serde_json::to_vec(body).unwrap_or_default();
    let head = format!(
        "HTTP/1.1 {status}\r\nContent-Type: application/json; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    stream.write_all(head.as_bytes()).await?;
    stream.write_all(&body).await?;
    stream.shutdown().await
}

#[derive(Deserialize)]
struct TranslateBody {
    text: String,
    source: Option<String>,
    target: Option<String>,
    engine: Option<String>,
}

async fn handle(app: AppHandle, mut stream: TcpStream, token: &str) -> std::io::Result<()> {
    let Some(req) = read_request(&mut stream).await else {
        return respond(&mut stream, "400 Bad Request", &serde_json::json!({ "error": "bad request" })).await;
    };
    // Constant-time enough for a local token: compare whole strings.
    if req.auth.as_deref() != Some(token) {
        return respond(&mut stream, "401 Unauthorized", &serde_json::json!({ "error": "token" })).await;
    }
    let state = app.state::<Arc<AppState>>().inner().clone();
    match (req.method.as_str(), req.path.as_str()) {
        ("GET", "/status") => {
            let body = {
                let s = state.settings.read().unwrap();
                serde_json::json!({
                    "version": env!("CARGO_PKG_VERSION"),
                    "target": s.target_lang,
                    "secondLang": s.second_lang,
                })
            };
            respond(&mut stream, "200 OK", &body).await
        }
        ("GET", "/engines") => {
            let list: Vec<serde_json::Value> = state
                .engine_list()
                .into_iter()
                .filter(|e| e.enabled && e.implemented)
                .map(|e| serde_json::json!({ "id": e.id, "name": e.name, "group": e.group }))
                .collect();
            respond(&mut stream, "200 OK", &serde_json::Value::Array(list)).await
        }
        ("POST", "/translate") => {
            let Ok(b) = serde_json::from_slice::<TranslateBody>(&req.body) else {
                return respond(&mut stream, "400 Bad Request", &serde_json::json!({ "error": "json" })).await;
            };
            let (target, second, programmer) = {
                let s = state.settings.read().unwrap();
                (b.target.clone().filter(|t| !t.is_empty()).unwrap_or_else(|| s.target_lang.clone()), s.second_lang.clone(), s.programmer_mode)
            };
            let r = Request {
                text: b.text,
                source: b.source.unwrap_or_else(|| "auto".into()),
                target,
                second_lang: second,
                engine: String::new(),
                programmer_mode: programmer,
                context: Vec::new(),
            };
            let router = state.router(b.engine.as_deref());
            match state.pipeline.run(RequestId(0), r, router, state.online(), |_| {}).await {
                Ok(done) => {
                    let body = serde_json::json!({
                        "text": done.text,
                        "sourceLang": done.source_lang,
                        "target": done.target,
                        "engine": done.engine,
                    });
                    respond(&mut stream, "200 OK", &body).await
                }
                Err(e) => respond(&mut stream, "502 Bad Gateway", &serde_json::json!({ "error": e })).await,
            }
        }
        _ => respond(&mut stream, "404 Not Found", &serde_json::json!({ "error": "not found" })).await,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One request through a real loopback socket: head, auth, body split
    /// across writes.
    #[tokio::test]
    async fn reads_requests() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let client = tokio::spawn(async move {
            let mut c = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
            let body = r#"{"text":"Привет, мир"}"#.as_bytes();
            let head = format!(
                "POST /translate HTTP/1.1\r\nHost: x\r\nAuthorization: Bearer abc\r\nContent-Length: {}\r\n\r\n",
                body.len()
            );
            c.write_all(head.as_bytes()).await.unwrap();
            c.write_all(&body[..5]).await.unwrap();
            c.flush().await.unwrap();
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            c.write_all(&body[5..]).await.unwrap();
            let mut out = String::new();
            c.read_to_string(&mut out).await.unwrap();
            out
        });
        let (mut s, _) = listener.accept().await.unwrap();
        let req = read_request(&mut s).await.unwrap();
        assert_eq!((req.method.as_str(), req.path.as_str()), ("POST", "/translate"));
        assert_eq!(req.auth.as_deref(), Some("abc"));
        let b: TranslateBody = serde_json::from_slice(&req.body).unwrap();
        assert_eq!(b.text, "Привет, мир");
        respond(&mut s, "200 OK", &serde_json::json!({ "text": "Hello, world" })).await.unwrap();
        let reply = client.await.unwrap();
        assert!(reply.starts_with("HTTP/1.1 200 OK"));
        assert!(reply.ends_with(r#"{"text":"Hello, world"}"#));
    }

    #[test]
    fn tokens_differ() {
        let (a, b) = (token(), token());
        assert_eq!(a.len(), 32);
        assert_ne!(a, b);
    }
}
