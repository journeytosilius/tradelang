use std::fmt;
use std::time::{SystemTime, UNIX_EPOCH};

use reqwest::blocking::{Client, Response};
use reqwest::StatusCode;
use serde::de::{self, Deserializer, Visitor};
use serde::Serialize;

use super::ExchangeFetchError;
use crate::interval::{DeclaredMarketSource, Interval};
use crate::runtime::Bar;

pub(crate) fn deserialize_f64_text<'de, D>(deserializer: D) -> Result<f64, D::Error>
where
    D: Deserializer<'de>,
{
    struct F64TextVisitor;

    impl<'de> Visitor<'de> for F64TextVisitor {
        type Value = f64;

        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str("a float or float-like string")
        }

        fn visit_f64<E>(self, value: f64) -> Result<Self::Value, E> {
            Ok(value)
        }

        fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E> {
            Ok(value as f64)
        }

        fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E> {
            Ok(value as f64)
        }

        fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            value
                .parse::<f64>()
                .map_err(|err| E::custom(err.to_string()))
        }

        fn visit_string<E>(self, value: String) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            self.visit_str(&value)
        }
    }

    deserializer.deserialize_any(F64TextVisitor)
}

pub(crate) fn deserialize_option_f64_text<'de, D>(deserializer: D) -> Result<Option<f64>, D::Error>
where
    D: Deserializer<'de>,
{
    struct OptionF64TextVisitor;

    impl<'de> Visitor<'de> for OptionF64TextVisitor {
        type Value = Option<f64>;

        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str("an optional float or float-like string")
        }

        fn visit_none<E>(self) -> Result<Self::Value, E> {
            Ok(None)
        }

        fn visit_unit<E>(self) -> Result<Self::Value, E> {
            Ok(None)
        }

        fn visit_some<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
        where
            D: Deserializer<'de>,
        {
            deserialize_f64_text(deserializer).map(Some)
        }

        fn visit_f64<E>(self, value: f64) -> Result<Self::Value, E> {
            Ok(Some(value))
        }

        fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E> {
            Ok(Some(value as f64))
        }

        fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E> {
            Ok(Some(value as f64))
        }

        fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            value
                .parse::<f64>()
                .map(Some)
                .map_err(|err| E::custom(err.to_string()))
        }

        fn visit_string<E>(self, value: String) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            self.visit_str(&value)
        }
    }

    deserializer.deserialize_option(OptionF64TextVisitor)
}

pub(crate) fn normalize_margin_percent(raw: f64) -> f64 {
    if raw > 1.0 {
        raw / 100.0
    } else {
        raw
    }
}

pub(crate) fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or(0)
}

pub(crate) fn page_window_end_ms(
    interval: Interval,
    start_open_ms: i64,
    max_candles: usize,
    hard_end_ms: i64,
) -> Option<i64> {
    if start_open_ms >= hard_end_ms {
        return None;
    }
    let mut next_open = start_open_ms;
    for _ in 0..max_candles {
        let Some(candidate) = interval.next_open_time(next_open) else {
            return Some(hard_end_ms);
        };
        next_open = candidate;
        if next_open >= hard_end_ms {
            return Some(hard_end_ms);
        }
    }
    Some(next_open)
}

pub(crate) fn ms_to_api_seconds(timestamp_ms: i64) -> i64 {
    timestamp_ms.div_euclid(1_000)
}

pub(crate) fn push_bar_if_in_window(
    bars: &mut Vec<Bar>,
    bar: Bar,
    source: &DeclaredMarketSource,
    interval: Interval,
    from_ms: i64,
    to_ms: i64,
) -> Result<bool, ExchangeFetchError> {
    let open_time = bar.time as i64;
    if open_time < from_ms || open_time >= to_ms {
        return Ok(false);
    }
    if let Some(previous) = bars.last() {
        let previous_open = previous.time as i64;
        if open_time <= previous_open {
            return Err(malformed_response(
                source,
                interval,
                "non-increasing candle response".to_string(),
            ));
        }
    }
    bars.push(bar);
    Ok(true)
}

pub(crate) fn first_open_time_in_window(
    interval: Interval,
    from_ms: i64,
    to_ms: i64,
) -> Option<i64> {
    if from_ms >= to_ms {
        return None;
    }
    let bucket_open = interval.bucket_open_time(from_ms)?;
    let first_open = if bucket_open >= from_ms {
        bucket_open
    } else {
        interval.next_open_time(bucket_open)?
    };
    (first_open < to_ms).then_some(first_open)
}

