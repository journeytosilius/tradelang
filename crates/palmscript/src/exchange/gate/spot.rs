use std::fmt;

use reqwest::blocking::Client;
use reqwest::StatusCode;
use serde::de::{self, Deserializer, IgnoredAny, SeqAccess, Visitor};
use serde::{Deserialize, Serialize};

use super::spot_interval_text;
use crate::exchange::common::{
    decode_json_response, deserialize_i64_text, first_open_time_in_window,
    gate_get_with_query_fallback, http_status_message, malformed_response, ms_to_api_seconds,
    no_data, page_window_end_ms, parse_text_f64, push_bar_if_in_window, request_failed,
};
use crate::exchange::ExchangeFetchError;
use crate::interval::{DeclaredMarketSource, Interval};
use crate::runtime::Bar;

const PAGE_LIMIT: usize = 1000;

#[derive(Clone, Debug, Serialize)]
struct GateSpotCandlesticksQuery<'a> {
    currency_pair: &'a str,
    interval: &'a str,
    from: i64,
    to: i64,
}

#[derive(Clone, Debug)]
pub(crate) struct GateSpotCandlestickRow {
    timestamp_secs: i64,
    close: String,
    high: String,
    low: String,
    open: String,
    base_volume: String,
}

#[derive(Deserialize)]
struct GateSpotTimestamp(#[serde(deserialize_with = "deserialize_i64_text")] i64);

impl GateSpotCandlestickRow {
    fn open_time_ms(&self) -> i64 {
        self.timestamp_secs.saturating_mul(1_000)
    }

    pub(crate) fn to_bar(
        &self,
        source: &DeclaredMarketSource,
        interval: Interval,
    ) -> Result<Bar, ExchangeFetchError> {
        Ok(Bar {
            time: self.open_time_ms() as f64,
            open: parse_text_f64(&self.open, source, interval, "open")?,
            high: parse_text_f64(&self.high, source, interval, "high")?,
            low: parse_text_f64(&self.low, source, interval, "low")?,
            close: parse_text_f64(&self.close, source, interval, "close")?,
            volume: parse_text_f64(&self.base_volume, source, interval, "volume")?,
        })
    }
}

impl<'de> Deserialize<'de> for GateSpotCandlestickRow {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct GateSpotCandlestickRowVisitor;

        impl<'de> Visitor<'de> for GateSpotCandlestickRowVisitor {
            type Value = GateSpotCandlestickRow;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a Gate spot candlestick array with seven fields")
            }

            fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
            where
                A: SeqAccess<'de>,
            {
                let timestamp_secs = seq
                    .next_element::<GateSpotTimestamp>()?
                    .ok_or_else(|| de::Error::invalid_length(0, &self))?
                    .0;
                let _quote_volume: String = seq
                    .next_element()?
                    .ok_or_else(|| de::Error::invalid_length(1, &self))?;
                let close = seq
                    .next_element()?
                    .ok_or_else(|| de::Error::invalid_length(2, &self))?;
                let high = seq
                    .next_element()?
                    .ok_or_else(|| de::Error::invalid_length(3, &self))?;
                let low = seq
                    .next_element()?
                    .ok_or_else(|| de::Error::invalid_length(4, &self))?;
                let open = seq
                    .next_element()?
                    .ok_or_else(|| de::Error::invalid_length(5, &self))?;
                let base_volume = seq
                    .next_element()?
                    .ok_or_else(|| de::Error::invalid_length(6, &self))?;

                while let Some(IgnoredAny) = seq.next_element()? {}

                Ok(GateSpotCandlestickRow {
                    timestamp_secs,
                    close,
                    high,
                    low,
                    open,
                    base_volume,
                })
            }
        }

        deserializer.deserialize_seq(GateSpotCandlestickRowVisitor)
    }
}

