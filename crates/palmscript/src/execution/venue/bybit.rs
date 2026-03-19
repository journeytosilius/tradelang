use reqwest::blocking::Client;
use serde::Deserialize;

use crate::exchange::ExchangeEndpoints;
use crate::interval::{DeclaredMarketSource, SourceTemplate};

use super::{decode_json, QuoteFeedData};
use crate::execution::{
    ExecutionError, FeedSnapshotState, PaperExecutionSource, PriceSnapshot, TopOfBookSnapshot,
};

#[derive(Deserialize)]
struct BybitEnvelope {
    #[serde(rename = "retCode")]
    ret_code: i32,
    #[serde(rename = "retMsg")]
    ret_msg: String,
    result: BybitTickerResult,
}

#[derive(Deserialize)]
struct BybitTickerResult {
    list: Vec<BybitTicker>,
}

#[derive(Deserialize)]
struct BybitTicker {
    #[serde(rename = "bid1Price")]
    bid_1_price: String,
    #[serde(rename = "ask1Price")]
    ask_1_price: String,
    #[serde(rename = "lastPrice")]
    last_price: String,
    #[serde(rename = "markPrice")]
    mark_price: Option<String>,
}

pub(crate) fn validate(source: &DeclaredMarketSource) -> Result<(), ExecutionError> {
    if source.symbol.is_empty() {
        return Err(ExecutionError::InvalidConfig {
            message: "bybit paper execution requires a non-empty symbol".to_string(),
        });
    }
    Ok(())
}

pub(crate) fn fetch_quote_feed(
    client: &Client,
    endpoints: &ExchangeEndpoints,
    source: &PaperExecutionSource,
    now_ms: i64,
) -> Result<QuoteFeedData, ExecutionError> {
    let base = endpoints.bybit_base_url.trim_end_matches('/');
    let category = match source.template {
        SourceTemplate::BybitSpot => "spot",
        SourceTemplate::BybitUsdtPerps => "linear",
        _ => unreachable!("bybit fetch_quote_feed called for non-bybit template"),
    };
    let url = format!("{base}/v5/market/tickers");
    let payload: BybitEnvelope = decode_json(
        client
            .get(&url)
            .query(&[("category", category), ("symbol", source.symbol.as_str())])
            .send()
            .map_err(|err| ExecutionError::Fetch(err.to_string()))?,
        &url,
    )?;
    if payload.ret_code != 0 {
        return Err(ExecutionError::Fetch(format!(
            "bybit tickers returned {}: {}",
            payload.ret_code, payload.ret_msg
        )));
    }
    let ticker = payload
        .result
        .list
        .into_iter()
        .next()
        .ok_or_else(|| ExecutionError::Fetch("bybit tickers returned no rows".to_string()))?;
    let best_bid = parse_number(&ticker.bid_1_price, "bybit best bid")?;
    let best_ask = parse_number(&ticker.ask_1_price, "bybit best ask")?;
    let top_of_book = Some(TopOfBookSnapshot {
        time_ms: now_ms,
        best_bid,
        best_ask,
        mid_price: (best_bid + best_ask) / 2.0,
        state: FeedSnapshotState::Live,
    });
    let last_price = Some(PriceSnapshot {
        time_ms: now_ms,
        price: parse_number(&ticker.last_price, "bybit last price")?,
        state: FeedSnapshotState::Live,
    });
    let mark_price = ticker
        .mark_price
        .map(|price| {
            Ok(PriceSnapshot {
                time_ms: now_ms,
                price: parse_number(&price, "bybit mark price")?,
                state: FeedSnapshotState::Live,
            })
        })
        .transpose()?;
    Ok(QuoteFeedData {
        top_of_book,
        last_price,
        mark_price,
    })
}

fn parse_number(raw: &str, field: &str) -> Result<f64, ExecutionError> {
    raw.parse::<f64>()
        .map_err(|err| ExecutionError::Fetch(format!("invalid {field} `{raw}`: {err}")))
}

#[cfg(test)]
mod tests {
    use mockito::{Matcher, Server};
    use serde_json::json;

    use super::*;

    fn sample_source(template: SourceTemplate, symbol: &str) -> PaperExecutionSource {
        PaperExecutionSource {
            alias: "exec".to_string(),
            template,
            symbol: symbol.to_string(),
        }
    }

    fn sample_declared_source(template: SourceTemplate, symbol: &str) -> DeclaredMarketSource {
        DeclaredMarketSource {
            id: 0,
            alias: "exec".to_string(),
            template,
            symbol: symbol.to_string(),
        }
    }

    fn endpoints(base_url: String) -> ExchangeEndpoints {
        ExchangeEndpoints {
            bybit_base_url: base_url,
            ..ExchangeEndpoints::default()
        }
    }

