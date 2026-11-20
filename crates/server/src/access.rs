//! The access token that is needed to use vugent when it listens beyond loopback (ADR 0009).

use std::fmt;
use std::sync::Arc;

use axum::Router;
use axum::extract::{Request, State};
use axum::http::{HeaderValue, StatusCode, header};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use subtle::ConstantTimeEq;

/// A random token, and the name of the cookie that keeps it in the browser.
#[derive(Clone)]
pub struct Token {
    value: String,
    cookie: String,
}

impl Token {
    /// A new random token for vugent listening on `port`. The port goes into the cookie's name, since cookies are not
    /// kept apart by port and two vugents on one machine would otherwise overwrite each other's.
    pub fn generate(port: u16) -> std::io::Result<Self> {
        let mut bytes = [0u8; 32];
        getrandom::fill(&mut bytes).map_err(|err| std::io::Error::other(err.to_string()))?;
        let value = bytes.iter().map(|byte| format!("{byte:02x}")).collect();
        Ok(Self::new(value, port))
    }

    fn new(value: String, port: u16) -> Self {
        Self {
            value,
            cookie: format!("vugent-token-{port}"),
        }
    }

    /// The token, to put in the URL as `?token=`.
    pub fn as_str(&self) -> &str {
        &self.value
    }

    /// Whether `given` is the token, compared in constant time.
    fn matches(&self, given: &str) -> bool {
        given.as_bytes().ct_eq(self.value.as_bytes()).into()
    }
}

impl fmt::Debug for Token {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Token")
            .field("cookie", &self.cookie)
            .finish_non_exhaustive()
    }
}

/// Makes every route of `router`, its fallback included, need `token`: either `?token=` in the URL, which sets the
/// cookie and redirects to the same URL without it, or the cookie.
pub fn require_token(router: Router, token: Token) -> Router {
    router.layer(middleware::from_fn_with_state(Arc::new(token), check))
}

async fn check(State(token): State<Arc<Token>>, request: Request, next: Next) -> Response {
    if let Some((given, rest)) = request.uri().query().and_then(take_token) {
        if !token.matches(given) {
            return unauthorized();
        }
        let mut location = request.uri().path().to_owned();
        if !rest.is_empty() {
            location.push('?');
            location.push_str(&rest);
        }
        let cookie = format!(
            "{}={}; HttpOnly; SameSite=Lax; Path=/",
            token.cookie, token.value
        );
        let (Ok(location), Ok(cookie)) = (
            HeaderValue::from_str(&location),
            HeaderValue::from_str(&cookie),
        ) else {
            return StatusCode::BAD_REQUEST.into_response();
        };
        return (
            StatusCode::SEE_OTHER,
            [
                (header::LOCATION, location),
                (header::SET_COOKIE, cookie),
                (header::CACHE_CONTROL, HeaderValue::from_static("no-store")),
            ],
        )
            .into_response();
    }
    if has_cookie(&request, &token) {
        next.run(request).await
    } else {
        unauthorized()
    }
}

/// Splits `query` into the value of its `token` parameter, if it has one, and the rest of the query.
fn take_token(query: &str) -> Option<(&str, String)> {
    let mut token = None;
    let mut rest = Vec::new();
    for pair in query.split('&') {
        match pair.strip_prefix("token=") {
            Some(value) => token = Some(value),
            None if pair.is_empty() => {}
            None => rest.push(pair),
        }
    }
    token.map(|token| (token, rest.join("&")))
}

/// Whether `request` carries the token's cookie with the right value.
fn has_cookie(request: &Request, token: &Token) -> bool {
    request
        .headers()
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(';'))
        .filter_map(|pair| pair.trim().split_once('='))
        .any(|(name, value)| name == token.cookie && token.matches(value))
}

