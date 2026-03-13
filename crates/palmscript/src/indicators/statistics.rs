//! Stateless statistical helpers used by TA-Lib rolling statistics builtins.

use std::f64::consts::PI;

use crate::diagnostic::RuntimeError;
use crate::types::Value;
use crate::vm::SeriesBuffer;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RegressionOutput {
    Value,
    Angle,
    Intercept,
    Slope,
    Forecast,
}

pub(crate) fn calculate_var(
    buffer: &SeriesBuffer,
    window: usize,
    _deviations: f64,
    pc: usize,
) -> Result<Value, RuntimeError> {
    match rolling_moments(buffer, window, pc)? {
        Some((sum, sum_sq)) => {
            let mean = sum / window as f64;
            let mean_sq = sum_sq / window as f64;
            Ok(Value::F64(mean_sq - mean * mean))
        }
        None => Ok(Value::NA),
    }
}

pub(crate) fn calculate_stddev(
    buffer: &SeriesBuffer,
    window: usize,
    deviations: f64,
    pc: usize,
) -> Result<Value, RuntimeError> {
    let variance = match calculate_var(buffer, window, deviations, pc)? {
        Value::F64(value) => value,
        Value::NA => return Ok(Value::NA),
        other => {
            return Err(RuntimeError::TypeMismatch {
                pc,
                expected: "f64",
                found: other.type_name(),
            });
        }
    };

    if variance > 0.0 {
        Ok(Value::F64(variance.sqrt() * deviations))
    } else {
        Ok(Value::F64(0.0))
    }
}

pub(crate) fn calculate_percentile(
    buffer: &SeriesBuffer,
    window: usize,
    percentile: f64,
    pc: usize,
) -> Result<Value, RuntimeError> {
    let Some(mut values) = rolling_values(buffer, window, pc)? else {
        return Ok(Value::NA);
    };
    values.sort_by(|left, right| left.partial_cmp(right).unwrap_or(std::cmp::Ordering::Equal));
    let rank = percentile.clamp(0.0, 100.0) / 100.0 * (window.saturating_sub(1) as f64);
    let lower_index = rank.floor() as usize;
    let upper_index = rank.ceil() as usize;
    if lower_index == upper_index {
        return Ok(Value::F64(values[lower_index]));
    }
    let fraction = rank - lower_index as f64;
    let lower = values[lower_index];
    let upper = values[upper_index];
    Ok(Value::F64(lower + (upper - lower) * fraction))
}

pub(crate) fn calculate_zscore(
    buffer: &SeriesBuffer,
    window: usize,
    pc: usize,
) -> Result<Value, RuntimeError> {
    let Some((sum, sum_sq)) = rolling_moments(buffer, window, pc)? else {
        return Ok(Value::NA);
    };
    let current = match buffer.get(0) {
        Value::F64(value) => value,
        Value::NA => return Ok(Value::NA),
        other => {
            return Err(RuntimeError::TypeMismatch {
                pc,
                expected: "f64",
                found: other.type_name(),
            });
        }
    };
    let mean = sum / window as f64;
    let variance = (sum_sq / window as f64) - mean * mean;
    if variance <= 0.0 {
        return Ok(Value::F64(0.0));
    }
    Ok(Value::F64((current - mean) / variance.sqrt()))
}

pub(crate) fn calculate_ulcer_index(
    buffer: &SeriesBuffer,
    window: usize,
    pc: usize,
) -> Result<Value, RuntimeError> {
    let Some(values) = rolling_values(buffer, window, pc)? else {
        return Ok(Value::NA);
    };

    let mut peak = f64::NEG_INFINITY;
    let mut sum_sq = 0.0;
    for value in values {
        peak = peak.max(value);
        let drawdown_pct = if peak > 0.0 {
            ((value / peak) - 1.0) * 100.0
        } else {
            0.0
        };
        sum_sq += drawdown_pct * drawdown_pct;
    }
    Ok(Value::F64((sum_sq / window as f64).sqrt()))
}

pub(crate) fn calculate_linear_regression(
    buffer: &SeriesBuffer,
    window: usize,
    output: RegressionOutput,
    pc: usize,
) -> Result<Value, RuntimeError> {
    let Some((slope, intercept)) = regression_coefficients(buffer, window, pc)? else {
        return Ok(Value::NA);
    };

    let value = match output {
        RegressionOutput::Value => intercept + slope * (window - 1) as f64,
        RegressionOutput::Angle => slope.atan() * (180.0 / PI),
        RegressionOutput::Intercept => intercept,
        RegressionOutput::Slope => slope,
        RegressionOutput::Forecast => intercept + slope * window as f64,
    };
    Ok(Value::F64(value))
}

