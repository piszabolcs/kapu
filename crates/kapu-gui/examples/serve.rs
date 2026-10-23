//! Development server: the KAPU window's interface in a browser, backed by the
//! same commands, for screenshots and interface tests.
//!
//! `cargo run -p kapu-gui --example serve -- [file.kapu] [--port 7878]`
//!
//! It listens on 127.0.0.1 only and serves `ui/` from the source tree, so an edit
//! shows on reload. `POST /api/<command>` takes the JSON arguments and answers
//! `{"ok": ...}` or `{"error": {"code", "detail"}}`.

use std::path::{Component, Path, PathBuf};

use kapu_gui::api::Session;
use serde_json::{Value, json};
use tiny_http::{Header, Method, Request, Response, Server};

fn content_type(path: &Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()) {
        Some("html") => "text/html; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("js") => "text/javascript; charset=utf-8",
        Some("svg") => "image/svg+xml",
        Some("woff2") => "font/woff2",
        Some("png") => "image/png",
        _ => "application/octet-stream",
    }
}

fn header(name: &str, value: &str) -> Option<Header> {
    Header::from_bytes(name.as_bytes(), value.as_bytes()).ok()
}

fn respond(request: Request, status: u16, body: Vec<u8>, kind: &str) {
    let mut response = Response::from_data(body).with_status_code(status);
    for h in [
        header("Content-Type", kind),
        header("Cache-Control", "no-store"),
    ]
    .into_iter()
    .flatten()
    {
        response.add_header(h);
    }
    if let Err(e) = request.respond(response) {
        eprintln!("serve: {e}");
    }
}

fn api(session: &Session, command: &str, request: &mut Request) -> Value {
    let mut body = String::new();
    if let Err(e) = request.as_reader().read_to_string(&mut body) {
        return json!({ "error": { "code": "GUI", "detail": e.to_string() } });
    }
    let args: Value = serde_json::from_str(&body).unwrap_or_else(|_| json!({}));
    match session.dispatch(command, &args) {
        Ok(v) => json!({ "ok": v }),
        Err(e) => json!({ "error": e }),
    }
}

/// `ui/<path>` if the path stays inside `ui/`.
fn static_file(ui: &Path, url: &str) -> Option<PathBuf> {
    let rel = Path::new(url.trim_start_matches('/'));
    if rel.components().any(|c| !matches!(c, Component::Normal(_))) {
        return None;
    }
    let path = if rel.as_os_str().is_empty() {
        ui.join("index.html")
    } else {
        ui.join(rel)
    };
    path.is_file().then_some(path)
}

fn main() -> std::process::ExitCode {
    let mut file = None;
    let mut port = 7878_u16;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        if a == "--port" {
            port = args.next().and_then(|p| p.parse().ok()).unwrap_or(port);
        } else {
            file = Some(PathBuf::from(a));
        }
    }
    let ui = Path::new(env!("CARGO_MANIFEST_DIR")).join("ui");
    let session = Session::new(file);
    let server = match Server::http(("127.0.0.1", port)) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("serve: cannot listen on 127.0.0.1:{port}: {e}");
            return std::process::ExitCode::FAILURE;
        }
    };
    println!("KAPU interface on http://127.0.0.1:{port}/");
    for mut request in server.incoming_requests() {
        let url = request
            .url()
            .split(['?', '#'])
            .next()
            .unwrap_or("/")
            .to_owned();
        if *request.method() == Method::Post
            && let Some(command) = url.strip_prefix("/api/")
        {
            let answer = api(&session, command, &mut request);
            respond(
                request,
                200,
                answer.to_string().into_bytes(),
                "application/json",
            );
            continue;
        }
        match static_file(&ui, &url).map(|p| (std::fs::read(&p), p)) {
            Some((Ok(body), path)) => respond(request, 200, body, content_type(&path)),
            _ => respond(request, 404, b"not found".to_vec(), "text/plain"),
        }
    }
    std::process::ExitCode::SUCCESS
}
