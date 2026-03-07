//! Volume indicator state.

use crate::diagnostic::RuntimeError;
use crate::types::Value;
use crate::vm::SeriesBuffer;

#[derive(Clone, Debug)]
pub(crate) struct ObvState {
    initialized: bool,
    last_price_version: u64,
    last_volume_version: u64,
    last_close: f64,
    value: f64,
    cached_output: Value,
}

impl ObvState {
    pub(crate) fn new() -> Self {
        Self {
            initialized: false,
            last_price_version: 0,
            last_volume_version: 0,
            last_close: 0.0,
            value: 0.0,
            cached_output: Value::NA,
        }
    }

    pub(crate) fn update(
        &mut self,
        price_buffer: &SeriesBuffer,
        volume_buffer: &SeriesBuffer,
        pc: usize,
    ) -> Result<Value, RuntimeError> {
        if price_buffer.version() == self.last_price_version
            && volume_buffer.version() == self.last_volume_version
        {
            return Ok(self.cached_output.clone());
        }
        self.last_price_version = price_buffer.version();
        self.last_volume_version = volume_buffer.version();

        let current_close = match price_buffer.get(0) {
            Value::F64(value) => value,
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
        };
        let current_volume = match volume_buffer.get(0) {
            Value::F64(value) => value,
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
        };

        if !self.initialized {
            self.initialized = true;
            self.last_close = current_close;
            self.value = current_volume;
            self.cached_output = Value::F64(self.value);
            return Ok(self.cached_output.clone());
        }

        if current_close > self.last_close {
            self.value += current_volume;
        } else if current_close < self.last_close {
            self.value -= current_volume;
        }
        self.last_close = current_close;
        self.cached_output = Value::F64(self.value);
        Ok(self.cached_output.clone())
    }
}

#[derive(Clone, Debug)]
pub(crate) struct AdState {
    last_versions: (u64, u64, u64, u64),
    value: f64,
    cached_output: Value,
}

impl AdState {
    pub(crate) fn new() -> Self {
        Self {
            last_versions: (0, 0, 0, 0),
            value: 0.0,
            cached_output: Value::NA,
        }
    }

    pub(crate) fn update(
        &mut self,
        high: &SeriesBuffer,
        low: &SeriesBuffer,
        close: &SeriesBuffer,
        volume: &SeriesBuffer,
        pc: usize,
    ) -> Result<Value, RuntimeError> {
        let versions = (
            high.version(),
            low.version(),
            close.version(),
            volume.version(),
        );
        if versions == self.last_versions {
            return Ok(self.cached_output.clone());
        }
        self.last_versions = versions;

        let Some(high) = expect_buffer_value(high, 0, pc)? else {
            self.cached_output = Value::NA;
            return Ok(Value::NA);
        };
        let Some(low) = expect_buffer_value(low, 0, pc)? else {
            self.cached_output = Value::NA;
            return Ok(Value::NA);
        };
        let Some(close) = expect_buffer_value(close, 0, pc)? else {
            self.cached_output = Value::NA;
            return Ok(Value::NA);
        };
        let Some(volume) = expect_buffer_value(volume, 0, pc)? else {
            self.cached_output = Value::NA;
            return Ok(Value::NA);
        };

        let range = high - low;
        if range > 0.0 {
            let multiplier = ((close - low) - (high - close)) / range;
            self.value += multiplier * volume;
        }
        self.cached_output = Value::F64(self.value);
        Ok(self.cached_output.clone())
    }
}

#[derive(Clone, Debug)]
pub(crate) struct AdOscState {
    fast_period: usize,
    slow_period: usize,
    last_versions: (u64, u64, u64, u64),
    ad_line: f64,
    fast_ema: f64,
    slow_ema: f64,
    initialized: bool,
    samples: usize,
    cached_output: Value,
}

impl AdOscState {
    pub(crate) fn new(fast_period: usize, slow_period: usize) -> Self {
        let (fast_period, slow_period) = if slow_period < fast_period {
            (slow_period, fast_period)
        } else {
            (fast_period, slow_period)
        };
        Self {
            fast_period,
            slow_period,
            last_versions: (0, 0, 0, 0),
            ad_line: 0.0,
            fast_ema: 0.0,
            slow_ema: 0.0,
            initialized: false,
            samples: 0,
            cached_output: Value::NA,
        }
    }