pub(crate) fn parse_text_f64(
    value: &str,
    source: &DeclaredMarketSource,
    interval: Interval,
    field: &str,
) -> Result<f64, ExchangeFetchError> {
    value
        .parse::<f64>()
        .map_err(|_| malformed_response(source, interval, format!("invalid `{field}` value")))
}

pub(crate) fn request_failed(
    source: &DeclaredMarketSource,
    interval: Interval,
    message: String,
) -> ExchangeFetchError {
    ExchangeFetchError::RequestFailed {
        alias: source.alias.clone(),
        template: source.template.as_str(),
        symbol: source.symbol.clone(),
        interval: interval.as_str(),
        message,
    }
}

pub(crate) fn malformed_response(
    source: &DeclaredMarketSource,
    interval: Interval,
    message: String,
) -> ExchangeFetchError {
    ExchangeFetchError::MalformedResponse {
        alias: source.alias.clone(),
        template: source.template.as_str(),
        symbol: source.symbol.clone(),
        interval: interval.as_str(),
        message,
    }
}

pub(crate) fn no_data(source: &DeclaredMarketSource, interval: Interval) -> ExchangeFetchError {
    ExchangeFetchError::NoData {
        alias: source.alias.clone(),
        template: source.template.as_str(),
        symbol: source.symbol.clone(),
        interval: interval.as_str(),
    }
}

pub(crate) fn gate_get_with_query_fallback<Q: Serialize + ?Sized>(
    client: &Client,
    base_url: &str,
    path: &str,
    query: &Q,
) -> Result<Response, reqwest::Error> {
    let mut urls = gate_url_candidates(base_url, path).into_iter();
    let first_url = urls
        .next()
        .expect("Gate URL candidates should be non-empty");
    let first_response = client.get(first_url).query(query).send()?;
    if first_response.status() == StatusCode::OK || !gate_base_url_needs_api_prefix(base_url) {
        return Ok(first_response);
    }

    let mut last_response = first_response;
    for url in urls {
        let response = client.get(url).query(query).send()?;
        if response.status() == StatusCode::OK {
            return Ok(response);
        }
        last_response = response;
    }
    Ok(last_response)
}

pub(crate) fn gate_get_fallback(
    client: &Client,
    base_url: &str,
    path: &str,
) -> Result<Response, reqwest::Error> {
    let mut urls = gate_url_candidates(base_url, path).into_iter();
    let first_url = urls
        .next()
        .expect("Gate URL candidates should be non-empty");
    let first_response = client.get(first_url).send()?;
    if first_response.status() == StatusCode::OK || !gate_base_url_needs_api_prefix(base_url) {
        return Ok(first_response);
    }

    let mut last_response = first_response;
    for url in urls {
        let response = client.get(url).send()?;
        if response.status() == StatusCode::OK {
            return Ok(response);
        }
        last_response = response;
    }
    Ok(last_response)
}

fn gate_url_candidates(base_url: &str, path: &str) -> Vec<String> {
    let trimmed = base_url.trim_end_matches('/');
    let mut urls = vec![format!("{trimmed}{path}")];
    if gate_base_url_needs_api_prefix(base_url) {
        urls.push(format!("{trimmed}/api/v4{path}"));
    }
    urls
}

fn gate_base_url_needs_api_prefix(base_url: &str) -> bool {
    !base_url.trim_end_matches('/').ends_with("/api/v4")
}

#[cfg(test)]
mod tests {
    use super::page_window_end_ms;
    use crate::interval::Interval;

    #[test]
    fn page_window_end_advances_by_page_capacity() {
        assert_eq!(
            page_window_end_ms(
                Interval::Min1,
                1_704_067_200_000,
                1_000,
                1_704_067_200_000 + 2_000 * 60_000
            ),
            Some(1_704_067_200_000 + 1_000 * 60_000)
        );
        assert_eq!(
            page_window_end_ms(
                Interval::Hour1,
                1_704_067_200_000,
                2_000,
                1_704_067_200_000 + 24 * 60 * 60 * 1_000
            ),
            Some(1_704_067_200_000 + 24 * 60 * 60 * 1_000)
        );
    }
}
