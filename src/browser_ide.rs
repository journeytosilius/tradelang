//! Hosted browser IDE support for PalmScript.
//!
//! This module provides the typed public dataset catalog, request/response
//! types, router builder, and curated backtest execution used by the browser
//! IDE service.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use axum::extract::ws::{Message as WsMessage, WebSocket, WebSocketUpgrade};
use axum::extract::{Query, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::response::{Html, IntoResponse, Redirect, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use futures_util::StreamExt;
use lsp_server::Message as LspMessage;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use tokio::time::timeout;
use tower_http::cors::CorsLayer;

use crate::backtest::{BacktestConfig, BacktestResult};
use crate::compiler::{compile, CompiledProgram};
use crate::exchange::{fetch_source_runtime_config, ExchangeEndpoints, ExchangeFetchError};
use crate::ide_lsp::IdeLspSession;
use crate::interval::{Interval, SourceTemplate};
use crate::runtime::{slice_runtime_window, SourceRuntimeConfig, VmLimits};
use crate::{run_backtest_with_sources, Diagnostic as PalmDiagnostic};

const DEFAULT_SCRIPT_LIMIT_BYTES: usize = 128 * 1024;
const DEFAULT_SESSION_IDLE_SECS: u64 = 30 * 60;
const DEFAULT_BACKTEST_TIMEOUT_SECS: u64 = 30;
const DEFAULT_MAX_PARALLEL_BACKTESTS: usize = 4;
const DEFAULT_MAX_LSP_SESSIONS: usize = 32;
const SESSION_HEADER: &str = "x-palmscript-session";
const DAY_MS: i64 = 24 * 60 * 60 * 1_000;

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Ord, PartialOrd)]
#[serde(rename_all = "snake_case")]
pub enum PublicDatasetId {
    BtcusdtBinanceSpot4h4y,
}

