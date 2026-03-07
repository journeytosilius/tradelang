//! Momentum helpers used by TA-Lib builtin execution.

use crate::diagnostic::RuntimeError;
use crate::types::Value;
use crate::vm::SeriesBuffer;

pub(crate) fn calculate_bop(
    open: &SeriesBuffer,
    high: &SeriesBuffer,
    low: &SeriesBuffer,
    close: &SeriesBuffer,
    pc: usize,
) -> Result<Value, RuntimeError> {
    if open.is_empty() || high.is_empty() || low.is_empty() || close.is_empty() {
        return Ok(Value::NA);
    }

    let Some(open) = expect_buffer_value(open, 0, pc)? else {
        return Ok(Value::NA);
    };
    let Some(high) = expect_buffer_value(high, 0, pc)? else {
        return Ok(Value::NA);
    };
    let Some(low) = expect_buffer_value(low, 0, pc)? else {
        return Ok(Value::NA);
    };
    let Some(close) = expect_buffer_value(close, 0, pc)? else {
        return Ok(Value::NA);
    };

    let range = high - low;
    if range > 0.0 {
        Ok(Value::F64((close - open) / range))
    } else {
        Ok(Value::F64(0.0))
    }
}

pub(crate) fn calculate_cci(
    high: &SeriesBuffer,
    low: &SeriesBuffer,
    close: &SeriesBuffer,
    window: usize,
    pc: usize,
) -> Result<Value, RuntimeError> {
    if high.len() < window || low.len() < window || close.len() < window {
        return Ok(Value::NA);
    }

    let mut total = 0.0;
    for ((high_value, low_value), close_value) in high
        .iter_recent(window)
        .zip(low.iter_recent(window))
        .zip(close.iter_recent(window))
    {
        let Some(typical_price) = typical_price(high_value, low_value, close_value, pc)? else {
            return Ok(Value::NA);
        };
        total += typical_price;
    }
    let average = total / window as f64;

    let mut mean_deviation_total = 0.0;
    let mut current_typical_price = None;
    for (index, ((high_value, low_value), close_value)) in high
        .iter_recent(window)
        .zip(low.iter_recent(window))
        .zip(close.iter_recent(window))
        .enumerate()
    {
        let Some(typical_price) = typical_price(high_value, low_value, close_value, pc)? else {
            return Ok(Value::NA);
        };
        if index == 0 {
            current_typical_price = Some(typical_price);
        }
        mean_deviation_total += (typical_price - average).abs();
    }

    let current_typical_price = current_typical_price.expect("window > 0");
    let mean_deviation = mean_deviation_total / window as f64;
    let delta = current_typical_price - average;
    if delta != 0.0 && mean_deviation != 0.0 {
        Ok(Value::F64(delta / (0.015 * mean_deviation)))
    } else {
        Ok(Value::F64(0.0))
    }
}

fn typical_price(
    high: &Value,
    low: &Value,
    close: &Value,
    pc: usize,
) -> Result<Option<f64>, RuntimeError> {
    let Some(high) = expect_value(high, pc)? else {
        return Ok(None);
    };
    let Some(low) = expect_value(low, pc)? else {
        return Ok(None);
    };
    let Some(close) = expect_value(close, pc)? else {
        return Ok(None);
    };
    Ok(Some((high + low + close) / 3.0))
}

fn expect_buffer_value(
    buffer: &SeriesBuffer,
    offset: usize,
    pc: usize,
) -> Result<Option<f64>, RuntimeError> {
    match buffer.get(offset) {
        Value::F64(value) => Ok(Some(value)),
        Value::NA => Ok(None),
        other => Err(RuntimeError::TypeMismatch {
            pc,
            expected: "f64",
            found: other.type_name(),
        }),
    }
}

fn expect_value(value: &Value, pc: usize) -> Result<Option<f64>, RuntimeError> {
    match value {
        Value::F64(value) => Ok(Some(*value)),
        Value::NA => Ok(None),
        other => Err(RuntimeError::TypeMismatch {
            pc,
            expected: "f64",
            found: other.type_name(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::{calculate_bop, calculate_cci};
    use crate::types::Value;
    use crate::vm::SeriesBuffer;

    fn assert_f64_eq(value: Value, expected: f64) {
        match value {
            Value::F64(value) => assert!((value - expected).abs() < 1e-9),
            other => panic!("expected f64, got {other:?}"),
        }
    }

    #[test]
    fn bop_returns_zero_when_range_is_zero() {
        let mut open = SeriesBuffer::new(4);
        let mut high = SeriesBuffer::new(4);
        let mut low = SeriesBuffer::new(4);
        let mut close = SeriesBuffer::new(4);

        open.push(Value::F64(10.0));
        high.push(Value::F64(10.0));
        low.push(Value::F64(10.0));
        close.push(Value::F64(11.0));

        assert_eq!(
            calculate_bop(&open, &high, &low, &close, 0).unwrap(),
            Value::F64(0.0)
        );
    }

    #[test]
    fn cci_matches_simple_rising_window() {
        let mut high = SeriesBuffer::new(8);
        let mut low = SeriesBuffer::new(8);
        let mut close = SeriesBuffer::new(8);
        for value in [100.0, 101.0, 102.0] {
            high.push(Value::F64(value + 1.0));
            low.push(Value::F64(value - 1.0));
            close.push(Value::F64(value));
        }

        assert_f64_eq(calculate_cci(&high, &low, &close, 3, 0).unwrap(), 100.0);
    }
}
