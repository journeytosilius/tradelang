use std::fmt;

use reqwest::blocking::Client;
use reqwest::StatusCode;
use serde::de::{self, Deserializer, IgnoredAny, SeqAccess, Visitor};
use serde::{Deserialize, Serialize};

use super::interval_text;
use crate::exchange::common::{
    decode_json_response, deserialize_i64_text, http_status_message, malformed_response, no_data,
    parse_text_f64, push_bar_if_in_window, request_failed,
};
use crate::exchange::ExchangeFetchError;
use crate::interval::{DeclaredMarketSource, Interval};
use crate::runtime::Bar;

const PAGE_LIMIT: usize = 1000;

#[derive(Clone, Debug, Serialize)]
struct BybitKlineQuery<'a> {
    category: &'static str,
    symbol: &'a str,
    interval: &'a str,
    start: i64,
    end: i64,
    limit: usize,
}

#[derive(Clone, Debug)]
pub(crate) struct BybitKlineRow {
    start_time_ms: i64,
    open: String,
    high: String,
    low: String,
    close: String,
    volume: String,
}

impl BybitKlineRow {
    fn open_time(&self) -> i64 {
        self.start_time_ms
    }

    pub(crate) fn to_bar(
        &self,
        source: &DeclaredMarketSource,
        interval: Interval,
    ) -> Result<Bar, ExchangeFetchError> {
        Ok(Bar {
            time: self.start_time_ms as f64,
            open: parse_text_f64(&self.open, source, interval, "open")?,
            high: parse_text_f64(&self.high, source, interval, "high")?,
            low: parse_text_f64(&self.low, source, interval, "low")?,
            close: parse_text_f64(&self.close, source, interval, "close")?,
            volume: parse_text_f64(&self.volume, source, interval, "volume")?,
            funding_rate: None,
            open_interest: None,
            mark_price: None,
            index_price: None,
            premium_index: None,
            basis: None,
        })
    }
}

impl<'de> Deserialize<'de> for BybitKlineRow {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct I64TextValue(#[serde(deserialize_with = "deserialize_i64_text")] i64);

        struct BybitKlineRowVisitor;

        impl<'de> Visitor<'de> for BybitKlineRowVisitor {
            type Value = BybitKlineRow;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a Bybit kline array with at least six OHLCV fields")
            }

            fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
            where
                A: SeqAccess<'de>,
            {
                let start_time_ms = seq
                    .next_element()?
                    .map(|value: I64TextValue| value.0)
                    .ok_or_else(|| de::Error::invalid_length(0, &self))?;
                let open = seq
                    .next_element()?
                    .ok_or_else(|| de::Error::invalid_length(1, &self))?;
                let high = seq
                    .next_element()?
                    .ok_or_else(|| de::Error::invalid_length(2, &self))?;
                let low = seq
                    .next_element()?
                    .ok_or_else(|| de::Error::invalid_length(3, &self))?;
                let close = seq
                    .next_element()?
                    .ok_or_else(|| de::Error::invalid_length(4, &self))?;
                let volume = seq
                    .next_element()?
                    .ok_or_else(|| de::Error::invalid_length(5, &self))?;

                while let Some(IgnoredAny) = seq.next_element()? {}

                Ok(BybitKlineRow {
                    start_time_ms,
                    open,
                    high,
                    low,
                    close,
                    volume,
                })
            }
        }

        deserializer.deserialize_seq(BybitKlineRowVisitor)
    }
}

#[derive(Clone, Debug, Deserialize)]
struct BybitEnvelope<T> {
    #[serde(rename = "retCode")]
    ret_code: i32,
    #[serde(rename = "retMsg")]
    ret_msg: String,
    result: Option<T>,
}

#[derive(Clone, Debug, Deserialize)]
struct BybitKlineResult<T> {
    list: Vec<T>,
}

pub(crate) fn fetch_bars(
    client: &Client,
    source: &DeclaredMarketSource,
    interval: Interval,
    from_ms: i64,
    to_ms: i64,
    base_url: &str,
) -> Result<Vec<Bar>, ExchangeFetchError> {
    fetch_bybit_bars(client, source, interval, from_ms, to_ms, base_url, "spot")
}