    #[test]
    fn validate_rejects_empty_symbol() {
        let err = validate(&sample_declared_source(SourceTemplate::BybitSpot, ""))
            .expect_err("empty symbol should be rejected");
        assert!(matches!(err, ExecutionError::InvalidConfig { .. }));
        assert!(err.to_string().contains("non-empty symbol"));
    }

    #[test]
    fn fetch_quote_feed_spot_uses_spot_category_without_mark_price() {
        let mut server = Server::new();
        let _mock = server
            .mock("GET", "/v5/market/tickers")
            .match_query(Matcher::AllOf(vec![
                Matcher::UrlEncoded("category".into(), "spot".into()),
                Matcher::UrlEncoded("symbol".into(), "BTCUSDT".into()),
            ]))
            .with_status(200)
            .with_body(
                json!({
                    "retCode": 0,
                    "retMsg": "OK",
                    "result": {
                        "list": [{
                            "bid1Price": "10.0",
                            "ask1Price": "12.0",
                            "lastPrice": "11.5"
                        }]
                    }
                })
                .to_string(),
            )
            .create();

        let feed = fetch_quote_feed(
            &Client::new(),
            &endpoints(server.url()),
            &sample_source(SourceTemplate::BybitSpot, "BTCUSDT"),
            123,
        )
        .expect("spot quote feed should parse");

        assert_eq!(
            feed.top_of_book,
            Some(TopOfBookSnapshot {
                time_ms: 123,
                best_bid: 10.0,
                best_ask: 12.0,
                mid_price: 11.0,
                state: FeedSnapshotState::Live,
            })
        );
        assert_eq!(
            feed.last_price,
            Some(PriceSnapshot {
                time_ms: 123,
                price: 11.5,
                state: FeedSnapshotState::Live,
            })
        );
        assert_eq!(feed.mark_price, None);
    }

    #[test]
    fn fetch_quote_feed_usdt_perps_uses_linear_category_and_mark_price() {
        let mut server = Server::new();
        let _mock = server
            .mock("GET", "/v5/market/tickers")
            .match_query(Matcher::AllOf(vec![
                Matcher::UrlEncoded("category".into(), "linear".into()),
                Matcher::UrlEncoded("symbol".into(), "BTCUSDT".into()),
            ]))
            .with_status(200)
            .with_body(
                json!({
                    "retCode": 0,
                    "retMsg": "OK",
                    "result": {
                        "list": [{
                            "bid1Price": "20.0",
                            "ask1Price": "22.0",
                            "lastPrice": "21.5",
                            "markPrice": "21.0"
                        }]
                    }
                })
                .to_string(),
            )
            .create();

        let feed = fetch_quote_feed(
            &Client::new(),
            &endpoints(server.url()),
            &sample_source(SourceTemplate::BybitUsdtPerps, "BTCUSDT"),
            456,
        )
        .expect("perp quote feed should parse");

        assert_eq!(
            feed.mark_price,
            Some(PriceSnapshot {
                time_ms: 456,
                price: 21.0,
                state: FeedSnapshotState::Live,
            })
        );
    }

    #[test]
    fn fetch_quote_feed_reports_api_error_payloads() {
        let mut server = Server::new();
        let _mock = server
            .mock("GET", "/v5/market/tickers")
            .match_query(Matcher::AllOf(vec![
                Matcher::UrlEncoded("category".into(), "linear".into()),
                Matcher::UrlEncoded("symbol".into(), "BTCUSDT".into()),
            ]))
            .with_status(200)
            .with_body(
                json!({
                    "retCode": 10001,
                    "retMsg": "bad symbol",
                    "result": { "list": [] }
                })
                .to_string(),
            )
            .create();

        let err = fetch_quote_feed(
            &Client::new(),
            &endpoints(server.url()),
            &sample_source(SourceTemplate::BybitUsdtPerps, "BTCUSDT"),
            789,
        )
        .expect_err("api error should bubble up");

        assert!(err
            .to_string()
            .contains("bybit tickers returned 10001: bad symbol"));
    }

    #[test]
    fn fetch_quote_feed_reports_invalid_numeric_fields() {
        let mut server = Server::new();
        let _mock = server
            .mock("GET", "/v5/market/tickers")
            .match_query(Matcher::AllOf(vec![
                Matcher::UrlEncoded("category".into(), "spot".into()),
                Matcher::UrlEncoded("symbol".into(), "BTCUSDT".into()),
            ]))
            .with_status(200)
            .with_body(
                json!({
                    "retCode": 0,
                    "retMsg": "OK",
                    "result": {
                        "list": [{
                            "bid1Price": "oops",
                            "ask1Price": "12.0",
                            "lastPrice": "11.5"
                        }]
                    }
                })
                .to_string(),
            )
            .create();

        let err = fetch_quote_feed(
            &Client::new(),
            &endpoints(server.url()),
            &sample_source(SourceTemplate::BybitSpot, "BTCUSDT"),
            999,
        )
        .expect_err("invalid numeric field should fail");

        assert!(err.to_string().contains("invalid bybit best bid `oops`"));
    }
}
