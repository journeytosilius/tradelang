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
// - RequestFailed
// - MalformedResponse
// - NoData

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
    let source_compiled =
        compiled("interval 1m\nsource a = binance.spot(\"BTCUSDT\")\nplot(a.close)");
    let mut missing_sources_compiled = source_compiled.clone();
    missing_sources_compiled.program.declared_sources.clear();
    let unsupported_interval_compiled =
        compiled("interval 1s\nsource a = bybit.usdt_perps(\"BTCUSDT\")\nplot(a.close)");

    let cases: [(&str, Result<(), ExchangeFetchError>); 7] = [
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
            "request_failed_http_status",
            fetch_source_runtime_config(
                &source_compiled,
                1_704_067_200_000,
                1_704_067_260_000,
                &ExchangeEndpoints {
                    binance_spot_base_url: server.url(),
                    binance_usdm_base_url: server.url(),
                    bybit_base_url: server.url(),
                    gate_base_url: server.url(),
                    ..ExchangeEndpoints::default()
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
                    bybit_base_url: server.url(),
                    gate_base_url: server.url(),
                    ..ExchangeEndpoints::default()
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
                    bybit_base_url: server.url(),
                    gate_base_url: server.url(),
                    ..ExchangeEndpoints::default()
                },
            )
            .map(|_| ()),
        ),
    ];

    let expected_exact = [
        "exchange-backed runs require a base interval declaration",
        "exchange-backed runs require at least one `source` declaration",
        "invalid market time window: from 1704067260000 must be less than to 1704067260000",
        "source `a` with template `bybit.usdt_perps` does not support interval `1s`",
        "",
        "malformed response for `a` (binance.spot) `BTCUSDT` 1m: invalid `open` value",
        "no data returned for `a` (binance.spot) `BTCUSDT` 1m",
    ];

    for ((name, result), expected_message) in cases.into_iter().zip(expected_exact) {
        let err = result.expect_err(name);
        let actual = err.to_string();
        if name == "request_failed_http_status" {
            assert!(
                actual.starts_with(
                    "failed to fetch `a` (binance.spot) `BTCUSDT` 1m: HTTP 500 Internal Server Error from http://127.0.0.1:"
                ),
                "{name}: {actual}"
            );
            assert!(
                actual.contains(
                    "/api/v3/klines?symbol=BTCUSDT&interval=1m&startTime=1704067200000&endTime=1704067259999&limit=1000"
                ),
                "{name}: {actual}"
            );
            continue;
        }
        assert_eq!(actual, expected_message, "{name}");
    }
}