pub(crate) fn fetch_bybit_bars(
    client: &Client,
    source: &DeclaredMarketSource,
    interval: Interval,
    from_ms: i64,
    to_ms: i64,
    base_url: &str,
    category: &'static str,
) -> Result<Vec<Bar>, ExchangeFetchError> {
    let interval_text = interval_text(interval).ok_or_else(|| {
        malformed_response(source, interval, "unsupported Bybit interval".to_string())
    })?;
    let mut start_time = from_ms;
    let mut bars = Vec::new();

    loop {
        let response = client
            .get(format!(
                "{}/v5/market/kline",
                base_url.trim_end_matches('/')
            ))
            .query(&BybitKlineQuery {
                category,
                symbol: source.symbol.as_str(),
                interval: interval_text,
                start: start_time,
                end: to_ms.saturating_sub(1),
                limit: PAGE_LIMIT,
            })
            .send()
            .map_err(|err| request_failed(source, interval, err.to_string()))?;
        if response.status() != StatusCode::OK {
            return Err(request_failed(
                source,
                interval,
                http_status_message(response),
            ));
        }
        let payload: BybitEnvelope<BybitKlineResult<BybitKlineRow>> =
            decode_json_response(response, source, interval)?;
        if payload.ret_code != 0 {
            return Err(request_failed(source, interval, payload.ret_msg));
        }
        let mut rows = payload
            .result
            .ok_or_else(|| {
                malformed_response(source, interval, "missing `result` body".to_string())
            })?
            .list;
        if rows.is_empty() {
            break;
        }
        rows.sort_by_key(BybitKlineRow::open_time);

        let mut last_open = None;
        for row in &rows {
            let bar = row.to_bar(source, interval)?;
            if push_bar_if_in_window(&mut bars, bar, source, interval, from_ms, to_ms)? {
                last_open = Some(row.open_time());
            }
        }

        if rows.len() < PAGE_LIMIT {
            break;
        }
        let Some(last_open) = last_open else {
            break;
        };
        let Some(next_start) = interval.next_open_time(last_open) else {
            break;
        };
        if next_start >= to_ms {
            break;
        }
        start_time = next_start;
    }

    if bars.is_empty() {
        return Err(no_data(source, interval, from_ms, to_ms));
    }
    Ok(bars)
}

#[cfg(test)]
mod tests {
    use super::BybitKlineRow;
    use crate::interval::{DeclaredMarketSource, Interval, SourceTemplate};
    use serde_json::json;

    #[test]
    fn bybit_kline_row_maps_ohlcv_fields() {
        let source = DeclaredMarketSource {
            id: 0,
            alias: "src".to_string(),
            template: SourceTemplate::BybitSpot,
            symbol: "BTCUSDT".to_string(),
        };
        let row: BybitKlineRow = serde_json::from_value(json!([
            1704067200000_i64,
            "1.0",
            "2.0",
            "0.5",
            "1.5",
            "10.0",
            "15.0"
        ]))
        .expect("row deserializes");
        let bar = row.to_bar(&source, Interval::Min1).expect("row maps");
        assert_eq!(bar.close, 1.5);
        assert_eq!(bar.volume, 10.0);
    }

    #[test]
    fn bybit_kline_row_accepts_string_timestamps() {
        let source = DeclaredMarketSource {
            id: 0,
            alias: "src".to_string(),
            template: SourceTemplate::BybitSpot,
            symbol: "BTCUSDT".to_string(),
        };
        let row: BybitKlineRow = serde_json::from_value(json!([
            "1704067200000",
            "1.0",
            "2.0",
            "0.5",
            "1.5",
            "10.0",
            "15.0"
        ]))
        .expect("row deserializes");
        let bar = row.to_bar(&source, Interval::Min1).expect("row maps");
        assert_eq!(bar.time, 1704067200000_f64);
        assert_eq!(bar.close, 1.5);
        assert_eq!(bar.volume, 10.0);
    }
}
