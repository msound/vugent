//! The HTTP server behind vugent's browser UI.

mod tree;

use std::borrow::Cow;
use std::sync::Arc;

use axum::extract::{Query, State};
use axum::http::{StatusCode, Uri, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use rust_embed::RustEmbed;
use serde::Deserialize;
use tokio::net::TcpListener;
use vugent_git::{Rev, Review};

/// The built frontend, which `npm run build` in `web/` writes to this crate's `frontend/` folder. In debug builds
/// rust-embed reads it from disk at run time; in release builds it is compiled in. It may be missing when the frontend
/// has not been built (see `build.rs`).
#[derive(RustEmbed)]
#[folder = "frontend/"]
#[allow_missing = true]
struct Frontend;

/// Builds the application's routes, serving `review`.
pub fn router(review: Review) -> Router {
    Router::new()
        .route("/api/health", get(health))
        .route("/api/tree", get(tree))
        .route("/api/diff", get(diff))
        .fallback(frontend)
        .with_state(Arc::new(review))
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

/// `GET /api/tree`: every folder and file of the review, with how each file changed.
async fn tree(State(review): State<Arc<Review>>) -> Response {
    let result = tokio::task::spawn_blocking(move || {
        let files = review.files(&Rev::WorkTree)?;
        let name = review
            .dir()
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        Ok(tree::build(&name, files))
    })
    .await;
    json_response(result)
}

#[derive(Debug, Deserialize)]
struct DiffQuery {
    path: String,
}

/// `GET /api/diff?path=<path>`: the diff of one file of the review.
async fn diff(State(review): State<Arc<Review>>, Query(query): Query<DiffQuery>) -> Response {
    let result =
        tokio::task::spawn_blocking(move || review.diff(&Rev::WorkTree, &query.path)).await;
    json_response(result)
}

/// Answers with `result` as JSON, or with the error as plain text.
fn json_response<T: serde::Serialize>(
    result: Result<Result<T, vugent_git::Error>, tokio::task::JoinError>,
) -> Response {
    let error = match result {
        Ok(Ok(value)) => {
            return ([(header::CACHE_CONTROL, "no-store")], Json(value)).into_response();
        }
        Ok(Err(error)) => error,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };
    let status = match error {
        vugent_git::Error::NotFound(_) => StatusCode::NOT_FOUND,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    };
    (status, error_message(&error)).into_response()
}

/// The error with its causes, such as "cannot read x: permission denied".
fn error_message(error: &dyn std::error::Error) -> String {
    let mut message = error.to_string();
    let mut source = error.source();
    while let Some(cause) = source {
        message.push_str(&format!(": {cause}"));
        source = cause.source();
    }
    message
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
        get_in(std::env::temp_dir(), path).await
    }

    async fn get_in(dir: impl Into<std::path::PathBuf>, path: &str) -> Response {
        router(Review::new(dir))
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

    /// Runs git in `dir` for test setup, isolated from the user's git configuration.
    fn git(dir: &std::path::Path, args: &[&str]) {
        let status = std::process::Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(["-c", "user.name=Test", "-c", "user.email=test@example.com"])
            .args(["-c", "commit.gpgsign=false"])
            .args(args)
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .status()
            .unwrap();
        assert!(status.success(), "git {args:?} failed");
    }

    /// A repository with a committed `src/a.txt` that has since been changed.
    fn repo() -> tempfile::TempDir {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src");
        std::fs::create_dir(&src).unwrap();
        std::fs::write(src.join("a.txt"), "one\n").unwrap();
        git(tmp.path(), &["init", "--quiet"]);
        git(tmp.path(), &["add", "."]);
        git(tmp.path(), &["commit", "--quiet", "-m", "init"]);
        std::fs::write(src.join("a.txt"), "two\n").unwrap();
        tmp
    }

    async fn json(response: Response) -> serde_json::Value {
        serde_json::from_slice(&body(response).await).unwrap()
    }

    #[tokio::test]
    async fn tree_lists_the_review() {
        let tmp = repo();

        let response = get_in(tmp.path(), "/api/tree").await;

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(header_of(&response, header::CACHE_CONTROL), "no-store");
        let tree = json(response).await;
        let file = &tree["children"][0]["children"][0];
        assert_eq!(file["path"], "src/a.txt");
        assert_eq!(file["status"], "modified");
    }

    #[tokio::test]
    async fn diff_returns_hunks() {
        let tmp = repo();

        let response = get_in(tmp.path(), "/api/diff?path=src%2Fa.txt").await;

        assert_eq!(response.status(), StatusCode::OK);
        let diff = json(response).await;
        let lines = &diff["hunks"][0]["lines"];
        assert_eq!(lines[0]["kind"], "removed");
        assert_eq!(lines[0]["text"], "one");
        assert_eq!(lines[1]["kind"], "added");
        assert_eq!(lines[1]["newLine"], 1);
    }

    #[tokio::test]
    async fn diff_of_an_unknown_path_is_not_found() {
        let tmp = repo();

        let response = get_in(tmp.path(), "/api/diff?path=nope.txt").await;

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn diff_without_a_path_is_a_bad_request() {
        assert_eq!(get("/api/diff").await.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn tree_outside_a_repository_is_a_server_error() {
        let tmp = tempfile::tempdir().unwrap();

        let response = get_in(tmp.path(), "/api/tree").await;

        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
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