fn unauthorized() -> Response {
    (
        StatusCode::UNAUTHORIZED,
        [(header::CACHE_CONTROL, "no-store")],
        "vugent needs its access token: open the URL that vugent printed when it started.\n",
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::routing::get;
    use tower::ServiceExt;

    const TOKEN: &str = "0123abcd";

    fn app() -> Router {
        let router = Router::new()
            .route("/api/tree", get(|| async { "tree" }))
            .route("/api/events", get(|| async { "events" }))
            .fallback(|| async { "page" });
        require_token(router, Token::new(TOKEN.to_owned(), 8000))
    }

    async fn send(request: axum::http::request::Builder) -> Response {
        app()
            .oneshot(request.body(Body::empty()).unwrap())
            .await
            .unwrap()
    }

    async fn get_with_cookie(path: &str, cookie: &str) -> Response {
        send(Request::get(path).header(header::COOKIE, cookie)).await
    }

    fn header_of<'a>(response: &'a Response, name: &header::HeaderName) -> &'a str {
        response.headers()[name].to_str().unwrap()
    }

    #[test]
    fn generated_tokens_are_long_random_hex() {
        let first = Token::generate(8000).unwrap();
        let second = Token::generate(8000).unwrap();
        assert_eq!(first.as_str().len(), 64);
        assert!(first.as_str().bytes().all(|byte| byte.is_ascii_hexdigit()));
        assert_ne!(first.as_str(), second.as_str());
        assert_eq!(first.cookie, "vugent-token-8000");
    }

    #[test]
    fn debug_hides_the_token() {
        let token = Token::new(TOKEN.to_owned(), 8000);
        assert!(!format!("{token:?}").contains(TOKEN));
    }

    #[tokio::test]
    async fn everything_needs_the_token() {
        for path in ["/", "/assets/index.js", "/api/tree", "/api/events"] {
            let response = send(Request::get(path)).await;
            assert_eq!(response.status(), StatusCode::UNAUTHORIZED, "{path}");
            assert_eq!(header_of(&response, &header::CACHE_CONTROL), "no-store");
        }
    }

    #[tokio::test]
    async fn the_token_in_the_url_sets_the_cookie_and_redirects_without_it() {
        let response = send(Request::get(format!("/?token={TOKEN}"))).await;

        assert_eq!(response.status(), StatusCode::SEE_OTHER);
        assert_eq!(header_of(&response, &header::LOCATION), "/");
        assert_eq!(
            header_of(&response, &header::SET_COOKIE),
            format!("vugent-token-8000={TOKEN}; HttpOnly; SameSite=Lax; Path=/")
        );
    }

    #[tokio::test]
    async fn the_redirect_keeps_the_rest_of_the_query() {
        let response = send(Request::get(format!(
            "/api/tree?rev=index&token={TOKEN}&x=1"
        )))
        .await;

        assert_eq!(response.status(), StatusCode::SEE_OTHER);
        assert_eq!(
            header_of(&response, &header::LOCATION),
            "/api/tree?rev=index&x=1"
        );
    }

    #[tokio::test]
    async fn a_wrong_token_in_the_url_is_unauthorized() {
        for query in [
            "token=0123abce",
            "token=0123abc",
            "token=",
            "token=0123abcd0",
        ] {
            let response = send(Request::get(format!("/?{query}"))).await;
            assert_eq!(response.status(), StatusCode::UNAUTHORIZED, "{query}");
            assert!(!response.headers().contains_key(header::SET_COOKIE));
        }
    }

    #[tokio::test]
    async fn the_cookie_lets_requests_through() {
        for (path, expected) in [
            ("/", "page"),
            ("/api/tree", "tree"),
            ("/api/events", "events"),
        ] {
            let response = get_with_cookie(path, &format!("vugent-token-8000={TOKEN}")).await;
            assert_eq!(response.status(), StatusCode::OK, "{path}");
            let body = axum::body::to_bytes(response.into_body(), usize::MAX)
                .await
                .unwrap();
            assert_eq!(body, expected.as_bytes());
        }
    }

    #[tokio::test]
    async fn the_cookie_is_found_among_others() {
        let cookie = format!("theme=dark; vugent-token-8000={TOKEN}; other=1");
        let response = get_with_cookie("/", &cookie).await;
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn a_wrong_cookie_is_unauthorized() {
        for cookie in [
            "vugent-token-8000=0123abce".to_owned(),
            format!("vugent-token-9000={TOKEN}"),
            format!("vugent-token-8000{TOKEN}"),
        ] {
            let response = get_with_cookie("/", &cookie).await;
            assert_eq!(response.status(), StatusCode::UNAUTHORIZED, "{cookie}");
        }
    }

    #[test]
    fn take_token_splits_the_query() {
        assert_eq!(take_token("rev=index"), None);
        assert_eq!(take_token("token=abc"), Some(("abc", String::new())));
        assert_eq!(
            take_token("a=1&&token=abc&b=2"),
            Some(("abc", "a=1&b=2".to_owned()))
        );
        assert_eq!(take_token("tokens=abc"), None);
    }
}
