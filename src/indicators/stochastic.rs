//! Stochastic oscillator state for STOCH, STOCHF, and STOCHRSI.

use crate::diagnostic::RuntimeError;
use crate::talib::MaType;
use crate::types::Value;
use crate::vm::SeriesBuffer;

use super::{MovingAverageState, RsiState};

#[derive(Clone, Debug)]
pub(crate) struct StochFastState {
    fast_k_period: usize,
    fast_d: MovingAverageState,
    fast_k_buffer: SeriesBuffer,
    last_versions: (u64, u64, u64),
    cached_output: Value,
}

impl StochFastState {
    pub(crate) fn new(
        fast_k_period: usize,
        fast_d_period: usize,
        fast_d_ma_type: MaType,
    ) -> Result<Self, RuntimeError> {
        Ok(Self {
            fast_k_period,
            fast_d: MovingAverageState::new(fast_d_ma_type, fast_d_period)?,
            fast_k_buffer: SeriesBuffer::new(
                MovingAverageState::input_history(fast_d_period, fast_d_ma_type).max(2),
            ),
            last_versions: (0, 0, 0),
            cached_output: na_tuple2(),
        })
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

        let fast_k = calculate_fast_k(high, low, close, self.fast_k_period, pc)?;
        self.fast_k_buffer.push(fast_k.clone());
        let fast_d = self.fast_d.update(&self.fast_k_buffer, pc)?;
        self.cached_output = match (&fast_k, &fast_d) {
            (Value::F64(k), Value::F64(d)) => {
                Value::Tuple2([Box::new(Value::F64(*k)), Box::new(Value::F64(*d))])
            }
            (Value::NA, _) | (_, Value::NA) => na_tuple2(),
            _ => {
                return Err(RuntimeError::TypeMismatch {
                    pc,
                    expected: "f64",
                    found: fast_k.type_name(),
                })
            }
        };
        Ok(self.cached_output.clone())
    }
}

#[derive(Clone, Debug)]
pub(crate) struct StochState {
    fast_k_period: usize,
    slow_k: MovingAverageState,
    slow_d: MovingAverageState,
    fast_k_buffer: SeriesBuffer,
    slow_k_buffer: SeriesBuffer,
    last_versions: (u64, u64, u64),
    cached_output: Value,
}

impl StochState {
    pub(crate) fn new(
        fast_k_period: usize,
        slow_k_period: usize,
        slow_k_ma_type: MaType,
        slow_d_period: usize,
        slow_d_ma_type: MaType,
    ) -> Result<Self, RuntimeError> {
        Ok(Self {
            fast_k_period,
            slow_k: MovingAverageState::new(slow_k_ma_type, slow_k_period)?,
            slow_d: MovingAverageState::new(slow_d_ma_type, slow_d_period)?,
            fast_k_buffer: SeriesBuffer::new(
                MovingAverageState::input_history(slow_k_period, slow_k_ma_type).max(2),
            ),
            slow_k_buffer: SeriesBuffer::new(
                MovingAverageState::input_history(slow_d_period, slow_d_ma_type).max(2),
            ),
            last_versions: (0, 0, 0),
            cached_output: na_tuple2(),
        })
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

        let fast_k = calculate_fast_k(high, low, close, self.fast_k_period, pc)?;
        self.fast_k_buffer.push(fast_k);
        let slow_k = self.slow_k.update(&self.fast_k_buffer, pc)?;
        self.slow_k_buffer.push(slow_k.clone());
        let slow_d = self.slow_d.update(&self.slow_k_buffer, pc)?;
        self.cached_output = match (&slow_k, &slow_d) {
            (Value::F64(k), Value::F64(d)) => {
                Value::Tuple2([Box::new(Value::F64(*k)), Box::new(Value::F64(*d))])
            }
            (Value::NA, _) | (_, Value::NA) => na_tuple2(),
            _ => {
                return Err(RuntimeError::TypeMismatch {
                    pc,
                    expected: "f64",
                    found: slow_k.type_name(),
                })
            }
        };
        Ok(self.cached_output.clone())
    }
}

#[derive(Clone, Debug)]
pub(crate) struct StochRsiState {
    rsi: RsiState,
    fast_k_period: usize,
    fast_d: MovingAverageState,
    rsi_buffer: SeriesBuffer,
    fast_k_buffer: SeriesBuffer,
    last_version: u64,
    cached_output: Value,
}

impl StochRsiState {
    pub(crate) fn new(
        time_period: usize,
        fast_k_period: usize,
        fast_d_period: usize,
        fast_d_ma_type: MaType,
    ) -> Result<Self, RuntimeError> {
        Ok(Self {
            rsi: RsiState::new(time_period),
            fast_k_period,
            fast_d: MovingAverageState::new(fast_d_ma_type, fast_d_period)?,
            rsi_buffer: SeriesBuffer::new(fast_k_period.max(2)),
            fast_k_buffer: SeriesBuffer::new(
                MovingAverageState::input_history(fast_d_period, fast_d_ma_type).max(2),
            ),
            last_version: 0,
            cached_output: na_tuple2(),
        })
    }

