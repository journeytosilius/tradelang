use mockito::{Matcher, Server};
use palmscript::{
    bytecode::Program, compile, fetch_source_runtime_config, CompiledProgram, ExchangeEndpoints,
    ExchangeFetchError,
};
use serde_json::json;

fn compiled(source: &str) -> CompiledProgram {
    compile(source).expect("script compiles")
}

// Reachable public ExchangeFetchError catalog:
// - MissingBaseInterval
// - MissingSources
// - InvalidTimeWindow
// - UnsupportedInterval
// - RecentHistoryLimitExceeded
// - RequestFailed
// - MalformedResponse
// - NoData
// - UnknownHyperliquidSpotSymbol

#[test]
fn market_fetch_error_catalog_matches_contract() {
    let empty_compiled = CompiledProgram {
        program: Program::default(),
        source: String::new(),
    };
    let mut server = Server::new();
    let _http_500 = server
        .mock("GET", "/api/v3/klines")
        .match_query(Matcher::AllOf(vec![
            Matcher::UrlEncoded("symbol".into(), "BTCUSDT".into()),
            Matcher::UrlEncoded("interval".into(), "1m".into()),
        ]))
        .with_status(500)
        .create();
    let _bad_open = server
        .mock("GET", "/bad-open/api/v3/klines")
        .match_query(Matcher::AllOf(vec![
            Matcher::UrlEncoded("symbol".into(), "BTCUSDT".into()),
            Matcher::UrlEncoded("interval".into(), "1m".into()),
        ]))
        .with_status(200)
        .with_body(json!([[1704067200000_i64, "bad", "2.0", "0.5", "1.5", "10.0"]]).to_string())
        .create();
    let _no_data = server
        .mock("GET", "/no-data/api/v3/klines")
        .match_query(Matcher::AllOf(vec![
            Matcher::UrlEncoded("symbol".into(), "BTCUSDT".into()),
            Matcher::UrlEncoded("interval".into(), "1m".into()),
        ]))
        .with_status(200)
        .with_body("[]")
        .create();
    let _spot_meta = server
        .mock("POST", "/missing-spot/info")
        .match_body(Matcher::Json(json!({ "type": "spotMeta" })))
        .with_status(200)
        .with_body(
            json!({
                "universe": [{"name": "@107", "tokens": [107, 0]}],
                "tokens": [{"name": "HYPE", "index": 107}]
            })
            .to_string(),
        )
        .create();

    let source_compiled =
        compiled("interval 1m\nsource a = binance.spot(\"BTCUSDT\")\nplot(a.close)");
    let missing_sources_compiled = compiled("interval 1m\nplot(close)");
    let unsupported_interval_compiled =
        compiled("interval 1s\nsource a = hyperliquid.perps(\"BTC\")\nplot(a.close)");
    let recent_history_limit_compiled =
        compiled("interval 1m\nsource a = hyperliquid.perps(\"BTC\")\nplot(a.close)");

    let cases: [(&str, Result<(), ExchangeFetchError>); 9] = [
        (
            "missing_base_interval",
            fetch_source_runtime_config(&empty_compiled, 1, 2, &ExchangeEndpoints::default())
                .map(|_| ()),
        ),
        (
            "missing_sources",
            fetch_source_runtime_config(
                &missing_sources_compiled,
                1_704_067_200_000,
                1_704_067_260_000,
                &ExchangeEndpoints::default(),
            )
            .map(|_| ()),
        ),
        (
            "invalid_time_window",
            fetch_source_runtime_config(
                &source_compiled,
                1_704_067_260_000,
                1_704_067_260_000,
                &ExchangeEndpoints::default(),
            )
            .map(|_| ()),
        ),
        (
            "unsupported_interval",
            fetch_source_runtime_config(
                &unsupported_interval_compiled,
                1_704_067_200_000,
                1_704_067_260_000,
                &ExchangeEndpoints::default(),
            )
            .map(|_| ()),
        ),
        (
            "recent_history_limit_exceeded",
            fetch_source_runtime_config(
                &recent_history_limit_compiled,
                1_704_067_200_000,
                1_704_067_200_000 + 5_001 * 60_000,
                &ExchangeEndpoints::default(),
            )
            .map(|_| ()),
        ),
        (
            "request_failed_http_status",
            fetch_source_runtime_config(
                &source_compiled,
                1_704_067_200_000,
                1_704_067_260_000,
                &ExchangeEndpoints {
                    binance_spot_base_url: server.url(),
                    binance_usdm_base_url: server.url(),
                    hyperliquid_info_url: format!("{}/info", server.url()),
                },
            )
            .map(|_| ()),
        ),
        (
            "malformed_response",
            fetch_source_runtime_config(
                &source_compiled,
                1_704_067_200_000,
                1_704_067_260_000,
                &ExchangeEndpoints {
                    binance_spot_base_url: format!("{}/bad-open", server.url()),
                    binance_usdm_base_url: server.url(),
                    hyperliquid_info_url: format!("{}/info", server.url()),
                },
            )
            .map(|_| ()),
        ),
        (
            "no_data",
            fetch_source_runtime_config(
                &source_compiled,
                1_704_067_200_000,
                1_704_067_260_000,
                &ExchangeEndpoints {
                    binance_spot_base_url: format!("{}/no-data", server.url()),
                    binance_usdm_base_url: server.url(),
                    hyperliquid_info_url: format!("{}/info", server.url()),
                },
            )
            .map(|_| ()),
        ),
        (
            "unknown_hyperliquid_spot_symbol",
            fetch_source_runtime_config(
                &compiled("interval 1m\nsource a = hyperliquid.spot(\"MISSING\")\nplot(a.close)"),
                1_704_067_200_000,
                1_704_067_260_000,
                &ExchangeEndpoints {
                    binance_spot_base_url: server.url(),
                    binance_usdm_base_url: server.url(),
                    hyperliquid_info_url: format!("{}/missing-spot/info", server.url()),
                },
            )
            .map(|_| ()),
        ),
    ];

    let expected = [
        "exchange-backed runs require a base interval declaration",
        "exchange-backed runs require at least one `source` declaration",
        "invalid market time window: from 1704067260000 must be less than to 1704067260000",
        "source `a` with template `hyperliquid.perps` does not support interval `1s`",
        "source `a` (hyperliquid.perps) `BTC` 1m requires 5001 candle(s) for the requested window, but the venue only provides the most recent 5000 candle(s) over REST",
        "failed to fetch `a` (binance.spot) `BTCUSDT` 1m: HTTP 500 Internal Server Error",
        "malformed response for `a` (binance.spot) `BTCUSDT` 1m: invalid `open` value",
        "no data returned for `a` (binance.spot) `BTCUSDT` 1m",
        "unknown Hyperliquid spot symbol `MISSING`",
    ];

    for ((name, result), expected_message) in cases.into_iter().zip(expected) {
        let err = result.expect_err(name);
        assert_eq!(err.to_string(), expected_message, "{name}");
    }
}