pub(crate) fn fetch_bars(
    client: &Client,
    source: &DeclaredMarketSource,
    interval: Interval,
    from_ms: i64,
    to_ms: i64,
    base_url: &str,
) -> Result<Vec<Bar>, ExchangeFetchError> {
    let interval_text = spot_interval_text(interval).ok_or_else(|| {
        malformed_response(
            source,
            interval,
            "unsupported Gate spot interval".to_string(),
        )
    })?;
    let Some(mut window_start_ms) = first_open_time_in_window(interval, from_ms, to_ms) else {
        return Err(no_data(source, interval));
    };
    let mut bars = Vec::new();

    while window_start_ms < to_ms {
        let window_end_ms = page_window_end_ms(interval, window_start_ms, PAGE_LIMIT, to_ms)
            .ok_or_else(|| {
                malformed_response(
                    source,
                    interval,
                    "unable to advance Gate spot pagination".to_string(),
                )
            })?;
        let response = gate_get_with_query_fallback(
            client,
            base_url,
            "/spot/candlesticks",
            &GateSpotCandlesticksQuery {
                currency_pair: source.symbol.as_str(),
                interval: interval_text,
                from: ms_to_api_seconds(window_start_ms),
                to: ms_to_api_seconds(window_end_ms),
            },
        )
        .map_err(|err| request_failed(source, interval, err.to_string()))?;
        if response.status() != StatusCode::OK {
            return Err(request_failed(
                source,
                interval,
                http_status_message(response),
            ));
        }
        let mut rows: Vec<GateSpotCandlestickRow> =
            decode_json_response(response, source, interval)?;
        rows.sort_by_key(GateSpotCandlestickRow::open_time_ms);

        for row in &rows {
            let bar = row.to_bar(source, interval)?;
            push_bar_if_in_window(&mut bars, bar, source, interval, from_ms, to_ms)?;
        }

        let Some(next_window_start) = interval.next_open_time(window_end_ms) else {
            break;
        };
        if next_window_start >= to_ms {
            break;
        }
        window_start_ms = next_window_start;
    }

    if bars.is_empty() {
        return Err(no_data(source, interval));
    }
    Ok(bars)
}

#[cfg(test)]
mod tests {
    use super::{fetch_bars, GateSpotCandlestickRow};
    use crate::interval::{DeclaredMarketSource, Interval, SourceTemplate};
    use mockito::{Matcher, Server};
    use reqwest::blocking::Client;
    use serde_json::json;

    #[test]
    fn gate_spot_candlestick_row_maps_ohlcv_fields() {
        let source = DeclaredMarketSource {
            id: 0,
            alias: "src".to_string(),
            template: SourceTemplate::GateSpot,
            symbol: "BTC_USDT".to_string(),
        };
        let row: GateSpotCandlestickRow = serde_json::from_value(json!([
            "1704067200",
            "15.0",
            "1.5",
            "2.0",
            "0.5",
            "1.0",
            "10.0",
            "true"
        ]))
        .expect("row deserializes");
        let bar = row.to_bar(&source, Interval::Min1).expect("row maps");
        assert_eq!(bar.time, 1704067200000.0);
        assert_eq!(bar.volume, 10.0);
    }

    #[test]
    fn gate_spot_fetch_caps_first_page_to_1000_inclusive_candles() {
        let source = DeclaredMarketSource {
            id: 0,
            alias: "src".to_string(),
            template: SourceTemplate::GateSpot,
            symbol: "BTC_USDT".to_string(),
        };
        let mut server = Server::new();
        let _candles = server
            .mock("GET", "/spot/candlesticks")
            .match_query(Matcher::AllOf(vec![
                Matcher::UrlEncoded("currency_pair".into(), "BTC_USDT".into()),
                Matcher::UrlEncoded("interval".into(), "4h".into()),
                Matcher::UrlEncoded("from".into(), "1640995200".into()),
                Matcher::UrlEncoded("to".into(), "1655380800".into()),
            ]))
            .with_status(200)
            .with_body(
                json!([
                    [
                        "1640995200",
                        "15.0",
                        "1.5",
                        "2.0",
                        "0.5",
                        "1.0",
                        "10.0",
                        "true"
                    ],
                    [
                        "1655380800",
                        "16.0",
                        "2.5",
                        "3.0",
                        "1.5",
                        "2.0",
                        "11.0",
                        "true"
                    ]
                ])
                .to_string(),
            )
            .create();

        let bars = fetch_bars(
            &Client::new(),
            &source,
            Interval::Hour4,
            1_640_995_200_000,
            1_655_395_200_000,
            &server.url(),
        )
        .expect("bars");

        assert_eq!(bars.len(), 2);
    }
}
