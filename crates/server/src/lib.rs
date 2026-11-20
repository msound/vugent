//! The HTTP server behind vugent's browser UI.

mod access;
mod events;
mod tree;

use std::borrow::Cow;
use std::sync::Arc;

use axum::extract::{FromRef, Path, Query, State};
use axum::http::{StatusCode, Uri, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use rust_embed::RustEmbed;
use serde::{Deserialize, Serialize};
use tokio::net::TcpListener;
use vugent_git::{Rev, Review};

pub use access::{Token, require_token};
pub use events::Events;

/// The built frontend, which `npm run build` in `web/` writes to this crate's `frontend/` folder. In debug builds
/// rust-embed reads it from disk at run time; in release builds it is compiled in. It may be missing when the frontend
/// has not been built (see `build.rs`).
#[derive(RustEmbed)]
#[folder = "frontend/"]
#[allow_missing = true]
struct Frontend;

/// What the handlers share.
#[derive(Clone)]
struct AppState {
    review: Arc<Review>,
    events: Events,
}

impl FromRef<AppState> for Arc<Review> {
    fn from_ref(state: &AppState) -> Self {
        state.review.clone()
    }
}

impl FromRef<AppState> for Events {
    fn from_ref(state: &AppState) -> Self {
        state.events.clone()
    }
}

/// Builds the application's routes, serving `review` and streaming `events`, which should be of the same folder.
pub fn router(review: Review, events: Events) -> Router {
    Router::new()
        .route("/api/health", get(health))
        .route("/api/version", get(version))
        .route("/api/commits", get(commits))
        .route("/api/commit", get(commit))
        .route("/api/tree", get(tree))
        .route("/api/diff", get(diff))
        .route("/api/highlight/{id}", get(highlight))
        .route("/api/events", get(events::events))
        .fallback(frontend)
        .with_state(AppState {
            review: Arc::new(review),
            events,
        })
}

/// Serves `router` on `listener` until Ctrl-C is pressed, then closes the streams of `events`.
pub async fn serve(listener: TcpListener, router: Router, events: Events) -> std::io::Result<()> {
    axum::serve(listener, router)
        .with_graceful_shutdown(async move {
            shutdown_signal().await;
            events.close();
        })
        .await
}

async fn health() -> &'static str {
    "ok"
}

/// What vugent's build is, from `git describe` at build time (see `build.rs`).
const VERSION: &str = env!("VUGENT_VERSION");

#[derive(Debug, Serialize)]
struct Version {
    version: &'static str,
}

/// `GET /api/version`: which build of vugent is running, for the logo panel.
async fn version() -> Json<Version> {
    Json(Version { version: VERSION })
}

/// `GET /api/commits`: what can be reviewed, for the sidebar.
async fn commits(State(review): State<Arc<Review>>) -> Response {
    let result = tokio::task::spawn_blocking(move || review.history()).await;
    json_response(result)
}

#[derive(Debug, Deserialize)]
struct CommitQuery {
    rev: String,
}

/// `GET /api/commit?rev=<sha>`: a commit's message, trailers, author and committer, for the detail page. Only a full
/// SHA is taken, so the answer never changes and the browser may keep it.
async fn commit(State(review): State<Arc<Review>>, Query(query): Query<CommitQuery>) -> Response {
    let sha = query.rev;
    if !is_full_sha(&sha) {
        return (
            StatusCode::BAD_REQUEST,
            format!("{sha:?} is not a commit's full SHA"),
        )
            .into_response();
    }
    let result = tokio::task::spawn_blocking(move || review.commit_details(&sha)).await;
    cached_json_response(result, "private, max-age=31536000, immutable")
}

#[derive(Debug, Deserialize)]
struct TreeQuery {
    rev: Option<String>,
}

