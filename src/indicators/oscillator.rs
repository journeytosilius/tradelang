//! Price oscillator helpers for TA-Lib-style moving-average oscillators.

use crate::diagnostic::RuntimeError;
use crate::talib::MaType;
use crate::types::Value;
use crate::vm::SeriesBuffer;

use super::MovingAverageState;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum OscillatorKind {
    Absolute,
    Percentage,
}

#[derive(Clone, Debug)]
pub(crate) struct PriceOscillatorState {
    kind: OscillatorKind,
    fast_state: MovingAverageState,
    slow_state: MovingAverageState,
    last_seen_version: u64,
    cached_output: Value,
}

impl PriceOscillatorState {
    pub(crate) fn new(
        builtin: &'static str,
        fast_period: usize,
        slow_period: usize,
        ma_type: MaType,
        kind: OscillatorKind,
    ) -> Self {
        let (fast_period, slow_period) = normalize_periods(fast_period, slow_period);
        Self {
            kind,
            fast_state: MovingAverageState::new(ma_type, fast_period).unwrap_or_else(|_| {
                panic!("{builtin} does not support ma_type {}", ma_type.as_str())
            }),
            slow_state: MovingAverageState::new(ma_type, slow_period).unwrap_or_else(|_| {
                panic!("{builtin} does not support ma_type {}", ma_type.as_str())
            }),
            last_seen_version: 0,
            cached_output: Value::NA,
        }
    }

    pub(crate) fn update(
        &mut self,
        price_buffer: &SeriesBuffer,
        pc: usize,
    ) -> Result<Value, RuntimeError> {
        let version = price_buffer.version();
        if version == self.last_seen_version {
            return Ok(self.cached_output.clone());
        }
        self.last_seen_version = version;

        match price_buffer.get(0) {
            Value::F64(_) => {}
            Value::NA => {
                self.cached_output = Value::NA;
                return Ok(Value::NA);
            }
            other => {
                return Err(RuntimeError::TypeMismatch {
                    pc,
                    expected: "f64",
                    found: other.type_name(),
                });
            }
        }

        let fast = self.fast_state.update(price_buffer, pc)?;
        let slow = self.slow_state.update(price_buffer, pc)?;
        let output = oscillator_value(fast, slow, self.kind);

        self.cached_output = output.clone();
        Ok(output)
    }
}

fn normalize_periods(fast_period: usize, slow_period: usize) -> (usize, usize) {
    if slow_period < fast_period {
        (slow_period, fast_period)
    } else {
        (fast_period, slow_period)
    }
}

fn oscillator_value(fast: Value, slow: Value, kind: OscillatorKind) -> Value {
    match (fast, slow) {
        (Value::F64(fast), Value::F64(slow)) => match kind {
            OscillatorKind::Absolute => Value::F64(fast - slow),
            OscillatorKind::Percentage => {
                if slow != 0.0 {
                    Value::F64(((fast - slow) / slow) * 100.0)
                } else {
                    Value::F64(0.0)
                }
            }
        },
        (Value::NA, _) | (_, Value::NA) => Value::NA,
        _ => unreachable!(),
    }
}

#[cfg(test)]
mod tests {
    use super::{OscillatorKind, PriceOscillatorState};
    use crate::talib::MaType;
    use crate::types::Value;
    use crate::vm::SeriesBuffer;

    #[test]
    fn sma_apo_matches_fast_minus_slow_average() {
        let mut state =
            PriceOscillatorState::new("apo", 3, 5, MaType::Sma, OscillatorKind::Absolute);
        let mut buffer = SeriesBuffer::new(8);
        for value in [1.0, 2.0, 3.0, 4.0, 5.0] {
            buffer.push(Value::F64(value));
        }

        assert_eq!(state.update(&buffer, 0).unwrap(), Value::F64(1.0));
    }

    #[test]
    fn ema_ppo_returns_zero_when_slow_average_is_zero() {
        let mut state =
            PriceOscillatorState::new("ppo", 3, 5, MaType::Ema, OscillatorKind::Percentage);
        let mut buffer = SeriesBuffer::new(8);
        for value in [0.0, 0.0, 0.0, 0.0, 0.0] {
            buffer.push(Value::F64(value));
            let _ = state.update(&buffer, 0).unwrap();
        }

        assert_eq!(state.update(&buffer, 0).unwrap(), Value::F64(0.0));
    }
}
