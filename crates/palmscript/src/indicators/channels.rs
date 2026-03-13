//! Channel and trend-state indicators that do not belong to the TA-Lib set.

use crate::diagnostic::RuntimeError;
use crate::indicators::directional::{DirectionalKind, DirectionalState};
use crate::types::Value;
use crate::vm::SeriesBuffer;

#[derive(Clone, Debug)]
pub(crate) struct SupertrendState {
    atr_state: DirectionalState,
    multiplier: f64,
    last_versions: (u64, u64, u64),
    previous_close: Option<f64>,
    final_upper: Option<f64>,
    final_lower: Option<f64>,
    bullish: bool,
    initialized: bool,
    cached_output: Value,
}

impl SupertrendState {
    pub(crate) fn new(window: usize, multiplier: f64) -> Self {
        Self {
            atr_state: DirectionalState::new(window, DirectionalKind::Atr),
            multiplier,
            last_versions: (0, 0, 0),
            previous_close: None,
            final_upper: None,
            final_lower: None,
            bullish: false,
            initialized: false,
            cached_output: na_trend_tuple(),
        }
    }

    pub(crate) fn update(
        &mut self,
        high: &SeriesBuffer,
        low: &SeriesBuffer,
        close: &SeriesBuffer,
        pc: usize,
    ) -> Result<Value, RuntimeError> {
        let versions = (high.version(), low.version(), close.version());
        if versions == self.last_versions {
            return Ok(self.cached_output.clone());
        }
        self.last_versions = versions;

        let Some(current_high) = expect_buffer_f64(high, 0, pc)? else {
            self.cached_output = na_trend_tuple();
            return Ok(self.cached_output.clone());
        };
        let Some(current_low) = expect_buffer_f64(low, 0, pc)? else {
            self.cached_output = na_trend_tuple();
            return Ok(self.cached_output.clone());
        };
        let Some(current_close) = expect_buffer_f64(close, 0, pc)? else {
            self.cached_output = na_trend_tuple();
            return Ok(self.cached_output.clone());
        };
        let previous_close = self.previous_close;
        self.previous_close = Some(current_close);

        let atr = match self.atr_state.update(high, low, close, pc)? {
            Value::F64(value) => Some(value),
            Value::NA => None,
            other => {
                return Err(RuntimeError::TypeMismatch {
                    pc,
                    expected: "f64",
                    found: other.type_name(),
                });
            }
        };
        let Some(atr) = atr else {
            self.cached_output = na_trend_tuple();
            return Ok(self.cached_output.clone());
        };

        let hl2 = (current_high + current_low) / 2.0;
        let basic_upper = hl2 + self.multiplier * atr;
        let basic_lower = hl2 - self.multiplier * atr;

        if !self.initialized {
            self.initialized = true;
            self.final_upper = Some(basic_upper);
            self.final_lower = Some(basic_lower);
            self.bullish = previous_close.is_some_and(|prior| current_close >= prior);
            self.cached_output = trend_tuple(
                if self.bullish {
                    basic_lower
                } else {
                    basic_upper
                },
                self.bullish,
            );
            return Ok(self.cached_output.clone());
        }

        let previous_final_upper = self.final_upper.unwrap_or(basic_upper);
        let previous_final_lower = self.final_lower.unwrap_or(basic_lower);
        let previous_close = previous_close.unwrap_or(current_close);

        let final_upper =
            if basic_upper < previous_final_upper || previous_close > previous_final_upper {
                basic_upper
            } else {
                previous_final_upper
            };
        let final_lower =
            if basic_lower > previous_final_lower || previous_close < previous_final_lower {
                basic_lower
            } else {
                previous_final_lower
            };

        self.bullish = if self.bullish {
            current_close >= final_lower
        } else {
            current_close > final_upper
        };
        self.final_upper = Some(final_upper);
        self.final_lower = Some(final_lower);
        self.cached_output = trend_tuple(
            if self.bullish {
                final_lower
            } else {
                final_upper
            },
            self.bullish,
        );
        Ok(self.cached_output.clone())
    }
}

pub(crate) fn calculate_donchian(
    high: &SeriesBuffer,
    low: &SeriesBuffer,
    window: usize,
    pc: usize,
) -> Result<Value, RuntimeError> {
    if high.len() < window || low.len() < window {
        return Ok(na_tuple3());
    }

    let mut upper = f64::NEG_INFINITY;
    let mut lower = f64::INFINITY;
    for offset in 0..window {
        let Some(high_value) = expect_buffer_f64(high, offset, pc)? else {
            return Ok(na_tuple3());
        };
        let Some(low_value) = expect_buffer_f64(low, offset, pc)? else {
            return Ok(na_tuple3());
        };
        upper = upper.max(high_value);
        lower = lower.min(low_value);
    }

    Ok(Value::Tuple3([
        Box::new(Value::F64(upper)),
        Box::new(Value::F64((upper + lower) / 2.0)),
        Box::new(Value::F64(lower)),
    ]))
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

fn trend_tuple(line: f64, bullish: bool) -> Value {
    Value::Tuple2([Box::new(Value::F64(line)), Box::new(Value::Bool(bullish))])
}

fn na_trend_tuple() -> Value {
    Value::Tuple2([Box::new(Value::NA), Box::new(Value::NA)])
}

fn na_tuple3() -> Value {
    Value::Tuple3([
        Box::new(Value::NA),
        Box::new(Value::NA),
        Box::new(Value::NA),
    ])
}

#[cfg(test)]
mod tests {
    use super::{calculate_donchian, SupertrendState};
    use crate::types::Value;
    use crate::vm::SeriesBuffer;

    fn series(values: &[f64]) -> SeriesBuffer {
        let mut buffer = SeriesBuffer::new(values.len().max(1));
        for value in values {
            buffer.push(Value::F64(*value));
        }
        buffer
    }

    #[test]
    fn donchian_returns_upper_middle_lower() {
        let high = series(&[10.0, 11.0, 13.0, 12.0]);
        let low = series(&[8.0, 7.0, 9.0, 8.5]);
        let result = calculate_donchian(&high, &low, 3, 0).expect("donchian should compute");
        match result {
            Value::Tuple3(values) => {
                assert_eq!(*values[0], Value::F64(13.0));
                assert_eq!(*values[1], Value::F64(10.0));
                assert_eq!(*values[2], Value::F64(7.0));
            }
            other => panic!("unexpected donchian result: {other:?}"),
        }
    }

    #[test]
    fn supertrend_eventually_emits_line_and_direction() {
        let highs = [10.0, 11.0, 12.0, 13.0, 14.0, 14.5, 14.0, 13.5];
        let lows = [9.0, 9.5, 10.5, 11.5, 12.5, 12.0, 11.0, 10.5];
        let closes = [9.5, 10.5, 11.5, 12.5, 13.5, 12.2, 11.2, 10.8];
        let mut high = SeriesBuffer::new(highs.len());
        let mut low = SeriesBuffer::new(lows.len());
        let mut close = SeriesBuffer::new(closes.len());
        let mut state = SupertrendState::new(3, 2.0);

        let mut final_output = Value::NA;
        for index in 0..highs.len() {
            high.push(Value::F64(highs[index]));
            low.push(Value::F64(lows[index]));
            close.push(Value::F64(closes[index]));
            final_output = state
                .update(&high, &low, &close, 0)
                .expect("supertrend should update");
        }

        match final_output {
            Value::Tuple2(values) => {
                assert!(matches!(*values[0], Value::F64(_)));
                assert!(matches!(*values[1], Value::Bool(_)));
            }
            other => panic!("unexpected supertrend tuple: {other:?}"),
        }
    }
}
