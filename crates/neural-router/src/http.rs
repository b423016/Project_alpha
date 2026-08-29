use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use axum::Router;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::{get, post};
use neural_router_config::Settings;
use neural_router_data::{ChainSnapshot, load_fixture};
use neural_router_domain::{Policy, Top20};
use neural_router_execution::{AlpacaOverlay, Blotter, DecideHist};
use serde::Serialize;

#[derive(Clone)]
pub struct AppState {
    pub snapshot: Arc<Mutex<Option<ChainSnapshot>>>,
    pub top20: Arc<Mutex<Option<Top20>>>,
    pub blotter: Arc<Mutex<Blotter>>,
    pub metrics: Arc<Mutex<DecideHist>>,
    pub killed: Arc<AtomicBool>,
    pub token: Option<String>,
    pub broker: Option<Arc<AlpacaOverlay>>,
    pub claude_configured: bool,
    pub paper: bool,
}

impl AppState {
    pub fn from_fixture() -> Self {
        let snap = load_fixture().ok();
        let top = snap.as_ref().map(|s| {
            let (t, ms) = neural_router_ml::decide_cpu_ms(s, &Policy::file_default());
            let mut h = DecideHist::default();
            h.record(ms as u64);
            (t, h)
        });
        let (top20, metrics) = match top {
            Some((t, h)) => (Some(t), h),
            None => (None, DecideHist::default()),
        };
        Self {
            snapshot: Arc::new(Mutex::new(snap)),
            top20: Arc::new(Mutex::new(top20)),
            blotter: Arc::new(Mutex::new(Blotter::default())),
            metrics: Arc::new(Mutex::new(metrics)),
            killed: Arc::new(AtomicBool::new(false)),
            token: None,
            broker: None,
            claude_configured: false,
            paper: true,
        }
    }

    pub fn from_settings(settings: &Settings) -> Self {
        let mut s = Self::from_fixture();
        s.token = settings.ui_token.clone();
        s.paper = settings.alpaca_paper;
        s.claude_configured = settings
            .anthropic_api_key
            .as_ref()
            .is_some_and(|k| !k.is_empty());
        s.broker = AlpacaOverlay::from_settings(settings).ok().map(Arc::new);
        s
    }

    pub fn inhibit(&self) -> bool {
        self.killed.load(Ordering::SeqCst)
    }
}

#[derive(Serialize)]
struct SnapshotBody {
    snapshot_id: String,
    delayed: bool,
    delayed_badge: &'static str,
    underlying: String,
    under_price: f64,
    n_contracts: usize,
    killed: bool,
    /// Feed label from the ingest source (fixture, yahoo-delayed, ...).
    source: String,
    asof_unix_ms: i64,
}

fn auth(state: &AppState, headers: &HeaderMap) -> Result<(), StatusCode> {
    let Some(expect) = &state.token else {
        return Ok(());
    };
    match headers.get("x-nr-token").and_then(|v| v.to_str().ok()) {
        Some(got) if got == expect => Ok(()),
        _ => Err(StatusCode::UNAUTHORIZED),
    }
}