    pub(crate) fn update(
        &mut self,
        series: &SeriesBuffer,
        pc: usize,
    ) -> Result<Value, RuntimeError> {
        let version = series.version();
        if version == self.last_version {
            return Ok(self.cached_output.clone());
        }
        self.last_version = version;

        let rsi = self.rsi.update(series);
        self.rsi_buffer.push(rsi);
        let fast_k = calculate_single_series_k(&self.rsi_buffer, self.fast_k_period, pc)?;
        self.fast_k_buffer.push(fast_k.clone());
        let fast_d = self.fast_d.update(&self.fast_k_buffer, pc)?;
        self.cached_output = match (&fast_k, &fast_d) {
            (Value::F64(k), Value::F64(d)) => {
                Value::Tuple2([Box::new(Value::F64(*k)), Box::new(Value::F64(*d))])
            }
            (Value::NA, _) | (_, Value::NA) => na_tuple2(),
            _ => {
                return Err(RuntimeError::TypeMismatch {
                    pc,
                    expected: "f64",
                    found: fast_k.type_name(),
                })
            }
        };
        Ok(self.cached_output.clone())
    }
}

fn calculate_fast_k(
    high: &SeriesBuffer,
    low: &SeriesBuffer,
    close: &SeriesBuffer,
    window: usize,
    pc: usize,
) -> Result<Value, RuntimeError> {
    if high.len() < window || low.len() < window || close.is_empty() {
        return Ok(Value::NA);
    }
    let Some(current_close) = expect_numeric(close.get(0), pc)? else {
        return Ok(Value::NA);
    };
    let mut highest = f64::NEG_INFINITY;
    let mut lowest = f64::INFINITY;
    for offset in 0..window {
        let Some(high_value) = expect_numeric(high.get(offset), pc)? else {
            return Ok(Value::NA);
        };
        let Some(low_value) = expect_numeric(low.get(offset), pc)? else {
            return Ok(Value::NA);
        };
        highest = highest.max(high_value);
        lowest = lowest.min(low_value);
    }
    let range = highest - lowest;
    if range == 0.0 {
        Ok(Value::F64(0.0))
    } else {
        Ok(Value::F64(((current_close - lowest) / range) * 100.0))
    }
}

fn calculate_single_series_k(
    series: &SeriesBuffer,
    window: usize,
    pc: usize,
) -> Result<Value, RuntimeError> {
    if series.len() < window {
        return Ok(Value::NA);
    }
    let Some(current) = expect_numeric(series.get(0), pc)? else {
        return Ok(Value::NA);
    };
    let mut highest = f64::NEG_INFINITY;
    let mut lowest = f64::INFINITY;
    for offset in 0..window {
        let Some(value) = expect_numeric(series.get(offset), pc)? else {
            return Ok(Value::NA);
        };
        highest = highest.max(value);
        lowest = lowest.min(value);
    }
    let range = highest - lowest;
    if range == 0.0 {
        Ok(Value::F64(0.0))
    } else {
        Ok(Value::F64(((current - lowest) / range) * 100.0))
    }
}

fn expect_numeric(value: Value, pc: usize) -> Result<Option<f64>, RuntimeError> {
    match value {
        Value::F64(value) => Ok(Some(value)),
        Value::NA => Ok(None),
        other => Err(RuntimeError::TypeMismatch {
            pc,
            expected: "f64",
            found: other.type_name(),
        }),
    }
}

fn na_tuple2() -> Value {
    Value::Tuple2([Box::new(Value::NA), Box::new(Value::NA)])
}

#[cfg(test)]
mod tests {
    use super::{StochFastState, StochRsiState, StochState};
    use crate::talib::MaType;
    use crate::types::Value;
    use crate::vm::SeriesBuffer;

    #[test]
    fn stochf_returns_tuple_after_seed() {
        let mut state = StochFastState::new(3, 3, MaType::Sma).unwrap();
        let mut high = SeriesBuffer::new(8);
        let mut low = SeriesBuffer::new(8);
        let mut close = SeriesBuffer::new(8);
        for (h, l, c) in [(3.0, 1.0, 2.0), (4.0, 2.0, 3.0), (5.0, 3.0, 4.0)] {
            high.push(Value::F64(h));
            low.push(Value::F64(l));
            close.push(Value::F64(c));
            let _ = state.update(&high, &low, &close, 0).unwrap();
        }
        assert!(matches!(
            state.update(&high, &low, &close, 0).unwrap(),
            Value::Tuple2(_)
        ));
    }

    #[test]
    fn stoch_returns_tuple_after_double_smoothing() {
        let mut state = StochState::new(3, 3, MaType::Sma, 3, MaType::Sma).unwrap();
        let mut high = SeriesBuffer::new(16);
        let mut low = SeriesBuffer::new(16);
        let mut close = SeriesBuffer::new(16);
        for value in 1..=8 {
            high.push(Value::F64(value as f64 + 1.0));
            low.push(Value::F64(value as f64 - 1.0));
            close.push(Value::F64(value as f64));
            let _ = state.update(&high, &low, &close, 0).unwrap();
        }
        assert!(matches!(
            state.update(&high, &low, &close, 0).unwrap(),
            Value::Tuple2(_)
        ));
    }

    #[test]
    fn stochrsi_returns_tuple_after_rsi_and_smoothing() {
        let mut state = StochRsiState::new(3, 3, 3, MaType::Sma).unwrap();
        let mut close = SeriesBuffer::new(16);
        for value in [1.0, 2.0, 3.0, 2.5, 3.5, 4.5, 4.0, 5.0] {
            close.push(Value::F64(value));
            let _ = state.update(&close, 0).unwrap();
        }
        assert!(matches!(state.update(&close, 0).unwrap(), Value::Tuple2(_)));
    }
}
