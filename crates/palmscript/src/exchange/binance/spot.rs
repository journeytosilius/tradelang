use std::fmt;

use reqwest::blocking::Client;
use reqwest::StatusCode;
use serde::de::{self, Deserializer, IgnoredAny, SeqAccess, Visitor};
use serde::{Deserialize, Serialize};

use crate::exchange::common::{http_status_message, no_data, parse_text_f64, request_failed};
use crate::exchange::ExchangeFetchError;
use crate::interval::{DeclaredMarketSource, Interval};
use crate::runtime::Bar;

const PAGE_LIMIT: usize = 1000;

pub(crate) struct BinanceKlineEndpoint {
    pub path: &'static str,
    pub page_limit: usize,
}

#[derive(Clone, Debug, Serialize)]
struct BinanceKlineQuery<'a> {
    symbol: &'a str,
    interval: &'a str,
    #[serde(rename = "startTime")]
    start_time: i64,
    #[serde(rename = "endTime")]
    end_time: i64,
    limit: usize,
}

#[derive(Clone, Debug)]
pub(crate) struct BinanceKlineRow {
    open_time: i64,
    open: String,
    high: String,
    low: String,
    close: String,
    volume: String,
}

impl BinanceKlineRow {
    fn open_time(&self) -> i64 {
        self.open_time
    }

    pub(crate) fn to_bar(
        &self,
        source: &DeclaredMarketSource,
        interval: Interval,
    ) -> Result<Bar, ExchangeFetchError> {
        Ok(Bar {
            time: self.open_time as f64,
            open: parse_text_f64(&self.open, source, interval, "open")?,
            high: parse_text_f64(&self.high, source, interval, "high")?,
            low: parse_text_f64(&self.low, source, interval, "low")?,
            close: parse_text_f64(&self.close, source, interval, "close")?,
            volume: parse_text_f64(&self.volume, source, interval, "volume")?,
        })
    }
}

impl<'de> Deserialize<'de> for BinanceKlineRow {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct BinanceKlineRowVisitor;

        impl<'de> Visitor<'de> for BinanceKlineRowVisitor {
            type Value = BinanceKlineRow;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a Binance kline array with at least six OHLCV fields")
            }

            fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
            where
                A: SeqAccess<'de>,
            {
                let open_time = seq
                    .next_element()?
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

                Ok(BinanceKlineRow {
                    open_time,
                    open,
                    high,
                    low,
                    close,
                    volume,
                })
            }
        }

        deserializer.deserialize_seq(BinanceKlineRowVisitor)
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
    fetch_binance_bars(
        client,
        source,
        interval,
        from_ms,
        to_ms,
        base_url,
        BinanceKlineEndpoint {
            path: "/api/v3/klines",
            page_limit: PAGE_LIMIT,
        },
    )
}

pub(crate) fn fetch_binance_bars(
    client: &Client,
    source: &DeclaredMarketSource,
    interval: Interval,
    from_ms: i64,
    to_ms: i64,
    base_url: &str,
    endpoint: BinanceKlineEndpoint,
) -> Result<Vec<Bar>, ExchangeFetchError> {
    let mut start_time = from_ms;
    let mut bars: Vec<Bar> = Vec::new();
    loop {
        let response = client
            .get(format!(
                "{}{}",
                base_url.trim_end_matches('/'),
                endpoint.path
            ))
            .query(&BinanceKlineQuery {
                symbol: source.symbol.as_str(),
                interval: interval.as_str(),
                start_time,
                end_time: to_ms.saturating_sub(1),
                limit: endpoint.page_limit,
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
        let rows: Vec<BinanceKlineRow> = response.json().map_err(|err| {
            crate::exchange::common::malformed_response(source, interval, err.to_string())
        })?;
        if rows.is_empty() {
            break;
        }

        let mut last_open = None;
        for row in &rows {
            let bar = row.to_bar(source, interval)?;
            let open_time = bar.time as i64;
            if open_time < from_ms || open_time >= to_ms {
                continue;
            }
            if let Some(previous) = bars.last() {
                let previous_open = previous.time as i64;
                if open_time <= previous_open {
                    return Err(crate::exchange::common::malformed_response(
                        source,
                        interval,
                        "non-increasing kline response".to_string(),
                    ));
                }
            }
            last_open = Some(row.open_time());
            bars.push(bar);
        }

        if rows.len() < endpoint.page_limit {
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
        return Err(no_data(source, interval));
    }
    Ok(bars)
}

#[cfg(test)]
mod tests {
    use super::BinanceKlineRow;
    use crate::interval::{DeclaredMarketSource, Interval, SourceTemplate};
    use crate::runtime::Bar;
    use serde_json::json;

    fn sample_source() -> DeclaredMarketSource {
        DeclaredMarketSource {
            id: 0,
            alias: "src".to_string(),
            template: SourceTemplate::BinanceSpot,
            symbol: "BTCUSDT".to_string(),
        }
    }

    #[test]
    fn binance_kline_row_maps_ohlcv_fields() {
        let source = sample_source();
        let row: BinanceKlineRow = serde_json::from_value(json!([
            1704067200000_i64,
            "1.0",
            "2.0",
            "0.5",
            "1.5",
            "10.0",
            1704067259999_i64,
            "15.0",
            42_u64,
            "6.0",
            "7.0",
            "0"
        ]))
        .expect("row deserializes");
        let bar = row.to_bar(&source, Interval::Min1).expect("row maps");
        assert_eq!(
            bar,
            Bar {
                time: 1704067200000.0,
                open: 1.0,
                high: 2.0,
                low: 0.5,
                close: 1.5,
                volume: 10.0,
            }
        );
    }
}