/// `GET /api/tree?rev=<rev>`: every folder and file of the review `rev`, with how each file changed.
async fn tree(State(review): State<Arc<Review>>, Query(query): Query<TreeQuery>) -> Response {
    let rev = match parse_rev(query.rev.as_deref()) {
        Ok(rev) => rev,
        Err(message) => return (StatusCode::BAD_REQUEST, message).into_response(),
    };
    let result = tokio::task::spawn_blocking(move || {
        let files = review.files(&rev)?;
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
    rev: Option<String>,
}

/// `GET /api/diff?path=<path>&rev=<rev>&v=<version>`: the diff of one file of the review `rev`. A commit's diff,
/// given its full SHA, never changes, so the browser may keep it; `v` is ignored, and only keeps an upgraded vugent
/// from being served old answers.
async fn diff(State(review): State<Arc<Review>>, Query(query): Query<DiffQuery>) -> Response {
    let rev = match parse_rev(query.rev.as_deref()) {
        Ok(rev) => rev,
        Err(message) => return (StatusCode::BAD_REQUEST, message).into_response(),
    };
    let cache = match &rev {
        Rev::Commit(sha) if is_full_sha(sha) => "private, max-age=31536000, immutable",
        _ => "no-store",
    };
    let result = tokio::task::spawn_blocking(move || {
        let diff = review.diff(&rev, &query.path)?;
        let highlight = diff.whole && !diff.binary;
        Ok(Diff { diff, highlight })
    })
    .await;
    cached_json_response(result, cache)
}

/// A file's diff, as the diff panel gets it.
#[derive(Debug, Serialize)]
struct Diff {
    #[serde(flatten)]
    diff: vugent_git::FileDiff,
    /// Whether the browser should fetch the highlighting of each side with a blob ID.
    highlight: bool,
}

#[derive(Debug, Deserialize)]
struct HighlightQuery {
    path: String,
}

/// A file's lines, highlighted.
#[derive(Debug, Serialize)]
struct Highlighted {
    lines: Vec<Vec<vugent_highlight::Piece>>,
}

/// The largest file that is highlighted, as large as a file diffed whole (ADR 0010).
const HIGHLIGHT_LIMIT: usize = 1024 * 1024;

/// `GET /api/highlight/<id>?path=<path>&v=<version>`: the lines of the blob `id`, which `/api/diff` gave for the
/// file at `path`, highlighted with the grammar its path picks. A blob never changes, so the browser may keep it;
/// `v` is ignored, and only keeps an upgraded vugent, whose grammars may differ, from being served old answers. A
/// working tree file that changed since its diff was fetched is not found.
async fn highlight(
    State(review): State<Arc<Review>>,
    Path(id): Path<String>,
    Query(query): Query<HighlightQuery>,
) -> Response {
    let result = tokio::task::spawn_blocking(move || {
        let content = review.content(&id, &query.path)?;
        if content.len() > HIGHLIGHT_LIMIT {
            return Ok(None);
        }
        let lines = vugent_highlight::highlight_file(&query.path, &content);
        Ok(Some(Highlighted { lines }))
    })
    .await;
    let result = match result {
        Ok(Ok(None)) => {
            return (
                StatusCode::PAYLOAD_TOO_LARGE,
                [(header::CACHE_CONTROL, "no-store")],
                "the file is too large to highlight",
            )
                .into_response();
        }
        Ok(Ok(Some(highlighted))) => Ok(Ok(highlighted)),
        Ok(Err(error)) => Ok(Err(error)),
        Err(error) => Err(error),
    };
    cached_json_response(result, "private, max-age=31536000, immutable")
}

/// Whether `sha` is a commit's whole SHA-1 or SHA-256 in lowercase hex, rather than an abbreviation that may come to
/// name another commit.
fn is_full_sha(sha: &str) -> bool {
    matches!(sha.len(), 40 | 64)
        && sha
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

/// Reads the `rev` parameter, which defaults to the working tree. The error is the message for 400 Bad Request.
fn parse_rev(rev: Option<&str>) -> Result<Rev, String> {
    match rev {
        None => Ok(Rev::WorkTree),
        Some(rev) => Rev::parse(rev).ok_or_else(|| {
            format!(
                "{rev:?} is not a review: use worktree, index, unstaged, branch or a commit SHA"
            )
        }),
    }
}

/// Answers with `result` as JSON, or with the error as plain text.
fn json_response<T: serde::Serialize>(
    result: Result<Result<T, vugent_git::Error>, tokio::task::JoinError>,
) -> Response {
    cached_json_response(result, "no-store")
}

/// Answers with `result` as JSON, cached as `cache` says, or with the error as plain text, never cached.
fn cached_json_response<T: serde::Serialize>(
    result: Result<Result<T, vugent_git::Error>, tokio::task::JoinError>,
    cache: &'static str,
) -> Response {
    let error = match result {
        Ok(Ok(value)) => {
            return ([(header::CACHE_CONTROL, cache)], Json(value)).into_response();
        }
        Ok(Err(error)) => error,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };
    let status = match error {
        vugent_git::Error::NotFound(_) => StatusCode::NOT_FOUND,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    };
    (
        status,
        [(header::CACHE_CONTROL, "no-store")],
        error_message(&error),
    )
        .into_response()
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
        let review = Review::new(dir);
        router(review.clone(), Events::new(review, true))
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
    async fn version_returns_the_build_version() {
        let response = get("/api/version").await;

        assert_eq!(response.status(), StatusCode::OK);
        let json: serde_json::Value = serde_json::from_slice(&body(response).await).unwrap();
        assert_eq!(json, serde_json::json!({ "version": VERSION }));
        assert!(
            VERSION.starts_with('v'),
            "{VERSION:?} does not start with v"
        );
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
    pub(crate) fn repo() -> tempfile::TempDir {
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
    async fn highlights_each_side_of_a_diff() {
        let tmp = repo();
        std::fs::write(tmp.path().join("src/a.txt"), "two\nthree\n").unwrap();
        let diff = json(get_in(tmp.path(), "/api/diff?path=src%2Fa.txt").await).await;
        assert_eq!(diff["highlight"], true);
        let (old, new) = (
            diff["oldId"].as_str().unwrap(),
            diff["newId"].as_str().unwrap(),
        );

        let old = get_in(
            tmp.path(),
            &format!("/api/highlight/{old}?path=src%2Fa.txt&v=1"),
        )
        .await;
        let new = get_in(
            tmp.path(),
            &format!("/api/highlight/{new}?path=src%2Fa.txt&v=1"),
        )
        .await;

        assert_eq!(old.status(), StatusCode::OK);
        assert!(header_of(&old, header::CACHE_CONTROL).contains("immutable"));
        assert_eq!(
            json(old).await,
            serde_json::json!({ "lines": [[{ "text": "one" }]] })
        );
        assert_eq!(
            json(new).await["lines"][1],
            serde_json::json!([{ "text": "three" }])
        );
    }

    #[tokio::test]
    async fn highlights_with_the_grammar_of_the_path_until_the_file_changes() {
        let tmp = repo();
        std::fs::write(tmp.path().join("src/b.rs"), "fn main() {}\n").unwrap();
        let diff = json(get_in(tmp.path(), "/api/diff?path=src%2Fb.rs").await).await;
        let new = diff["newId"].as_str().unwrap();
        let url = format!("/api/highlight/{new}?path=src%2Fb.rs");

        let rust = json(get_in(tmp.path(), &url).await).await;
        std::fs::write(tmp.path().join("src/b.rs"), "fn changed() {}\n").unwrap();
        let changed = get_in(tmp.path(), &url).await;

        assert_eq!(
            rust["lines"][0][0],
            serde_json::json!({ "text": "fn", "class": "keyword" })
        );
        assert_eq!(changed.status(), StatusCode::NOT_FOUND);
        assert_eq!(header_of(&changed, header::CACHE_CONTROL), "no-store");
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
        // The working tree changes all the time.
        assert_eq!(header_of(&response, header::CACHE_CONTROL), "no-store");
        let diff = json(response).await;
        assert_eq!(diff["whole"], true);
        let lines = &diff["hunks"][0]["lines"];
        assert_eq!(lines[0]["kind"], "removed");
        assert_eq!(lines[0]["text"], "one");
        assert_eq!(lines[1]["kind"], "added");
        assert_eq!(lines[1]["newLine"], 1);
    }

    #[tokio::test]
    async fn commits_lists_the_history() {
        let tmp = repo();

        let response = get_in(tmp.path(), "/api/commits").await;

        assert_eq!(response.status(), StatusCode::OK);
        let history = json(response).await;
        assert_eq!(history["uncommitted"], true);
        assert_eq!(history["staged"], false);
        assert_eq!(history["unstaged"], true);
        assert_eq!(history["commits"][0]["subject"], "init");
        assert!(history["commits"][0]["shortSha"].is_string());
        assert!(history["base"].is_null());
        assert!(history["github"].is_null());
    }

    fn head(dir: &std::path::Path) -> String {
        let sha = std::process::Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(["rev-parse", "HEAD"])
            .output()
            .unwrap()
            .stdout;
        String::from_utf8(sha).unwrap().trim().to_owned()
    }

    #[tokio::test]
    async fn commit_returns_the_details_and_may_be_cached() {
        let tmp = repo();
        let sha = head(tmp.path());

        let response = get_in(tmp.path(), &format!("/api/commit?rev={sha}")).await;

        assert_eq!(response.status(), StatusCode::OK);
        assert!(header_of(&response, header::CACHE_CONTROL).contains("immutable"));
        let details = json(response).await;
        assert_eq!(details["sha"], sha.as_str());
        assert_eq!(details["subject"], "init");
        assert_eq!(details["author"]["name"], "Test");
        assert!(details["trailers"].as_array().unwrap().is_empty());
    }

    #[tokio::test]
    async fn commit_takes_only_a_full_sha() {
        let tmp = repo();
        let sha = head(tmp.path());

        for rev in [&sha[..7], "HEAD", "worktree", &sha.to_uppercase()] {
            let response = get_in(tmp.path(), &format!("/api/commit?rev={rev}")).await;
            assert_eq!(response.status(), StatusCode::BAD_REQUEST, "{rev}");
        }
        let unknown = get_in(tmp.path(), &format!("/api/commit?rev={}", "0".repeat(40))).await;
        assert_eq!(unknown.status(), StatusCode::NOT_FOUND);
        assert_eq!(header_of(&unknown, header::CACHE_CONTROL), "no-store");
    }

    #[tokio::test]
    async fn tree_and_diff_take_a_commit() {
        let tmp = repo();
        let sha = std::process::Command::new("git")
            .arg("-C")
            .arg(tmp.path())
            .args(["rev-parse", "HEAD"])
            .output()
            .unwrap()
            .stdout;
        let sha = String::from_utf8(sha).unwrap();
        let sha = sha.trim();

        let tree = json(get_in(tmp.path(), &format!("/api/tree?rev={sha}")).await).await;
        let diff = get_in(
            tmp.path(),
            &format!("/api/diff?path=src%2Fa.txt&rev={sha}&v=1"),
        )
        .await;
        let short = get_in(
            tmp.path(),
            &format!("/api/diff?path=src%2Fa.txt&rev={}", &sha[..7]),
        )
        .await;

        assert_eq!(tree["children"][0]["children"][0]["status"], "added");
        // Only a full SHA always names the same commit.
        assert!(header_of(&diff, header::CACHE_CONTROL).contains("immutable"));
        assert_eq!(header_of(&short, header::CACHE_CONTROL), "no-store");
        let diff = json(diff).await;
        assert_eq!(diff["hunks"][0]["lines"][0]["text"], "one");
    }

    #[tokio::test]
    async fn the_index_has_no_unstaged_changes() {
        let tmp = repo();

        let tree = json(get_in(tmp.path(), "/api/tree?rev=index").await).await;

        assert_eq!(tree["children"][0]["children"][0]["status"], "unchanged");
    }

    #[tokio::test]
    async fn unstaged_shows_changes_since_the_index() {
        let tmp = repo();

        let tree = json(get_in(tmp.path(), "/api/tree?rev=unstaged").await).await;
        let diff = json(get_in(tmp.path(), "/api/diff?path=src%2Fa.txt&rev=unstaged").await).await;

        assert_eq!(tree["children"][0]["children"][0]["status"], "modified");
        assert_eq!(diff["hunks"][0]["lines"][0]["text"], "one");
    }

    #[tokio::test]
    async fn an_invalid_rev_is_a_bad_request() {
        let tmp = repo();

        for path in ["/api/tree?rev=HEAD", "/api/diff?path=src%2Fa.txt&rev=--all"] {
            let response = get_in(tmp.path(), path).await;
            assert_eq!(response.status(), StatusCode::BAD_REQUEST, "{path}");
        }
    }

    #[tokio::test]
    async fn an_unknown_commit_is_not_found() {
        let tmp = repo();

        let response = get_in(tmp.path(), &format!("/api/tree?rev={}", "0".repeat(40))).await;

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
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
