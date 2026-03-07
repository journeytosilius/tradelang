//! MACD tuple state aligned to TA-Lib's EMA seeding rules.

use std::collections::VecDeque;

use crate::diagnostic::RuntimeError;
use crate::talib::MaType;
use crate::types::Value;
use crate::vm::SeriesBuffer;

use super::MovingAverageState;

#[derive(Clone, Debug)]
enum MacdPhase {
    CollectPrices,
    CollectSignal { seed_sum: f64, seed_count: usize },
    Running,
}

#[derive(Clone, Debug)]
pub(crate) struct MacdState {
    fast_period: usize,
    slow_period: usize,
    signal_period: usize,
    fast_alpha: f64,
    slow_alpha: f64,
    signal_alpha: f64,
    initial_prices: VecDeque<f64>,
    phase: MacdPhase,
    fast_ema: f64,
    slow_ema: f64,
    signal_ema: f64,
    last_seen_version: u64,
    cached_output: Value,
}

#[derive(Clone, Debug)]
pub(crate) struct MacdExtState {
    fast: MovingAverageState,
    slow: MovingAverageState,
    signal: MovingAverageState,
    macd_buffer: SeriesBuffer,
    last_seen_version: u64,
    cached_output: Value,
}

impl MacdExtState {
    pub(crate) fn new(
        fast_period: usize,
        fast_ma_type: MaType,
        slow_period: usize,
        slow_ma_type: MaType,
        signal_period: usize,
        signal_ma_type: MaType,
    ) -> Result<Self, RuntimeError> {
        Ok(Self {
            fast: MovingAverageState::new(fast_ma_type, fast_period)?,
            slow: MovingAverageState::new(slow_ma_type, slow_period)?,
            signal: MovingAverageState::new(signal_ma_type, signal_period)?,
            macd_buffer: SeriesBuffer::new(
                MovingAverageState::input_history(signal_period, signal_ma_type).max(2),
            ),
            last_seen_version: 0,
            cached_output: na_tuple(),
        })
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

        let fast = self.fast.update(price_buffer, pc)?;
        let slow = self.slow.update(price_buffer, pc)?;
        let line = match (fast, slow) {
            (Value::F64(fast), Value::F64(slow)) => Value::F64(fast - slow),
            (Value::NA, _) | (_, Value::NA) => Value::NA,
            (other, _) => {
                return Err(RuntimeError::TypeMismatch {
                    pc,
                    expected: "f64",
                    found: other.type_name(),
                })
            }
        };
        self.macd_buffer.push(line.clone());
        let signal = self.signal.update(&self.macd_buffer, pc)?;
        self.cached_output = match (&line, &signal) {
            (Value::F64(line), Value::F64(signal)) => tuple(*line, *signal, line - signal),
            (Value::NA, _) | (_, Value::NA) => na_tuple(),
            _ => {
                return Err(RuntimeError::TypeMismatch {
                    pc,
                    expected: "f64",
                    found: line.type_name(),
                })
            }
        };
        Ok(self.cached_output.clone())
    }
}

