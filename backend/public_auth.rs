//! Small juha.no-only identity and local-file gateway. No connection to Revontuli.
use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, Path, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use jsonwebtoken::{Algorithm, DecodingKey, Validation, decode, decode_header};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::Mutex;

const SESSION: &str = "__Secure-gaia-session";
const CHALLENGE: &str = "__Secure-gaia-challenge";
#[derive(Clone)]
struct App {
    client_id: String,
    allowed: HashSet<String>,
    root: PathBuf,
    http: reqwest::Client,
    sessions: Arc<Mutex<HashMap<String, (Instant, String)>>>,
    challenges: Arc<Mutex<HashMap<String, Instant>>>,
    certs: Arc<Mutex<Option<(Instant, HashMap<String, String>)>>>,
}
#[derive(Clone, Deserialize, Serialize)]
struct Claims {
    sub: String,
    email: String,
    email_verified: bool,
    nonce: Option<String>,
    exp: u64,
}
#[derive(Deserialize)]
struct Login {
    credential: String,
}
type Error = (StatusCode, &'static str);
fn denied() -> Error {
    (
        StatusCode::UNAUTHORIZED,
        "Sign in with Google to access these images",
    )
}
fn cookie(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get(header::COOKIE)?
        .to_str()
        .ok()?
        .split(';')
        .find_map(|c| {
            c.trim()
                .strip_prefix(&format!("{name}="))
                .map(str::to_owned)
        })
}
fn same_origin(headers: &HeaderMap) -> bool {
    headers.get(header::ORIGIN).and_then(|h| h.to_str().ok()) == Some("https://juha.no")
}
fn set_cookie(r: &mut Response, name: &str, value: &str, age: u64) {
    r.headers_mut().append(
        header::SET_COOKIE,
        HeaderValue::from_str(&format!(
            "{name}={value}; Path=/gaia/; Secure; HttpOnly; SameSite=Lax; Max-Age={age}"
        ))
        .unwrap(),
    );
}
fn private(mut r: Response) -> Response {
    r.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, no-store"),
    );
    r.headers_mut()
        .insert(header::VARY, HeaderValue::from_static("Cookie"));
    r.headers_mut().insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    r
}
async fn identity(s: &App, h: &HeaderMap) -> Option<String> {
    let key = cookie(h, SESSION)?;
    let mut rows = s.sessions.lock().await;
    rows.retain(|_, (t, _)| t.elapsed() < Duration::from_secs(28800));
    rows.get(&key).map(|(_, email)| email.clone())
}
async fn session(State(s): State<App>, h: HeaderMap) -> Response {
    let email = identity(&s, &h).await;
    let authorized = email.as_ref().is_some_and(|v| s.allowed.contains(v));
    let mut c = s.challenges.lock().await;
    c.retain(|_, t| t.elapsed() < Duration::from_secs(600));
    let nonce = cookie(&h, CHALLENGE)
        .filter(|n| c.contains_key(n))
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    if c.len() >= 1000 && !c.contains_key(&nonce) {
        return private((StatusCode::TOO_MANY_REQUESTS, "Please try again shortly").into_response());
    }
    c.entry(nonce.clone()).or_insert_with(Instant::now);
    let mut r = private(
        Json(json!({"email":email,"authorized":authorized,"client_id":s.client_id,"nonce":nonce}))
            .into_response(),
    );
    set_cookie(&mut r, CHALLENGE, &nonce, 600);
    r
}
async fn certificates(s: &App, refresh: bool) -> Result<HashMap<String, String>, Error> {
    let mut c = s.certs.lock().await;
    if let Some((t, keys)) = &*c {
        if t.elapsed() < Duration::from_secs(3600)
            && (!refresh || t.elapsed() < Duration::from_secs(30))
        {
            return Ok(keys.clone());
        }
    }
    let response = s
        .http
        .get("https://www.googleapis.com/oauth2/v1/certs")
        .send()
        .await
        .map_err(|_| (StatusCode::SERVICE_UNAVAILABLE, "Google keys unavailable"))?
        .error_for_status()
        .map_err(|_| (StatusCode::SERVICE_UNAVAILABLE, "Google keys unavailable"))?;
    let keys = response
        .json::<HashMap<String, String>>()
        .await
        .map_err(|_| denied())?;
    *c = Some((Instant::now(), keys.clone()));
    Ok(keys)
}
async fn login(
    State(s): State<App>,
    h: HeaderMap,
    Json(input): Json<Login>,
) -> Result<Response, Error> {
    if !same_origin(&h) {
        return Err((StatusCode::FORBIDDEN, "Invalid login origin"));
    }
    if input.credential.len() > 16000 {
        return Err(denied());
    }
    let nonce = cookie(&h, CHALLENGE).ok_or_else(denied)?;
    let challenge = s
        .challenges
        .lock()
        .await
        .remove(&nonce)
        .ok_or_else(denied)?;
    if challenge.elapsed() > Duration::from_secs(600) {
        return Err(denied());
    }
    let jwt = decode_header(&input.credential).map_err(|_| denied())?;
    if jwt.alg != Algorithm::RS256 {
        return Err(denied());
    }
    let kid = jwt.kid.ok_or_else(denied)?;
    let mut certs = certificates(&s, false).await?;
    if !certs.contains_key(&kid) {
        certs = certificates(&s, true).await?
    }
    let key = DecodingKey::from_rsa_pem(certs.get(&kid).ok_or_else(denied)?.as_bytes())
        .map_err(|_| denied())?;
    let mut v = Validation::new(Algorithm::RS256);
    v.set_audience(&[s.client_id.as_str()]);
    v.set_issuer(&["accounts.google.com", "https://accounts.google.com"]);
    v.leeway = 30;
    let claims = decode::<Claims>(&input.credential, &key, &v)
        .map_err(|_| denied())?
        .claims;
    if !claims.email_verified || claims.sub.is_empty() || claims.nonce.as_deref() != Some(&nonce) {
        return Err(denied());
    }
    let email = claims.email.to_ascii_lowercase();
    let authorized = s.allowed.contains(&email);
    let mut sessions = s.sessions.lock().await;
    sessions.retain(|_, (t, _)| t.elapsed() < Duration::from_secs(28800));
    if sessions.len() >= 1000 {
        return Err((StatusCode::TOO_MANY_REQUESTS, "Please try later"));
    }
    if let Some(old) = cookie(&h, SESSION) {
        sessions.remove(&old);
    }
    let token = uuid::Uuid::new_v4().to_string();
    sessions.insert(token.clone(), (Instant::now(), email.clone()));
    let mut r = private(Json(json!({"email":email,"authorized":authorized})).into_response());
    set_cookie(&mut r, SESSION, &token, 28800);
    set_cookie(&mut r, CHALLENGE, "", 0);
    Ok(r)
}
async fn logout(State(s): State<App>, h: HeaderMap) -> Result<Response, Error> {
    if !same_origin(&h) {
        return Err((StatusCode::FORBIDDEN, "Invalid logout origin"));
    }
    if let Some(token) = cookie(&h, SESSION) {
        s.sessions.lock().await.remove(&token);
    }
    let mut r = private(StatusCode::NO_CONTENT.into_response());
    set_cookie(&mut r, SESSION, "", 0);
    Ok(r)
}
fn safe_path(path: &str) -> bool {
    matches!(path, "manifest.json" | "archive-manifest.json")
        || path.strip_prefix("assets/").is_some_and(|n| {
            !n.is_empty()
                && !n.starts_with('.')
                && n.bytes()
                    .all(|c| c.is_ascii_alphanumeric() || b"-_.".contains(&c))
                && !n.contains("..")
        })
}
async fn asset(
    State(s): State<App>,
    h: HeaderMap,
    Path(path): Path<String>,
) -> Result<Response, Error> {
    let email = identity(&s, &h).await.ok_or_else(denied)?;
    if !s.allowed.contains(&email) {
        return Err((
            StatusCode::FORBIDDEN,
            "This Google account is not authorized for Starvisor imagery",
        ));
    }
    if !safe_path(&path) {
        return Err((StatusCode::NOT_FOUND, "Not found"));
    }
    let dest = tokio::fs::canonicalize(s.root.join(&path))
        .await
        .map_err(|_| (StatusCode::NOT_FOUND, "Not found"))?;
    if !dest.starts_with(&s.root) {
        return Err((StatusCode::NOT_FOUND, "Not found"));
    }
    let mut bytes = tokio::fs::read(dest)
        .await
        .map_err(|_| (StatusCode::NOT_FOUND, "Not found"))?;
    let mime = if path.ends_with(".json") {
        let mut m: serde_json::Value = serde_json::from_slice(&bytes)
            .map_err(|_| (StatusCode::SERVICE_UNAVAILABLE, "Publication unavailable"))?;
        rewrite(&mut m);
        bytes = serde_json::to_vec(&m).unwrap();
        "application/json"
    } else if path.ends_with(".png") {
        "image/png"
    } else if path.ends_with(".jpg") || path.ends_with(".jpeg") {
        "image/jpeg"
    } else if path.ends_with(".webp") {
        "image/webp"
    } else {
        "application/octet-stream"
    };
    let mut r = private(bytes.into_response());
    r.headers_mut()
        .insert(header::CONTENT_TYPE, HeaderValue::from_static(mime));
    Ok(r)
}
fn rewrite(v: &mut serde_json::Value) {
    match v {
        serde_json::Value::String(s) => {
            if s.starts_with("/gaia/public/") {
                *s = s.replacen("/gaia/public/", "/gaia/restricted/", 1)
            }
        }
        serde_json::Value::Array(a) => {
            for x in a {
                rewrite(x)
            }
        }
        serde_json::Value::Object(o) => {
            for x in o.values_mut() {
                rewrite(x)
            }
        }
        _ => {}
    }
}
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let client_id = std::env::var("GAIA_GOOGLE_CLIENT_ID")?;
    anyhow::ensure!(!client_id.is_empty(), "Missing Google client ID");
    let allowed = std::fs::read_to_string(std::env::var("GAIA_AUTH_ALLOWLIST")?)?
        .lines()
        .map(str::trim)
        .filter(|v| !v.is_empty() && !v.starts_with('#'))
        .map(str::to_ascii_lowercase)
        .collect::<HashSet<_>>();
    anyhow::ensure!(!allowed.is_empty(), "Empty allowlist");
    let root = std::fs::canonicalize(
        std::env::var("GAIA_AUTH_ASSET_ROOT").unwrap_or("/var/www/gaia-public".into()),
    )?;
    let s = App {
        client_id,
        allowed,
        root,
        http: reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .build()?,
        sessions: Default::default(),
        challenges: Default::default(),
        certs: Default::default(),
    };
    let app = Router::new()
        .route("/gaia/auth/session", get(session))
        .route("/gaia/auth/google", post(login))
        .route("/gaia/auth/logout", post(logout))
        .route("/gaia/restricted/{*path}", get(asset))
        .layer(DefaultBodyLimit::max(20000))
        .with_state(s);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:18766").await?;
    axum::serve(listener, app).await?;
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn paths_cannot_escape() {
        for p in [
            "../a",
            "assets/../a",
            "assets/%2e%2e/a",
            "assets/a/b",
            "assets/.secret",
        ] {
            assert!(!safe_path(p));
        }
        assert!(safe_path("assets/source-abc.png"));
        assert!(safe_path("manifest.json"));
    }
    #[test]
    fn origin_exact() {
        let mut h = HeaderMap::new();
        assert!(!same_origin(&h));
        h.insert(
            header::ORIGIN,
            HeaderValue::from_static("https://juha.no.evil.test"),
        );
        assert!(!same_origin(&h));
        h.insert(header::ORIGIN, HeaderValue::from_static("https://juha.no"));
        assert!(same_origin(&h));
    }
    #[test]
    fn restricted_urls() {
        let mut m = json!({"images":[{"texture_url":"/gaia/public/assets/a.webp"}]});
        rewrite(&mut m);
        assert_eq!(
            m["images"][0]["texture_url"],
            "/gaia/restricted/assets/a.webp"
        );
    }

    #[tokio::test]
    async fn assets_require_an_allowed_server_session() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("manifest.json"),
            br#"{"geometry_url":"/gaia/public/assets/shell.bin"}"#,
        )
        .unwrap();
        let s = App {
            client_id: "test".into(),
            allowed: HashSet::from(["allowed@example.test".into()]),
            root: dir.path().to_owned(),
            http: reqwest::Client::new(),
            sessions: Default::default(),
            challenges: Default::default(),
            certs: Default::default(),
        };
        assert_eq!(
            asset(
                State(s.clone()),
                HeaderMap::new(),
                Path("manifest.json".into())
            )
            .await
            .unwrap_err()
            .0,
            StatusCode::UNAUTHORIZED
        );
        let mut h = HeaderMap::new();
        h.insert(
            header::COOKIE,
            HeaderValue::from_static("__Secure-gaia-session=forged"),
        );
        assert_eq!(
            asset(State(s.clone()), h.clone(), Path("manifest.json".into()))
                .await
                .unwrap_err()
                .0,
            StatusCode::UNAUTHORIZED
        );
        s.sessions.lock().await.insert(
            "forged".into(),
            (Instant::now(), "other@example.test".into()),
        );
        assert_eq!(
            asset(State(s.clone()), h.clone(), Path("manifest.json".into()))
                .await
                .unwrap_err()
                .0,
            StatusCode::FORBIDDEN
        );
        s.sessions.lock().await.insert(
            "forged".into(),
            (Instant::now(), "allowed@example.test".into()),
        );
        let r = asset(State(s), h, Path("manifest.json".into()))
            .await
            .unwrap();
        assert_eq!(r.status(), StatusCode::OK);
        assert_eq!(r.headers()[header::CACHE_CONTROL], "private, no-store");
    }
    #[tokio::test]
    async fn forged_google_credential_never_creates_session() {
        let s = App {
            client_id: "test".into(),
            allowed: HashSet::new(),
            root: PathBuf::from("/tmp"),
            http: reqwest::Client::new(),
            sessions: Default::default(),
            challenges: Default::default(),
            certs: Default::default(),
        };
        s.challenges
            .lock()
            .await
            .insert("challenge".into(), Instant::now());
        let mut h = HeaderMap::new();
        h.insert(header::ORIGIN, HeaderValue::from_static("https://juha.no"));
        h.insert(
            header::COOKIE,
            HeaderValue::from_static("__Secure-gaia-challenge=challenge"),
        );
        assert_eq!(
            login(
                State(s.clone()),
                h,
                Json(Login {
                    credential: "fake.token.value".into()
                })
            )
            .await
            .unwrap_err()
            .0,
            StatusCode::UNAUTHORIZED
        );
        assert!(s.sessions.lock().await.is_empty());
    }
}