async fn snapshot(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Response, StatusCode> {
    auth(&state, &headers)?;
    let guard = state
        .snapshot
        .lock()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let Some(snap) = guard.as_ref() else {
        return Ok((StatusCode::SERVICE_UNAVAILABLE, "STALE_DATA").into_response());
    };
    // Inhibit is kernel state, not market identity: fold it into ETag so a
    // kill cannot 304-serve a pre-kill body that still says armed.
    let etag = format!(
        "\"{}:{}\"",
        snap.stamps.snapshot_id.as_str(),
        if state.inhibit() { "k" } else { "a" }
    );
    if headers
        .get(header::IF_NONE_MATCH)
        .and_then(|v| v.to_str().ok())
        == Some(etag.as_str())
    {
        return Ok(StatusCode::NOT_MODIFIED.into_response());
    }
    let body = SnapshotBody {
        snapshot_id: snap.stamps.snapshot_id.as_str().into(),
        delayed: snap.stamps.delayed,
        delayed_badge: if snap.stamps.delayed {
            "DELAYED"
        } else {
            "LIVE"
        },
        underlying: snap.underlying.clone(),
        under_price: snap.under_price,
        n_contracts: snap.contracts.len(),
        killed: state.inhibit(),
        source: snap.stamps.source.clone(),
        asof_unix_ms: snap.stamps.asof_unix_ms,
    };
    let json = serde_json::to_vec(&body).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Response::builder()
        .status(StatusCode::OK)
        .header(header::ETAG, etag)
        .header(header::CACHE_CONTROL, "no-store")
        .header(header::CONTENT_TYPE, "application/json")
        .body(axum::body::Body::from(json))
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

async fn top20(State(state): State<AppState>, headers: HeaderMap) -> Result<Response, StatusCode> {
    auth(&state, &headers)?;
    let top = state
        .top20
        .lock()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let Some(top) = top.as_ref() else {
        return Ok((StatusCode::SERVICE_UNAVAILABLE, "STALE_DATA").into_response());
    };
    let etag = format!("\"{}\"", top.snapshot_id.as_str());
    let json = serde_json::to_vec(top).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Response::builder()
        .status(StatusCode::OK)
        .header(header::ETAG, etag)
        .header(header::CACHE_CONTROL, "max-age=0, private")
        .header(header::CONTENT_TYPE, "application/json")
        .body(axum::body::Body::from(json))
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

async fn chain(State(state): State<AppState>, headers: HeaderMap) -> Result<Response, StatusCode> {
    auth(&state, &headers)?;
    let guard = state
        .snapshot
        .lock()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let Some(snap) = guard.as_ref() else {
        return Ok((StatusCode::SERVICE_UNAVAILABLE, "STALE_DATA").into_response());
    };
    let body = serde_json::json!({
        "snapshot_id": snap.stamps.snapshot_id.as_str(),
        "underlying": snap.underlying,
        "under_price": snap.under_price,
        "rows": &snap.contracts,
    });
    let json = serde_json::to_vec(&body).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CACHE_CONTROL, "max-age=0, private")
        .header(header::CONTENT_TYPE, "application/json")
        .body(axum::body::Body::from(json))
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

async fn policy(State(state): State<AppState>, headers: HeaderMap) -> Result<Response, StatusCode> {
    // v1 serves the active file policy; the Claude path swaps this once bit 7
    // wires LastGood into state.
    let p = Policy::file_default();
    auth(&state, &headers)?;
    let etag = format!("\"{}\"", p.policy_id.as_str());
    if headers
        .get(header::IF_NONE_MATCH)
        .and_then(|v| v.to_str().ok())
        == Some(etag.as_str())
    {
        return Ok(StatusCode::NOT_MODIFIED.into_response());
    }
    let json = serde_json::to_vec(&p).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Response::builder()
        .status(StatusCode::OK)
        .header(header::ETAG, etag)
        .header(header::CACHE_CONTROL, "max-age=0, private")
        .header(header::CONTENT_TYPE, "application/json")
        .body(axum::body::Body::from(json))
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

async fn agents(State(state): State<AppState>, headers: HeaderMap) -> Result<Response, StatusCode> {
    auth(&state, &headers)?;
    let hist = state
        .metrics
        .lock()
        .map(|h| h.json())
        .unwrap_or_else(|_| serde_json::Value::Null);
    let body = serde_json::json!({
        "policy": Policy::file_default(),
        "decide_hist": hist,
        "killed": state.inhibit(),
    });
    let json = serde_json::to_vec(&body).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CACHE_CONTROL, "no-store")
        .header(header::CONTENT_TYPE, "application/json")
        .body(axum::body::Body::from(json))
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

async fn blotter(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Response, StatusCode> {
    auth(&state, &headers)?;
    let n = state
        .blotter
        .lock()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .rows
        .len();
    let json = serde_json::json!({ "rows": n, "killed": state.inhibit() });
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CACHE_CONTROL, "no-store")
        .header(header::CONTENT_TYPE, "application/json")
        .body(axum::body::Body::from(json.to_string()))
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

async fn metrics(State(state): State<AppState>) -> impl IntoResponse {
    let body = state
        .metrics
        .lock()
        .map(|h| h.prometheus())
        .unwrap_or_else(|_| "nr_decide_ms_count 0\n".into());
    ([(header::CONTENT_TYPE, "text/plain; version=0.0.4")], body)
}

async fn broker_status(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Response, StatusCode> {
    auth(&state, &headers)?;
    let body = match &state.broker {
        None => serde_json::json!({
            "alpaca": "missing_creds",
            "paper": state.paper,
            "claude_configured": state.claude_configured,
            "llm_off": true,
        }),
        Some(b) => match b.account() {
            Ok(a) => serde_json::json!({
                "alpaca": "ok",
                "paper": a.paper,
                "status": a.status,
                "equity": a.equity,
                "account": a.account_tail,
                "base": b.base_url(),
                "claude_configured": state.claude_configured,
            }),
            Err(_) => serde_json::json!({
                "alpaca": "http_error",
                "paper": state.paper,
                "base": b.base_url(),
                "claude_configured": state.claude_configured,
            }),
        },
    };
    let json = serde_json::to_vec(&body).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CACHE_CONTROL, "no-store")
        .header(header::CONTENT_TYPE, "application/json")
        .body(axum::body::Body::from(json))
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

async fn kill(State(state): State<AppState>, headers: HeaderMap) -> Result<StatusCode, StatusCode> {
    auth(&state, &headers)?;
    state.killed.store(true, Ordering::SeqCst);
    Ok(StatusCode::NO_CONTENT)
}

// UI assets are embedded so the binary is self-contained; no filesystem
// serving means no path traversal surface.
const INDEX_HTML: &str = include_str!("../../../frontend/index.html");
const APP_JS: &str = include_str!("../../../frontend/app.js");
const THEME_CSS: &str = include_str!("../../../frontend/theme.css");

async fn index() -> Html<&'static str> {
    Html(INDEX_HTML)
}

async fn app_js() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "text/javascript; charset=utf-8")],
        APP_JS,
    )
}

async fn theme_css() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "text/css; charset=utf-8")],
        THEME_CSS,
    )
}

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/", get(index))
        .route("/index.html", get(index))
        .route("/app.js", get(app_js))
        .route("/theme.css", get(theme_css))
        .route("/api/snapshot", get(snapshot))
        .route("/api/chain", get(chain))
        .route("/api/policy", get(policy))
        .route("/api/agents", get(agents))
        .route("/api/top20", get(top20))
        .route("/api/blotter", get(blotter))
        .route("/metrics", get(metrics))
        .route("/api/metrics", get(metrics))
        .route("/api/broker", get(broker_status))
        .route("/api/kill", post(kill))
        .with_state(state)
}