pub(crate) fn calculate_beta(
    buffer0: &SeriesBuffer,
    buffer1: &SeriesBuffer,
    window: usize,
    pc: usize,
) -> Result<Value, RuntimeError> {
    if window == 0 || buffer0.len() < window + 1 || buffer1.len() < window + 1 {
        return Ok(Value::NA);
    }

    let mut sum_x = 0.0;
    let mut sum_y = 0.0;
    let mut sum_xx = 0.0;
    let mut sum_xy = 0.0;

    let Some(mut last_x) = expect_buffer_f64(buffer0, window, pc)? else {
        return Ok(Value::NA);
    };
    let Some(mut last_y) = expect_buffer_f64(buffer1, window, pc)? else {
        return Ok(Value::NA);
    };

    for offset in (0..window).rev() {
        let Some(current_x) = expect_buffer_f64(buffer0, offset, pc)? else {
            return Ok(Value::NA);
        };
        let Some(current_y) = expect_buffer_f64(buffer1, offset, pc)? else {
            return Ok(Value::NA);
        };

        let ratio_x = if last_x != 0.0 {
            (current_x - last_x) / last_x
        } else {
            0.0
        };
        let ratio_y = if last_y != 0.0 {
            (current_y - last_y) / last_y
        } else {
            0.0
        };

        sum_x += ratio_x;
        sum_y += ratio_y;
        sum_xx += ratio_x * ratio_x;
        sum_xy += ratio_x * ratio_y;

        last_x = current_x;
        last_y = current_y;
    }

    let n = window as f64;
    let denominator = n * sum_xx - sum_x * sum_x;
    if denominator == 0.0 {
        return Ok(Value::F64(0.0));
    }

    Ok(Value::F64((n * sum_xy - sum_x * sum_y) / denominator))
}

pub(crate) fn calculate_correl(
    buffer0: &SeriesBuffer,
    buffer1: &SeriesBuffer,
    window: usize,
    pc: usize,
) -> Result<Value, RuntimeError> {
    if window == 0 || buffer0.len() < window || buffer1.len() < window {
        return Ok(Value::NA);
    }

    let mut sum_x = 0.0;
    let mut sum_y = 0.0;
    let mut sum_xy = 0.0;
    let mut sum_x2 = 0.0;
    let mut sum_y2 = 0.0;

    for offset in 0..window {
        let Some(x) = expect_buffer_f64(buffer0, offset, pc)? else {
            return Ok(Value::NA);
        };
        let Some(y) = expect_buffer_f64(buffer1, offset, pc)? else {
            return Ok(Value::NA);
        };
        sum_x += x;
        sum_y += y;
        sum_xy += x * y;
        sum_x2 += x * x;
        sum_y2 += y * y;
    }

    let period = window as f64;
    let denominator = (sum_x2 - (sum_x * sum_x) / period) * (sum_y2 - (sum_y * sum_y) / period);
    if denominator <= 0.0 {
        return Ok(Value::F64(0.0));
    }

    Ok(Value::F64(
        (sum_xy - (sum_x * sum_y) / period) / denominator.sqrt(),
    ))
}

fn rolling_moments(
    buffer: &SeriesBuffer,
    window: usize,
    pc: usize,
) -> Result<Option<(f64, f64)>, RuntimeError> {
    if buffer.len() < window {
        return Ok(None);
    }

    let mut sum = 0.0;
    let mut sum_sq = 0.0;
    for value in buffer.iter_recent(window) {
        match value {
            Value::F64(value) => {
                sum += value;
                sum_sq += value * value;
            }
            Value::NA => return Ok(None),
            other => {
                return Err(RuntimeError::TypeMismatch {
                    pc,
                    expected: "f64",
                    found: other.type_name(),
                });
            }
        }
    }

    Ok(Some((sum, sum_sq)))
}

fn rolling_values(
    buffer: &SeriesBuffer,
    window: usize,
    pc: usize,
) -> Result<Option<Vec<f64>>, RuntimeError> {
    if buffer.len() < window {
        return Ok(None);
    }

    let mut values = Vec::with_capacity(window);
    for offset in (0..window).rev() {
        match buffer.get(offset) {
            Value::F64(value) => values.push(value),
            Value::NA => return Ok(None),
            other => {
                return Err(RuntimeError::TypeMismatch {
                    pc,
                    expected: "f64",
                    found: other.type_name(),
                });
            }
        }
    }
    Ok(Some(values))
}

fn regression_coefficients(
    buffer: &SeriesBuffer,
    window: usize,
    pc: usize,
) -> Result<Option<(f64, f64)>, RuntimeError> {
    if buffer.len() < window {
        return Ok(None);
    }

    let mut sum_xy = 0.0;
    let mut sum_y = 0.0;
    for x in 0..window {
        let offset = window - 1 - x;
        match buffer.get(offset) {
            Value::F64(value) => {
                sum_y += value;
                sum_xy += x as f64 * value;
            }
            Value::NA => return Ok(None),
            other => {
                return Err(RuntimeError::TypeMismatch {
                    pc,
                    expected: "f64",
                    found: other.type_name(),
                });
            }
        }
    }

    let n = window as f64;
    let sum_x = n * (n - 1.0) / 2.0;
    let sum_x_sq = n * (n - 1.0) * (2.0 * n - 1.0) / 6.0;
    let divisor = n * sum_x_sq - sum_x * sum_x;
    if divisor == 0.0 {
        return Ok(Some((0.0, sum_y / n)));
    }

    let slope = (n * sum_xy - sum_x * sum_y) / divisor;
    let intercept = (sum_y - slope * sum_x) / n;
    Ok(Some((slope, intercept)))
}