impl MacdState {
    pub(crate) fn new(fast_period: usize, slow_period: usize, signal_period: usize) -> Self {
        let (fast_period, slow_period) = if slow_period < fast_period {
            (slow_period, fast_period)
        } else {
            (fast_period, slow_period)
        };
        Self {
            fast_period,
            slow_period,
            signal_period,
            fast_alpha: 2.0 / (fast_period as f64 + 1.0),
            slow_alpha: 2.0 / (slow_period as f64 + 1.0),
            signal_alpha: 2.0 / (signal_period as f64 + 1.0),
            initial_prices: VecDeque::with_capacity(slow_period),
            phase: MacdPhase::CollectPrices,
            fast_ema: 0.0,
            slow_ema: 0.0,
            signal_ema: 0.0,
            last_seen_version: 0,
            cached_output: na_tuple(),
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

        let current_price = match price_buffer.get(0) {
            Value::F64(value) => value,
            Value::NA => {
                self.cached_output = na_tuple();
                return Ok(self.cached_output.clone());
            }
            other => {
                return Err(RuntimeError::TypeMismatch {
                    pc,
                    expected: "f64",
                    found: other.type_name(),
                });
            }
        };

        match &mut self.phase {
            MacdPhase::CollectPrices => {
                if self.initial_prices.len() == self.slow_period {
                    self.initial_prices.pop_front();
                }
                self.initial_prices.push_back(current_price);
                if self.initial_prices.len() < self.slow_period {
                    self.cached_output = na_tuple();
                    return Ok(self.cached_output.clone());
                }

                self.slow_ema =
                    self.initial_prices.iter().copied().sum::<f64>() / self.slow_period as f64;
                self.fast_ema = self
                    .initial_prices
                    .iter()
                    .skip(self.slow_period - self.fast_period)
                    .copied()
                    .sum::<f64>()
                    / self.fast_period as f64;
                let macd_line = self.fast_ema - self.slow_ema;
                if self.signal_period == 1 {
                    self.signal_ema = macd_line;
                    self.phase = MacdPhase::Running;
                    self.cached_output = tuple(macd_line, self.signal_ema, 0.0);
                } else {
                    self.phase = MacdPhase::CollectSignal {
                        seed_sum: macd_line,
                        seed_count: 1,
                    };
                    self.cached_output = na_tuple();
                }
            }
            MacdPhase::CollectSignal {
                seed_sum,
                seed_count,
            } => {
                self.fast_ema = ((current_price - self.fast_ema) * self.fast_alpha) + self.fast_ema;
                self.slow_ema = ((current_price - self.slow_ema) * self.slow_alpha) + self.slow_ema;
                let macd_line = self.fast_ema - self.slow_ema;
                *seed_sum += macd_line;
                *seed_count += 1;
                if *seed_count < self.signal_period {
                    self.cached_output = na_tuple();
                } else {
                    self.signal_ema = *seed_sum / self.signal_period as f64;
                    self.phase = MacdPhase::Running;
                    self.cached_output =
                        tuple(macd_line, self.signal_ema, macd_line - self.signal_ema);
                }
            }
            MacdPhase::Running => {
                self.fast_ema = ((current_price - self.fast_ema) * self.fast_alpha) + self.fast_ema;
                self.slow_ema = ((current_price - self.slow_ema) * self.slow_alpha) + self.slow_ema;
                let macd_line = self.fast_ema - self.slow_ema;
                self.signal_ema =
                    ((macd_line - self.signal_ema) * self.signal_alpha) + self.signal_ema;
                self.cached_output = tuple(macd_line, self.signal_ema, macd_line - self.signal_ema);
            }
        }

        Ok(self.cached_output.clone())
    }
}

fn tuple(line: f64, signal: f64, hist: f64) -> Value {
    Value::Tuple3([
        Box::new(Value::F64(line)),
        Box::new(Value::F64(signal)),
        Box::new(Value::F64(hist)),
    ])
}

fn na_tuple() -> Value {
    Value::Tuple3([
        Box::new(Value::NA),
        Box::new(Value::NA),
        Box::new(Value::NA),
    ])
}

#[cfg(test)]
mod tests {
    use super::{MacdExtState, MacdState};
    use crate::talib::MaType;
    use crate::types::Value;
    use crate::vm::SeriesBuffer;

    #[test]
    fn produces_na_until_signal_seed_is_ready() {
        let mut state = MacdState::new(3, 5, 2);
        let mut buffer = SeriesBuffer::new(16);
        for price in [1.0, 2.0, 3.0, 4.0, 5.0] {
            buffer.push(Value::F64(price));
        }
        assert_eq!(
            state.update(&buffer, 0).unwrap(),
            Value::Tuple3([
                Box::new(Value::NA),
                Box::new(Value::NA),
                Box::new(Value::NA)
            ])
        );

        buffer.push(Value::F64(6.0));
        let value = state.update(&buffer, 0).unwrap();
        assert_eq!(value.tuple_len(), Some(3));
    }

    #[test]
    fn macdext_returns_tuple_once_all_lines_are_ready() {
        let mut state = MacdExtState::new(3, MaType::Sma, 5, MaType::Sma, 2, MaType::Sma).unwrap();
        let mut buffer = SeriesBuffer::new(16);
        for price in [1.0, 2.0, 3.0, 4.0, 5.0, 6.0] {
            buffer.push(Value::F64(price));
            let _ = state.update(&buffer, 0).unwrap();
        }
        assert_eq!(state.update(&buffer, 0).unwrap().tuple_len(), Some(3));
    }
}