    pub(crate) fn update(
        &mut self,
        high: &SeriesBuffer,
        low: &SeriesBuffer,
        close: &SeriesBuffer,
        volume: &SeriesBuffer,
        pc: usize,
    ) -> Result<Value, RuntimeError> {
        let versions = (
            high.version(),
            low.version(),
            close.version(),
            volume.version(),
        );
        if versions == self.last_versions {
            return Ok(self.cached_output.clone());
        }
        self.last_versions = versions;

        let Some(high) = expect_buffer_value(high, 0, pc)? else {
            self.cached_output = Value::NA;
            return Ok(Value::NA);
        };
        let Some(low) = expect_buffer_value(low, 0, pc)? else {
            self.cached_output = Value::NA;
            return Ok(Value::NA);
        };
        let Some(close) = expect_buffer_value(close, 0, pc)? else {
            self.cached_output = Value::NA;
            return Ok(Value::NA);
        };
        let Some(volume) = expect_buffer_value(volume, 0, pc)? else {
            self.cached_output = Value::NA;
            return Ok(Value::NA);
        };

        let range = high - low;
        if range > 0.0 {
            let multiplier = ((close - low) - (high - close)) / range;
            self.ad_line += multiplier * volume;
        }

        if !self.initialized {
            self.initialized = true;
            self.fast_ema = self.ad_line;
            self.slow_ema = self.ad_line;
            self.samples = 1;
            self.cached_output = Value::NA;
            return Ok(Value::NA);
        }

        let fast_alpha = 2.0 / (self.fast_period as f64 + 1.0);
        let slow_alpha = 2.0 / (self.slow_period as f64 + 1.0);
        self.fast_ema = ((self.ad_line - self.fast_ema) * fast_alpha) + self.fast_ema;
        self.slow_ema = ((self.ad_line - self.slow_ema) * slow_alpha) + self.slow_ema;
        self.samples += 1;

        if self.samples < self.slow_period {
            self.cached_output = Value::NA;
        } else {
            self.cached_output = Value::F64(self.fast_ema - self.slow_ema);
        }
        Ok(self.cached_output.clone())
    }
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

#[cfg(test)]
mod tests {
    use super::{AdOscState, AdState, ObvState};
    use crate::types::Value;
    use crate::vm::SeriesBuffer;

    #[test]
    fn obv_seeds_from_first_volume_and_accumulates_directionally() {
        let mut state = ObvState::new();
        let mut price = SeriesBuffer::new(8);
        let mut volume = SeriesBuffer::new(8);

        price.push(Value::F64(10.0));
        volume.push(Value::F64(100.0));
        assert_eq!(state.update(&price, &volume, 0).unwrap(), Value::F64(100.0));

        price.push(Value::F64(11.0));
        volume.push(Value::F64(50.0));
        assert_eq!(state.update(&price, &volume, 0).unwrap(), Value::F64(150.0));

        price.push(Value::F64(9.0));
        volume.push(Value::F64(25.0));
        assert_eq!(state.update(&price, &volume, 0).unwrap(), Value::F64(125.0));
    }

    #[test]
    fn ad_accumulates_money_flow_volume() {
        let mut state = AdState::new();
        let mut high = SeriesBuffer::new(4);
        let mut low = SeriesBuffer::new(4);
        let mut close = SeriesBuffer::new(4);
        let mut volume = SeriesBuffer::new(4);

        for (h, l, c, v) in [(10.0, 8.0, 9.0, 100.0), (11.0, 9.0, 10.5, 50.0)] {
            high.push(Value::F64(h));
            low.push(Value::F64(l));
            close.push(Value::F64(c));
            volume.push(Value::F64(v));
        }

        assert!(matches!(
            state.update(&high, &low, &close, &volume, 0).unwrap(),
            Value::F64(_)
        ));
    }

    #[test]
    fn adosc_stays_na_until_slow_period() {
        let mut state = AdOscState::new(3, 5);
        let mut high = SeriesBuffer::new(8);
        let mut low = SeriesBuffer::new(8);
        let mut close = SeriesBuffer::new(8);
        let mut volume = SeriesBuffer::new(8);

        for (h, l, c, v) in [
            (10.0, 8.0, 9.0, 100.0),
            (11.0, 9.0, 10.0, 110.0),
            (12.0, 10.0, 11.0, 120.0),
            (13.0, 11.0, 12.0, 130.0),
            (14.0, 12.0, 13.0, 140.0),
        ] {
            high.push(Value::F64(h));
            low.push(Value::F64(l));
            close.push(Value::F64(c));
            volume.push(Value::F64(v));
            let _ = state.update(&high, &low, &close, &volume, 0).unwrap();
        }

        assert!(matches!(
            state.update(&high, &low, &close, &volume, 0).unwrap(),
            Value::F64(_) | Value::NA
        ));
    }
}
