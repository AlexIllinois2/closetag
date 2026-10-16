use actix_web::{HttpRequest, HttpResponse};
use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "frontend/dist"]
struct Assets;

pub async fn index() -> HttpResponse {
    serve("index.html")
}

/// Serve an embedded file at the request's own path.
/// The route is mounted at `/assets/{tail:.*}`, but rust-embed keys are paths
/// relative to `frontend/dist` (e.g. `assets/index-xxxx.js`), so we must look
/// the file up by the full request path instead of the captured tail.
pub async fn asset(req: HttpRequest) -> HttpResponse {
    serve(req.path().trim_start_matches('/'))
}

/// SPA fallback: serve a real embedded file when one exists at this path
/// (favicon, etc.), otherwise serve index.html so client routing works.
/// Unknown /api and /ws paths still get a JSON 404.
pub async fn spa(req: HttpRequest) -> HttpResponse {
    let path = req.path();
    if path.starts_with("/api") || path.starts_with("/ws") {
        return HttpResponse::NotFound().json(serde_json::json!({ "error": "not found" }));
    }
    let p = path.trim_start_matches('/');
    if !p.is_empty() && p.contains('.') && !p.contains("..") {
        if let Some(content) = Assets::get(p) {
            return build_response(p, content);
        }
    }
    serve("index.html")
}

fn serve(path: &str) -> HttpResponse {
    match Assets::get(path) {
        Some(content) => build_response(path, content),
        None => HttpResponse::NotFound().json(serde_json::json!({ "error": "asset not found" })),
    }
}

fn build_response(path: &str, content: rust_embed::EmbeddedFile) -> HttpResponse {
    let mime = mime_guess::from_path(path).first_or_octet_stream();
    let mut builder = HttpResponse::Ok();
    builder.insert_header(("Content-Type", mime.as_ref()));
    if path.ends_with(".html") {
        builder.insert_header(("Cache-Control", "no-cache"));
    } else {
        builder.insert_header(("Cache-Control", "public, max-age=31536000, immutable"));
    }
    builder.body(content.data.into_owned())
}
