use reqwest::blocking::Client;
use serde::Deserialize;

use crate::exchange::ExchangeEndpoints;
use crate::interval::{DeclaredMarketSource, SourceTemplate};

use super::{decode_json, gate_api_base, QuoteFeedData};
use crate::execution::{
    ExecutionError, FeedSnapshotState, PaperExecutionSource, PriceSnapshot, TopOfBookSnapshot,
};

#[derive(Deserialize)]
struct GateSpotTicker {
    highest_bid: String,
    lowest_ask: String,
    last: String,
}

#[derive(Deserialize)]
struct GateFuturesTicker {
    highest_bid: String,
    lowest_ask: String,
    last: String,
    mark_price: String,
}

pub(crate) fn validate(source: &DeclaredMarketSource) -> Result<(), ExecutionError> {
    if source.symbol.is_empty() {
        return Err(ExecutionError::InvalidConfig {
            message: "gate paper execution requires a non-empty symbol".to_string(),
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
    let base = gate_api_base(&endpoints.gate_base_url);
    match source.template {
        SourceTemplate::GateSpot => {
            let url = format!("{base}/spot/tickers");
            let rows: Vec<GateSpotTicker> = decode_json(
                client
                    .get(&url)
                    .query(&[("currency_pair", source.symbol.as_str())])
                    .send()
                    .map_err(|err| ExecutionError::Fetch(err.to_string()))?,
                &url,
            )?;
            let row = rows.into_iter().next().ok_or_else(|| {
                ExecutionError::Fetch("gate spot tickers returned no rows".to_string())
            })?;
            build_quote_feed(
                now_ms,
                &row.highest_bid,
                &row.lowest_ask,
                &row.last,
                None,
                "gate",
            )
        }
        SourceTemplate::GateUsdtPerps => {
            let url = format!("{base}/futures/usdt/tickers");
            let rows: Vec<GateFuturesTicker> = decode_json(
                client
                    .get(&url)
                    .query(&[("contract", source.symbol.as_str())])
                    .send()
                    .map_err(|err| ExecutionError::Fetch(err.to_string()))?,
                &url,
            )?;
            let row = rows.into_iter().next().ok_or_else(|| {
                ExecutionError::Fetch("gate futures tickers returned no rows".to_string())
            })?;
            build_quote_feed(
                now_ms,
                &row.highest_bid,
                &row.lowest_ask,
                &row.last,
                Some(&row.mark_price),
                "gate",
            )
        }
        _ => unreachable!("gate fetch_quote_feed called for non-gate template"),
    }
}

fn build_quote_feed(
    now_ms: i64,
    bid: &str,
    ask: &str,
    last: &str,
    mark: Option<&str>,
    label: &str,
) -> Result<QuoteFeedData, ExecutionError> {
    let best_bid = parse_number(bid, label, "best bid")?;
    let best_ask = parse_number(ask, label, "best ask")?;
    let top_of_book = Some(TopOfBookSnapshot {
        time_ms: now_ms,
        best_bid,
        best_ask,
        mid_price: (best_bid + best_ask) / 2.0,
        state: FeedSnapshotState::Live,
    });
    let last_price = Some(PriceSnapshot {
        time_ms: now_ms,
        price: parse_number(last, label, "last price")?,
        state: FeedSnapshotState::Live,
    });
    let mark_price = mark
        .map(|value| {
            Ok(PriceSnapshot {
                time_ms: now_ms,
                price: parse_number(value, label, "mark price")?,
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

fn parse_number(raw: &str, venue: &str, field: &str) -> Result<f64, ExecutionError> {
    raw.parse::<f64>()
        .map_err(|err| ExecutionError::Fetch(format!("invalid {venue} {field} `{raw}`: {err}")))
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
            gate_base_url: base_url,
            ..ExchangeEndpoints::default()
        }
    }

    #[test]
    fn validate_rejects_empty_symbol() {
        let err = validate(&sample_declared_source(SourceTemplate::GateSpot, ""))
            .expect_err("empty symbol should be rejected");
        assert!(matches!(err, ExecutionError::InvalidConfig { .. }));
        assert!(err.to_string().contains("non-empty symbol"));
    }

    #[test]
    fn fetch_quote_feed_spot_appends_api_v4_and_parses_first_row() {
        let mut server = Server::new();
        let _mock = server
            .mock("GET", "/api/v4/spot/tickers")
            .match_query(Matcher::UrlEncoded(
                "currency_pair".into(),
                "BTC_USDT".into(),
            ))
            .with_status(200)
            .with_body(
                json!([
                    {
                        "highest_bid": "100.0",
                        "lowest_ask": "102.0",
                        "last": "101.5"
                    },
                    {
                        "highest_bid": "1.0",
                        "lowest_ask": "2.0",
                        "last": "1.5"
                    }
                ])
                .to_string(),
            )
            .create();

        let feed = fetch_quote_feed(
            &Client::new(),
            &endpoints(server.url()),
            &sample_source(SourceTemplate::GateSpot, "BTC_USDT"),
            321,
        )
        .expect("spot quote feed should parse");

        assert_eq!(
            feed.top_of_book,
            Some(TopOfBookSnapshot {
                time_ms: 321,
                best_bid: 100.0,
                best_ask: 102.0,
                mid_price: 101.0,
                state: FeedSnapshotState::Live,
            })
        );
        assert_eq!(
            feed.last_price,
            Some(PriceSnapshot {
                time_ms: 321,
                price: 101.5,
                state: FeedSnapshotState::Live,
            })
        );
        assert_eq!(feed.mark_price, None);
    }

    #[test]
    fn fetch_quote_feed_usdt_perps_parses_mark_price() {
        let mut server = Server::new();
        let _mock = server
            .mock("GET", "/api/v4/futures/usdt/tickers")
            .match_query(Matcher::UrlEncoded("contract".into(), "BTC_USDT".into()))
            .with_status(200)
            .with_body(
                json!([
                    {
                        "highest_bid": "200.0",
                        "lowest_ask": "202.0",
                        "last": "201.5",
                        "mark_price": "201.0"
                    }
                ])
                .to_string(),
            )
            .create();

        let feed = fetch_quote_feed(
            &Client::new(),
            &endpoints(server.url()),
            &sample_source(SourceTemplate::GateUsdtPerps, "BTC_USDT"),
            654,
        )
        .expect("perp quote feed should parse");

        assert_eq!(
            feed.mark_price,
            Some(PriceSnapshot {
                time_ms: 654,
                price: 201.0,
                state: FeedSnapshotState::Live,
            })
        );
    }

    #[test]
    fn fetch_quote_feed_reports_empty_rows() {
        let mut server = Server::new();
        let _mock = server
            .mock("GET", "/api/v4/spot/tickers")
            .match_query(Matcher::UrlEncoded(
                "currency_pair".into(),
                "BTC_USDT".into(),
            ))
            .with_status(200)
            .with_body("[]")
            .create();

        let err = fetch_quote_feed(
            &Client::new(),
            &endpoints(server.url()),
            &sample_source(SourceTemplate::GateSpot, "BTC_USDT"),
            777,
        )
        .expect_err("empty rows should fail");

        assert!(err
            .to_string()
            .contains("gate spot tickers returned no rows"));
    }

    #[test]
    fn fetch_quote_feed_reports_invalid_numeric_fields() {
        let mut server = Server::new();
        let _mock = server
            .mock("GET", "/api/v4/futures/usdt/tickers")
            .match_query(Matcher::UrlEncoded("contract".into(), "BTC_USDT".into()))
            .with_status(200)
            .with_body(
                json!([
                    {
                        "highest_bid": "bad",
                        "lowest_ask": "202.0",
                        "last": "201.5",
                        "mark_price": "201.0"
                    }
                ])
                .to_string(),
            )
            .create();

        let err = fetch_quote_feed(
            &Client::new(),
            &endpoints(server.url()),
            &sample_source(SourceTemplate::GateUsdtPerps, "BTC_USDT"),
            888,
        )
        .expect_err("invalid numeric field should fail");

        assert!(err.to_string().contains("invalid gate best bid `bad`"));
    }
}