fn expect_buffer_f64(
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

#[cfg(test)]
mod tests {
    use super::{
        calculate_beta, calculate_correl, calculate_linear_regression, calculate_percentile,
        calculate_stddev, calculate_ulcer_index, calculate_var, calculate_zscore, RegressionOutput,
    };
    use crate::types::Value;
    use crate::vm::SeriesBuffer;

    #[test]
    fn variance_matches_population_variance() {
        let mut buffer = SeriesBuffer::new(8);
        for value in [1.0, 2.0, 3.0, 4.0, 5.0] {
            buffer.push(Value::F64(value));
        }

        assert_eq!(calculate_var(&buffer, 5, 3.0, 0).unwrap(), Value::F64(2.0));
    }

    #[test]
    fn stddev_applies_deviation_multiplier() {
        let mut buffer = SeriesBuffer::new(8);
        for value in [1.0, 2.0, 3.0, 4.0, 5.0] {
            buffer.push(Value::F64(value));
        }

        assert_eq!(
            calculate_stddev(&buffer, 5, 2.0, 0).unwrap(),
            Value::F64(2.0 * 2.0_f64.sqrt())
        );
    }

    #[test]
    fn linear_regression_family_matches_simple_line() {
        let mut buffer = SeriesBuffer::new(8);
        for value in [1.0, 2.0, 3.0, 4.0, 5.0] {
            buffer.push(Value::F64(value));
        }

        assert_eq!(
            calculate_linear_regression(&buffer, 5, RegressionOutput::Value, 0).unwrap(),
            Value::F64(5.0)
        );
        assert_eq!(
            calculate_linear_regression(&buffer, 5, RegressionOutput::Intercept, 0).unwrap(),
            Value::F64(1.0)
        );
        assert_eq!(
            calculate_linear_regression(&buffer, 5, RegressionOutput::Slope, 0).unwrap(),
            Value::F64(1.0)
        );
        assert_eq!(
            calculate_linear_regression(&buffer, 5, RegressionOutput::Forecast, 0).unwrap(),
            Value::F64(6.0)
        );
        assert_eq!(
            calculate_linear_regression(&buffer, 5, RegressionOutput::Angle, 0).unwrap(),
            Value::F64(45.0)
        );
    }

    #[test]
    fn correl_matches_identical_series() {
        let mut left = SeriesBuffer::new(8);
        let mut right = SeriesBuffer::new(8);
        for value in [1.0, 2.0, 3.0, 4.0, 5.0] {
            left.push(Value::F64(value));
            right.push(Value::F64(value));
        }

        assert_eq!(
            calculate_correl(&left, &right, 5, 0).unwrap(),
            Value::F64(1.0)
        );
    }

    #[test]
    fn beta_returns_zero_when_input_returns_are_flat() {
        let mut left = SeriesBuffer::new(8);
        let mut right = SeriesBuffer::new(8);
        for value in [10.0, 10.0, 10.0, 10.0, 10.0, 10.0] {
            left.push(Value::F64(value));
            right.push(Value::F64(value));
        }

        assert_eq!(
            calculate_beta(&left, &right, 5, 0).unwrap(),
            Value::F64(0.0)
        );
    }

    #[test]
    fn beta_matches_simple_proportional_return_series() {
        let mut left = SeriesBuffer::new(8);
        let mut right = SeriesBuffer::new(8);
        for value in [10.0, 11.0, 12.0, 13.0] {
            left.push(Value::F64(value));
            right.push(Value::F64(2.0 * value));
        }

        let Value::F64(beta) = calculate_beta(&left, &right, 3, 0).unwrap() else {
            panic!("beta should return f64");
        };
        assert!(
            (beta - 1.0).abs() < 1e-12,
            "expected beta 1.0, found {beta}"
        );
    }

    #[test]
    fn percentile_interpolates_sorted_window_values() {
        let mut buffer = SeriesBuffer::new(8);
        for value in [1.0, 2.0, 3.0, 4.0, 100.0] {
            buffer.push(Value::F64(value));
        }

        let result = calculate_percentile(&buffer, 5, 90.0, 0).expect("percentile should compute");
        assert_eq!(result, Value::F64(61.60000000000001));
    }

    #[test]
    fn zscore_returns_zero_for_flat_window() {
        let mut buffer = SeriesBuffer::new(8);
        for value in [5.0, 5.0, 5.0, 5.0, 5.0] {
            buffer.push(Value::F64(value));
        }

        let result = calculate_zscore(&buffer, 5, 0).expect("zscore should compute");
        assert_eq!(result, Value::F64(0.0));
    }

    #[test]
    fn ulcer_index_measures_drawdown_depth() {
        let mut buffer = SeriesBuffer::new(8);
        for value in [100.0, 110.0, 105.0, 90.0, 95.0] {
            buffer.push(Value::F64(value));
        }

        let result = calculate_ulcer_index(&buffer, 5, 0).expect("ulcer index should compute");
        match result {
            Value::F64(value) => assert!(value > 0.0),
            other => panic!("unexpected ulcer index value: {other:?}"),
        }
    }
}