impl PublicDatasetId {
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::BtcusdtBinanceSpot4h4y => "btcusdt_binance_spot_4h_4y",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PublicDataset {
    pub dataset_id: PublicDatasetId,
    pub display_name: String,
    pub source_template: SourceTemplate,
    pub symbol: String,
    pub base_interval: Interval,
    pub supported_intervals: Vec<Interval>,
    pub from: i64,
    pub to: i64,
    pub initial_capital: f64,
    pub fee_bps: f64,
    pub slippage_bps: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PublicDatasetCatalog {
    pub datasets: Vec<PublicDataset>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PublicExample {
    pub id: String,
    pub name: String,
    pub description: String,
    pub source: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct BrowserSessionId(pub String);

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CheckRequest {
    pub script: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CheckResponse {
    pub diagnostics: Vec<PalmDiagnostic>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BacktestRequest {
    pub script: String,
    pub dataset_id: PublicDatasetId,
    pub from_ms: i64,
    pub to_ms: i64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BacktestResponse {
    pub dataset: PublicDataset,
    pub result: BacktestResult,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CachedPublicDataset {
    pub dataset: PublicDataset,
    pub runtime: SourceRuntimeConfig,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PublicIdeServerConfig {
    pub max_script_bytes: usize,
    pub session_idle_secs: u64,
    pub backtest_timeout_secs: u64,
    pub max_parallel_backtests: usize,
    pub max_lsp_sessions: usize,
}

impl Default for PublicIdeServerConfig {
    fn default() -> Self {
        Self {
            max_script_bytes: DEFAULT_SCRIPT_LIMIT_BYTES,
            session_idle_secs: DEFAULT_SESSION_IDLE_SECS,
            backtest_timeout_secs: DEFAULT_BACKTEST_TIMEOUT_SECS,
            max_parallel_backtests: DEFAULT_MAX_PARALLEL_BACKTESTS,
            max_lsp_sessions: DEFAULT_MAX_LSP_SESSIONS,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum BrowserIdeError {
    #[error("public dataset `{0}` is unavailable")]
    DatasetUnavailable(String),
    #[error("failed to fetch curated dataset runtime: {0}")]
    Exchange(#[from] ExchangeFetchError),
    #[error("failed to compile curated dataset support script: {0}")]
    Compile(String),
}

#[derive(Clone)]
pub struct PublicIdeState {
    config: PublicIdeServerConfig,
    examples: Arc<Vec<PublicExample>>,
    datasets: Arc<BTreeMap<PublicDatasetId, CachedPublicDataset>>,
    run_permits: Arc<Semaphore>,
    lsp_permits: Arc<Semaphore>,
    sessions: Arc<Mutex<SessionRegistry>>,
}

#[derive(Default)]
struct SessionRegistry {
    last_seen: HashMap<String, Instant>,
    active_backtests: HashSet<String>,
}

#[derive(Debug, Serialize)]
struct ApiErrorBody {
    error: String,
}

struct ApiError {
    status: StatusCode,
    message: String,
}

impl ApiError {
    fn new(status: StatusCode, message: impl Into<String>) -> Self {
        Self {
            status,
            message: message.into(),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (
            self.status,
            Json(ApiErrorBody {
                error: self.message,
            }),
        )
            .into_response()
    }
}

impl PublicIdeState {
    pub fn new(
        config: PublicIdeServerConfig,
        examples: Vec<PublicExample>,
        datasets: Vec<CachedPublicDataset>,
    ) -> Self {
        Self {
            config,
            examples: Arc::new(examples),
            datasets: Arc::new(
                datasets
                    .into_iter()
                    .map(|dataset| (dataset.dataset.dataset_id.clone(), dataset))
                    .collect(),
            ),
            run_permits: Arc::new(Semaphore::new(config.max_parallel_backtests.max(1))),
            lsp_permits: Arc::new(Semaphore::new(config.max_lsp_sessions.max(1))),
            sessions: Arc::new(Mutex::new(SessionRegistry::default())),
        }
    }

    fn dataset(&self, dataset_id: &PublicDatasetId) -> Option<&CachedPublicDataset> {
        self.datasets.get(dataset_id)
    }

    fn mark_session_active(&self, session: &BrowserSessionId) {
        let mut registry = self.sessions.lock().expect("session registry poisoned");
        prune_sessions(&mut registry, self.config.session_idle_secs);
        registry.last_seen.insert(session.0.clone(), Instant::now());
    }

    fn begin_backtest(&self, session: &BrowserSessionId) -> Result<(), ApiError> {
        let mut registry = self.sessions.lock().expect("session registry poisoned");
        prune_sessions(&mut registry, self.config.session_idle_secs);
        registry.last_seen.insert(session.0.clone(), Instant::now());
        if !registry.active_backtests.insert(session.0.clone()) {
            return Err(ApiError::new(
                StatusCode::TOO_MANY_REQUESTS,
                "only one active backtest is allowed per browser session",
            ));
        }
        Ok(())
    }

    fn end_backtest(&self, session: &BrowserSessionId) {
        let mut registry = self.sessions.lock().expect("session registry poisoned");
        registry.last_seen.insert(session.0.clone(), Instant::now());
        registry.active_backtests.remove(&session.0);
    }
}

pub fn public_dataset_catalog() -> PublicDatasetCatalog {
    PublicDatasetCatalog {
        datasets: vec![PublicDataset {
            dataset_id: PublicDatasetId::BtcusdtBinanceSpot4h4y,
            display_name: "BTCUSDT Binance Spot 4h".to_string(),
            source_template: SourceTemplate::BinanceSpot,
            symbol: "BTCUSDT".to_string(),
            base_interval: Interval::Hour4,
            supported_intervals: vec![Interval::Hour4, Interval::Day1, Interval::Week1],
            from: 1_646_611_200_000,
            to: 1_772_841_600_000,
            initial_capital: 10_000.0,
            fee_bps: 7.5,
            slippage_bps: 2.0,
        }],
    }
}

pub fn public_examples() -> Vec<PublicExample> {
    vec![
        PublicExample {
            id: "starter".to_string(),
            name: "Starter".to_string(),
            description: "Minimal single-source PalmScript strategy.".to_string(),
            source: r#"interval 4h
source spot = binance.spot("BTCUSDT")
use spot 1d
use spot 1w

let daily_fast = ema(spot.1d.close, 30)
let daily_slow = ema(spot.1d.close, 80)
let weekly_fast = ema(spot.1w.close, 5)
let weekly_slow = ema(spot.1w.close, 13)
let fast = ema(spot.close, 13)
let slow = ema(spot.close, 89)

let trend_long = above(daily_fast, daily_slow) and above(weekly_fast, weekly_slow)
let breakout_long = above(spot.close, highest(spot.high, 20)[1])

entry long = trend_long and breakout_long and above(fast, slow)
exit long = below(fast, slow)

plot(fast - slow)
export trend_long_state = trend_long
"#
            .to_string(),
        },
        PublicExample {
            id: "adaptive".to_string(),
            name: "Adaptive Trend".to_string(),
            description: "The checked-in BTC spot adaptive trend example.".to_string(),
            source: include_str!("../examples/strategies/adaptive_trend_backtest.palm").to_string(),
        },
    ]
}

pub async fn build_public_dataset_cache(
    endpoints: ExchangeEndpoints,
) -> Result<Vec<CachedPublicDataset>, BrowserIdeError> {
    let datasets = public_dataset_catalog().datasets;
    let mut cached = Vec::with_capacity(datasets.len());
    for dataset in datasets {
        let support_script = dataset_support_script(&dataset);
        let runtime = tokio::task::spawn_blocking({
            let endpoints = endpoints.clone();
            let support_script = support_script.clone();
            let dataset = dataset.clone();
            move || {
                let compiled = compile(&support_script)
                    .map_err(|err| BrowserIdeError::Compile(err.to_string()))?;
                fetch_source_runtime_config(&compiled, dataset.from, dataset.to, &endpoints)
                    .map_err(BrowserIdeError::from)
            }
        })
        .await
        .map_err(|err| BrowserIdeError::Compile(err.to_string()))??;
        cached.push(CachedPublicDataset { dataset, runtime });
    }
    Ok(cached)
}

pub fn browser_ide_router(state: PublicIdeState) -> Router {
    Router::new()
        .route("/", get(index_html))
        .route("/app", get(app_root_redirect))
        .route("/app/", get(index_html))
        .route("/ide/palmscript_ide.js", get(ide_wasm_js))
        .route("/app/ide/palmscript_ide.js", get(ide_wasm_js))
        .route("/ide/palmscript_ide_bg.wasm", get(ide_wasm_binary))
        .route("/app/ide/palmscript_ide_bg.wasm", get(ide_wasm_binary))
        .route("/api/healthz", get(healthz))
        .route("/app/api/healthz", get(healthz))
        .route("/api/examples", get(list_examples))
        .route("/app/api/examples", get(list_examples))
        .route("/api/datasets", get(list_datasets))
        .route("/app/api/datasets", get(list_datasets))
        .route("/api/check", post(check_script))
        .route("/app/api/check", post(check_script))
        .route("/api/backtest", post(run_backtest))
        .route("/app/api/backtest", post(run_backtest))
        .route("/api/lsp", get(lsp_socket))
        .route("/app/api/lsp", get(lsp_socket))
        .layer(CorsLayer::permissive())
        .with_state(state)
}

async fn healthz() -> impl IntoResponse {
    Json(json!({ "ok": true }))
}

async fn index_html() -> impl IntoResponse {
    Html(include_str!("../ide-wasm/index.html"))
}

async fn app_root_redirect() -> impl IntoResponse {
    Redirect::permanent("/app/")
}

async fn ide_wasm_js() -> impl IntoResponse {
    (
        [(
            axum::http::header::CONTENT_TYPE,
            HeaderValue::from_static("application/javascript"),
        )],
        include_str!("../ide-wasm/dist/palmscript_ide.js"),
    )
}

async fn ide_wasm_binary() -> impl IntoResponse {
    (
        [(
            axum::http::header::CONTENT_TYPE,
            HeaderValue::from_static("application/wasm"),
        )],
        include_bytes!("../ide-wasm/dist/palmscript_ide_bg.wasm").as_slice(),
    )
}

async fn list_examples(State(state): State<PublicIdeState>) -> impl IntoResponse {
    Json(state.examples.as_ref().clone())
}

async fn list_datasets(State(state): State<PublicIdeState>) -> impl IntoResponse {
    let datasets: Vec<PublicDataset> = state
        .datasets
        .values()
        .map(|cached| cached.dataset.clone())
        .collect();
    Json(PublicDatasetCatalog { datasets })
}

async fn check_script(
    State(state): State<PublicIdeState>,
    headers: HeaderMap,
    Json(request): Json<CheckRequest>,
) -> Result<impl IntoResponse, ApiError> {
    validate_script_size(&state, &request.script)?;
    if let Some(session) = session_id_from_headers(&headers) {
        state.mark_session_active(&session);
    }
    let diagnostics = match compile(&request.script) {
        Ok(_) => Vec::new(),
        Err(err) => err.diagnostics,
    };
    Ok(Json(CheckResponse { diagnostics }))
}

async fn run_backtest(
    State(state): State<PublicIdeState>,
    headers: HeaderMap,
    Json(request): Json<BacktestRequest>,
) -> Result<impl IntoResponse, ApiError> {
    validate_script_size(&state, &request.script)?;
    let session = session_id_from_headers(&headers)
        .unwrap_or_else(|| BrowserSessionId(format!("anon-{}", rand::random::<u64>())));
    state.begin_backtest(&session)?;
    let permit = state
        .run_permits
        .clone()
        .acquire_owned()
        .await
        .map_err(|_| {
            ApiError::new(
                StatusCode::SERVICE_UNAVAILABLE,
                "run worker pool is unavailable",
            )
        })?;
    let cached = state
        .dataset(&request.dataset_id)
        .cloned()
        .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "unknown public dataset"))?;
    validate_requested_window(&cached.dataset, request.from_ms, request.to_ms)?;
    let script = request.script.clone();
    let timeout_secs = state.config.backtest_timeout_secs;
    let state_clone = state.clone();
    let session_clone = session.clone();
    let response = timeout(
        Duration::from_secs(timeout_secs),
        tokio::task::spawn_blocking(move || {
            let _permit = permit;
            let compiled = compile_public_script(&script, &cached)?;
            let runtime = slice_runtime_window(&cached.runtime, request.from_ms, request.to_ms);
            let mut dataset = cached.dataset.clone();
            dataset.from = request.from_ms;
            dataset.to = request.to_ms;
            let result = run_backtest_with_sources(
                &compiled,
                runtime,
                VmLimits::default(),
                BacktestConfig {
                    execution_source_alias: compiled.program.declared_sources[0].alias.clone(),
                    initial_capital: dataset.initial_capital,
                    fee_bps: dataset.fee_bps,
                    slippage_bps: dataset.slippage_bps,
                    perp: None,
                    perp_context: None,
                },
            )
            .map_err(|err| ApiError::new(StatusCode::BAD_REQUEST, err.to_string()))?;
            Ok::<_, ApiError>(BacktestResponse { dataset, result })
        }),
    )
    .await;
    state_clone.end_backtest(&session_clone);
    match response {
        Ok(joined) => match joined {
            Ok(result) => Ok(Json(result?)),
            Err(err) => Err(ApiError::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("backtest worker failed: {err}"),
            )),
        },
        Err(_) => Err(ApiError::new(
            StatusCode::REQUEST_TIMEOUT,
            "backtest timed out",
        )),
    }
}

async fn lsp_socket(
    ws: WebSocketUpgrade,
    State(state): State<PublicIdeState>,
    headers: HeaderMap,
    query: Query<HashMap<String, String>>,
) -> Result<impl IntoResponse, ApiError> {
    let session = session_id_from_headers(&headers)
        .or_else(|| query.get("session").cloned().map(BrowserSessionId));
    if let Some(session) = &session {
        state.mark_session_active(session);
    }
    let permit = state.lsp_permits.clone().try_acquire_owned().map_err(|_| {
        ApiError::new(
            StatusCode::TOO_MANY_REQUESTS,
            "too many active IDE sessions",
        )
    })?;
    Ok(ws.on_upgrade(move |socket| async move {
        lsp_socket_loop(socket, state, session, permit).await;
    }))
}

async fn lsp_socket_loop(
    mut socket: WebSocket,
    state: PublicIdeState,
    session: Option<BrowserSessionId>,
    _permit: OwnedSemaphorePermit,
) {
    let mut lsp = IdeLspSession::new();
    while let Some(message) = socket.next().await {
        let Ok(message) = message else {
            break;
        };
        match message {
            WsMessage::Text(text) => {
                if let Some(session) = &session {
                    state.mark_session_active(session);
                }
                let Ok(message) = serde_json::from_str::<LspMessage>(&text) else {
                    let _ = socket
                        .send(WsMessage::Text(
                            serde_json::to_string(&json!({
                                "jsonrpc": "2.0",
                                "error": { "code": -32700, "message": "invalid LSP message" },
                                "id": null
                            }))
                            .unwrap_or_else(|_| "{\"jsonrpc\":\"2.0\",\"error\":{\"code\":-32700,\"message\":\"invalid LSP message\"},\"id\":null}".to_string())
                            .into(),
                        ))
                        .await;
                    continue;
                };
                for outbound in lsp.handle_message(message) {
                    let Ok(payload) = serde_json::to_string(&outbound) else {
                        continue;
                    };
                    if socket.send(WsMessage::Text(payload.into())).await.is_err() {
                        return;
                    }
                }
                if lsp.should_exit() {
                    return;
                }
            }
            WsMessage::Close(_) => break,
            WsMessage::Ping(payload) => {
                if socket.send(WsMessage::Pong(payload)).await.is_err() {
                    break;
                }
            }
            WsMessage::Binary(_) | WsMessage::Pong(_) => {}
        }
    }
}

fn validate_script_size(state: &PublicIdeState, script: &str) -> Result<(), ApiError> {
    if script.len() > state.config.max_script_bytes {
        return Err(ApiError::new(
            StatusCode::PAYLOAD_TOO_LARGE,
            format!(
                "script exceeds the public IDE limit of {} bytes",
                state.config.max_script_bytes
            ),
        ));
    }
    Ok(())
}

fn session_id_from_headers(headers: &HeaderMap) -> Option<BrowserSessionId> {
    headers
        .get(SESSION_HEADER)
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.trim().is_empty())
        .map(|value| BrowserSessionId(value.to_string()))
}

fn prune_sessions(registry: &mut SessionRegistry, idle_secs: u64) {
    let cutoff = Instant::now() - Duration::from_secs(idle_secs);
    registry
        .last_seen
        .retain(|_, last_seen| *last_seen >= cutoff);
    registry
        .active_backtests
        .retain(|session| registry.last_seen.contains_key(session));
}

fn compile_public_script(
    script: &str,
    cached: &CachedPublicDataset,
) -> Result<CompiledProgram, ApiError> {
    let compiled =
        compile(script).map_err(|err| ApiError::new(StatusCode::BAD_REQUEST, err.to_string()))?;
    validate_dataset_compatibility(&compiled, &cached.dataset)?;
    Ok(compiled)
}

fn validate_requested_window(
    dataset: &PublicDataset,
    from_ms: i64,
    to_ms: i64,
) -> Result<(), ApiError> {
    if from_ms >= to_ms {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            format!("requested IDE range must satisfy from < to, got {from_ms} >= {to_ms}"),
        ));
    }
    if to_ms - from_ms < DAY_MS {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "requested IDE range must cover at least one calendar day",
        ));
    }
    if from_ms < dataset.from || to_ms > dataset.to {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            format!(
                "requested IDE range must stay within curated dataset bounds [{}, {})",
                dataset.from, dataset.to
            ),
        ));
    }
    Ok(())
}

fn validate_dataset_compatibility(
    compiled: &CompiledProgram,
    dataset: &PublicDataset,
) -> Result<(), ApiError> {
    if compiled.program.declared_sources.len() != 1 {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "public IDE backtests support exactly one declared source",
        ));
    }
    let source = &compiled.program.declared_sources[0];
    if compiled.program.base_interval != Some(dataset.base_interval) {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            format!(
                "dataset `{}` requires base interval `{}`",
                dataset.dataset_id.as_str(),
                dataset.base_interval.as_str()
            ),
        ));
    }
    if source.template != dataset.source_template || source.symbol != dataset.symbol {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            format!(
                "dataset `{}` only supports {}(\"{}\")",
                dataset.dataset_id.as_str(),
                dataset.source_template.as_str(),
                dataset.symbol
            ),
        ));
    }
    let allowed: BTreeSet<Interval> = dataset.supported_intervals.iter().copied().collect();
    for interval_ref in &compiled.program.source_intervals {
        if interval_ref.source_id != source.id {
            return Err(ApiError::new(
                StatusCode::BAD_REQUEST,
                "public IDE backtests do not support multiple sources",
            ));
        }
        if !allowed.contains(&interval_ref.interval) {
            return Err(ApiError::new(
                StatusCode::BAD_REQUEST,
                format!(
                    "dataset `{}` does not provide `{}` bars",
                    dataset.dataset_id.as_str(),
                    interval_ref.interval.as_str()
                ),
            ));
        }
    }
    Ok(())
}

