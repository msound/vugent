//! The HTTP server behind vugent's browser UI.

use std::borrow::Cow;

use axum::Router;
use axum::http::{StatusCode, Uri, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use rust_embed::RustEmbed;
use tokio::net::TcpListener;

/// The built frontend, which `npm run build` in `web/` writes to this crate's `frontend/` folder. In debug builds
/// rust-embed reads it from disk at run time; in release builds it is compiled in. It may be missing when the frontend
/// has not been built (see `build.rs`).
#[derive(RustEmbed)]
#[folder = "frontend/"]
#[allow_missing = true]
struct Frontend;

/// Builds the application's routes.
pub fn router() -> Router {
    Router::new()
        .route("/api/health", get(health))
        .fallback(frontend)
}

/// Serves `router` on `listener` until Ctrl-C is pressed.
pub async fn serve(listener: TcpListener, router: Router) -> std::io::Result<()> {
    axum::serve(listener, router)
        .with_graceful_shutdown(shutdown_signal())
        .await
}

async fn health() -> &'static str {
    "ok"
}

async fn frontend(uri: Uri) -> Response {
    asset_response(uri.path(), |path| {
        Frontend::get(path).map(|file| Asset {
            mime: file.metadata.mimetype().to_owned(),
            data: file.data,
        })
    })
}

/// A file of the built frontend.
struct Asset {
    data: Cow<'static, [u8]>,
    mime: String,
}

const MISSING_FRONTEND: &str = "The vugent frontend was not built into this binary.\n\
Run `npm install && npm run build` in web/, then rebuild vugent.\n";

/// Answers a request for `path` with a file from `get`, falling back to `index.html` so that client-side routes work.
fn asset_response(path: &str, get: impl Fn(&str) -> Option<Asset>) -> Response {
    let path = path.trim_start_matches('/');
    if path == "api" || path.starts_with("api/") {
        return StatusCode::NOT_FOUND.into_response();
    }

    if let Some(asset) = get(path).filter(|_| !path.is_empty()) {
        // Vite puts content hashes in the names of files under assets/, so they never change.
        let cache = if path.starts_with("assets/") {
            "public, max-age=31536000, immutable"
        } else {
            "no-cache"
        };
        return file_response(asset, cache);
    }

    match get("index.html") {
        Some(index) => file_response(index, "no-cache"),
        None => (
            StatusCode::SERVICE_UNAVAILABLE,
            [(header::CACHE_CONTROL, "no-store")],
            MISSING_FRONTEND,
        )
            .into_response(),
    }
}

fn file_response(asset: Asset, cache: &'static str) -> Response {
    (
        [
            (header::CONTENT_TYPE, asset.mime),
            (header::CACHE_CONTROL, cache.to_owned()),
        ],
        asset.data,
    )
        .into_response()
}

async fn shutdown_signal() {
    // If the handler cannot be installed, keep serving; the process can still be killed.
    if tokio::signal::ctrl_c().await.is_err() {
        std::future::pending::<()>().await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::{Body, to_bytes};
    use axum::http::Request;
    use tower::ServiceExt;

    async fn get(path: &str) -> Response {
        router()
            .oneshot(Request::get(path).body(Body::empty()).unwrap())
            .await
            .unwrap()
    }

    async fn body(response: Response) -> Vec<u8> {
        to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap()
            .to_vec()
    }

    fn header_of(response: &Response, name: header::HeaderName) -> &str {
        response.headers()[name].to_str().unwrap()
    }

    fn fake_dist(path: &str) -> Option<Asset> {
        let (data, mime): (&[u8], _) = match path {
            "index.html" => (b"<html>", "text/html"),
            "logo.svg" => (b"<svg>", "image/svg+xml"),
            "assets/index-abc.js" => (b"js", "text/javascript"),
            _ => return None,
        };
        Some(Asset {
            data: Cow::Borrowed(data),
            mime: mime.to_owned(),
        })
    }

    #[tokio::test]
    async fn health_returns_ok() {
        let response = get("/api/health").await;

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(body(response).await, b"ok");
    }

    #[tokio::test]
    async fn unknown_api_route_is_not_found() {
        assert_eq!(get("/api/nope").await.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn root_serves_the_frontend_or_explains_it_is_missing() {
        let response = get("/").await;

        match Frontend::get("index.html") {
            Some(_) => {
                assert_eq!(response.status(), StatusCode::OK);
                assert!(header_of(&response, header::CONTENT_TYPE).starts_with("text/html"));
            }
            None => assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE),
        }
    }

    #[tokio::test]
    async fn serves_index_at_root() {
        let response = asset_response("/", fake_dist);

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(header_of(&response, header::CONTENT_TYPE), "text/html");
        assert_eq!(header_of(&response, header::CACHE_CONTROL), "no-cache");
        assert_eq!(body(response).await, b"<html>");
    }

    #[tokio::test]
    async fn serves_files_with_their_type() {
        let response = asset_response("/logo.svg", fake_dist);

        assert_eq!(header_of(&response, header::CONTENT_TYPE), "image/svg+xml");
        assert_eq!(header_of(&response, header::CACHE_CONTROL), "no-cache");
        assert_eq!(body(response).await, b"<svg>");
    }

    #[tokio::test]
    async fn hashed_assets_are_cached_forever() {
        let response = asset_response("/assets/index-abc.js", fake_dist);

        assert_eq!(
            header_of(&response, header::CACHE_CONTROL),
            "public, max-age=31536000, immutable"
        );
        assert_eq!(body(response).await, b"js");
    }

    #[tokio::test]
    async fn unknown_paths_fall_back_to_index() {
        let response = asset_response("/some/client/route", fake_dist);

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(body(response).await, b"<html>");
    }

    #[tokio::test]
    async fn api_paths_never_fall_back_to_index() {
        let response = asset_response("/api/nope", fake_dist);

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn explains_a_missing_frontend() {
        let response = asset_response("/", |_| None);

        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(body(response).await, MISSING_FRONTEND.as_bytes());
    }
}