pub async fn serve(bind: &str, state: AppState) -> anyhow::Result<()> {
    let listener = tokio::net::TcpListener::bind(bind).await?;
    axum::serve(listener, router(state)).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    use super::*;

    #[tokio::test]
    async fn snapshot_etag_and_delayed() {
        let app = router(AppState::from_fixture());
        let res = app
            .oneshot(
                Request::builder()
                    .uri("/api/snapshot")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let etag = res.headers().get(header::ETAG).unwrap().to_str().unwrap();
        assert!(etag.contains("snap-fix01"));
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(v["delayed_badge"], "DELAYED");
        assert_eq!(v["underlying"], "SPY");
        assert!(v["n_contracts"].as_u64().unwrap() >= 200);
        assert!(v["snapshot_id"].as_str().unwrap().starts_with("snap-"));
    }

    #[tokio::test]
    async fn blotter_is_no_store() {
        let app = router(AppState::from_fixture());
        let res = app
            .oneshot(
                Request::builder()
                    .uri("/api/blotter")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(
            res.headers().get(header::CACHE_CONTROL).unwrap(),
            "no-store"
        );
    }

    #[tokio::test]
    async fn metrics_alias_and_nr_decide_ms() {
        let app = router(AppState::from_fixture());
        for uri in ["/metrics", "/api/metrics"] {
            let res = app
                .clone()
                .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
                .await
                .unwrap();
            assert_eq!(res.status(), StatusCode::OK, "{uri}");
            let body =
                String::from_utf8(res.into_body().collect().await.unwrap().to_bytes().to_vec())
                    .unwrap();
            assert!(body.contains("nr_decide_ms"), "{uri}");
        }
    }

    #[tokio::test]
    async fn metrics_has_nr_decide_ms() {
        let app = router(AppState::from_fixture());
        let res = app
            .oneshot(
                Request::builder()
                    .uri("/metrics")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        let body = String::from_utf8(bytes.to_vec()).unwrap();
        assert!(body.contains("nr_decide_ms"));
    }

    #[tokio::test]
    async fn broker_status_without_keys() {
        let app = router(AppState::from_fixture());
        let res = app
            .oneshot(
                Request::builder()
                    .uri("/api/broker")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(v["alpaca"], "missing_creds");
        assert_eq!(v["claude_configured"], false);
    }

    #[tokio::test]
    async fn missing_ui_token_is_unauthorized() {
        let mut state = AppState::from_fixture();
        state.token = Some("nr-test-token".into());
        let app = router(state);
        let res = app
            .oneshot(
                Request::builder()
                    .uri("/api/blotter")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn ui_assets_served_with_content_types() {
        let app = router(AppState::from_fixture());
        for (uri, want) in [
            ("/", "text/html"),
            ("/app.js", "text/javascript"),
            ("/theme.css", "text/css"),
        ] {
            let res = app
                .clone()
                .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
                .await
                .unwrap();
            assert_eq!(res.status(), StatusCode::OK, "GET {uri}");
            let ct = res.headers().get(header::CONTENT_TYPE).unwrap();
            assert!(ct.to_str().unwrap().starts_with(want), "{uri} ct={ct:?}");
        }
    }

    #[tokio::test]
    async fn chain_lists_full_contract_set() {
        let app = router(AppState::from_fixture());
        let res = app
            .oneshot(
                Request::builder()
                    .uri("/api/chain")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert!(v["rows"].as_array().unwrap().len() >= 200);
        assert_eq!(v["rows"][0]["right"], "Put");
        assert!(v["under_price"].as_f64().unwrap() > 0.0);
    }

    #[tokio::test]
    async fn policy_serves_bounds_and_lambdas() {
        let app = router(AppState::from_fixture());
        let res = app
            .oneshot(
                Request::builder()
                    .uri("/api/policy")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        assert_eq!(
            res.headers().get(header::ETAG).unwrap(),
            "\"file-default-policy\""
        );
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(v["dte_min"], 30);
        assert_eq!(v["dte_max"], 60);
        assert_eq!(v["lambda_eff"], 1.0);
    }

    #[tokio::test]
    async fn agents_exposes_decide_histogram() {
        let app = router(AppState::from_fixture());
        let res = app
            .oneshot(
                Request::builder()
                    .uri("/api/agents")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(v["policy"]["regime"], "unknown");
        assert!(v["decide_hist"]["counts"].as_array().unwrap().len() == 7);
    }

    #[tokio::test]
    async fn kill_204_even_if_we_clear_snapshot() {
        let state = AppState::from_fixture();
        *state.snapshot.lock().unwrap() = None;
        let app = router(state.clone());
        let res = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/kill")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::NO_CONTENT);
        assert!(state.inhibit());
    }
}
