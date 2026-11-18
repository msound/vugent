//! The HTTP server behind vugent's browser UI.

use axum::Router;
use axum::routing::get;
use tokio::net::TcpListener;

/// Builds the application's routes.
pub fn router() -> Router {
    Router::new().route("/api/health", get(health))
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
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    #[tokio::test]
    async fn health_returns_ok() {
        let response = router()
            .oneshot(Request::get("/api/health").body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        assert_eq!(&body[..], b"ok");
    }
}