fn dataset_support_script(dataset: &PublicDataset) -> String {
    let alias = "spot";
    let mut source = format!(
        "interval {}\nsource {alias} = {}(\"{}\")\n",
        dataset.base_interval.as_str(),
        dataset.source_template.as_str(),
        dataset.symbol
    );
    for interval in &dataset.supported_intervals {
        if *interval == dataset.base_interval {
            continue;
        }
        source.push_str(&format!("use {alias} {}\n", interval.as_str()));
    }
    source.push_str("export public_dataset_close = spot.close\n");
    source
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::{to_bytes, Body};
    use axum::http::Request;
    use tower::ServiceExt;

    fn fixture_state() -> PublicIdeState {
        let dataset = PublicDataset {
            dataset_id: PublicDatasetId::BtcusdtBinanceSpot4h4y,
            display_name: "Fixture".to_string(),
            source_template: SourceTemplate::BinanceSpot,
            symbol: "BTCUSDT".to_string(),
            base_interval: Interval::Hour4,
            supported_intervals: vec![Interval::Hour4, Interval::Day1, Interval::Week1],
            from: 0,
            to: 1_000,
            initial_capital: 10_000.0,
            fee_bps: 7.5,
            slippage_bps: 2.0,
        };
        let runtime = SourceRuntimeConfig {
            base_interval: Interval::Hour4,
            feeds: vec![crate::runtime::SourceFeed {
                source_id: 0,
                interval: Interval::Hour4,
                bars: vec![crate::runtime::Bar {
                    open: 10.0,
                    high: 11.0,
                    low: 9.0,
                    close: 10.5,
                    volume: 1.0,
                    time: 0.0,
                }],
            }],
        };
        PublicIdeState::new(
            PublicIdeServerConfig::default(),
            public_examples(),
            vec![CachedPublicDataset { dataset, runtime }],
        )
    }

    #[test]
    fn validates_single_matching_source() {
        let compiled = compile(
            r#"interval 4h
source spot = binance.spot("BTCUSDT")
use spot 1d
export x = spot.close
"#,
        )
        .expect("script should compile");
        let dataset = fixture_state()
            .dataset(&PublicDatasetId::BtcusdtBinanceSpot4h4y)
            .expect("fixture dataset")
            .dataset
            .clone();
        assert!(validate_dataset_compatibility(&compiled, &dataset).is_ok());
    }

    #[test]
    fn rejects_multiple_sources() {
        let compiled = compile(
            r#"interval 4h
source spot = binance.spot("BTCUSDT")
source alt = binance.spot("ETHUSDT")
export x = spot.close
"#,
        )
        .expect("script should compile");
        let dataset = fixture_state()
            .dataset(&PublicDatasetId::BtcusdtBinanceSpot4h4y)
            .expect("fixture dataset")
            .dataset
            .clone();
        assert!(validate_dataset_compatibility(&compiled, &dataset).is_err());
    }

    #[tokio::test]
    async fn examples_endpoint_returns_catalog() {
        let app = browser_ide_router(fixture_state());
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/examples")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");
        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body bytes");
        let examples: Vec<PublicExample> =
            serde_json::from_slice(&body).expect("examples response should deserialize");
        assert!(!examples.is_empty());
    }

    #[tokio::test]
    async fn app_prefixed_dataset_endpoint_returns_catalog() {
        let app = browser_ide_router(fixture_state());
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/app/api/datasets")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn wasm_asset_route_is_served() {
        let app = browser_ide_router(fixture_state());
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/app/ide/palmscript_ide.js")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response
                .headers()
                .get(axum::http::header::CONTENT_TYPE)
                .and_then(|value| value.to_str().ok()),
            Some("application/javascript")
        );
    }

    #[tokio::test]
    async fn check_endpoint_returns_compile_diagnostics() {
        let app = browser_ide_router(fixture_state());
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/check")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::to_vec(&CheckRequest {
                            script: "interval".to_string(),
                        })
                        .expect("request body"),
                    ))
                    .expect("request"),
            )
            .await
            .expect("response");
        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body bytes");
        let payload: CheckResponse =
            serde_json::from_slice(&body).expect("check response should deserialize");
        assert!(!payload.diagnostics.is_empty());
    }

    #[tokio::test]
    async fn backtest_endpoint_rejects_incompatible_source() {
        let app = browser_ide_router(fixture_state());
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/backtest")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::to_vec(&BacktestRequest {
                            script: r#"interval 4h
source spot = binance.spot("ETHUSDT")
export x = spot.close
"#
                            .to_string(),
                            dataset_id: PublicDatasetId::BtcusdtBinanceSpot4h4y,
                            from_ms: 0,
                            to_ms: 1_000,
                        })
                        .expect("request body"),
                    ))
                    .expect("request"),
            )
            .await
            .expect("response");
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[test]
    fn rejects_requested_window_outside_curated_bounds() {
        let dataset = fixture_state()
            .dataset(&PublicDatasetId::BtcusdtBinanceSpot4h4y)
            .expect("fixture dataset")
            .dataset
            .clone();
        let err = validate_requested_window(&dataset, -1, 1_000).expect_err("window rejected");
        assert_eq!(err.status, StatusCode::BAD_REQUEST);
    }
}
