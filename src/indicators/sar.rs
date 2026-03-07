//! Parabolic SAR state aligned with TA-Lib's bootstrap rules.

use crate::diagnostic::RuntimeError;
use crate::types::Value;
use crate::vm::SeriesBuffer;

#[derive(Clone, Debug)]
pub(crate) struct SarState {
    config: SarConfig,
    last_versions: (u64, u64),
    initialized: bool,
    is_long: bool,
    prev_high: f64,
    prev_low: f64,
    ep: f64,
    sar: f64,
    af_long: f64,
    af_short: f64,
    cached_output: Value,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct SarConfig {
    pub(crate) start_value: f64,
    pub(crate) offset_on_reverse: f64,
    pub(crate) acceleration_init_long: f64,
    pub(crate) acceleration_long: f64,
    pub(crate) acceleration_max_long: f64,
    pub(crate) acceleration_init_short: f64,
    pub(crate) acceleration_short: f64,
    pub(crate) acceleration_max_short: f64,
    pub(crate) signed_short: bool,
}

impl SarConfig {
    pub(crate) const fn standard(acceleration: f64, maximum: f64) -> Self {
        Self {
            start_value: 0.0,
            offset_on_reverse: 0.0,
            acceleration_init_long: acceleration,
            acceleration_long: acceleration,
            acceleration_max_long: maximum,
            acceleration_init_short: acceleration,
            acceleration_short: acceleration,
            acceleration_max_short: maximum,
            signed_short: false,
        }
    }
}

impl SarState {
    pub(crate) fn new(config: SarConfig) -> Self {
        let mut config = config;
        if config.acceleration_init_long > config.acceleration_max_long {
            config.acceleration_init_long = config.acceleration_max_long;
        }
        if config.acceleration_long > config.acceleration_max_long {
            config.acceleration_long = config.acceleration_max_long;
        }
        if config.acceleration_init_short > config.acceleration_max_short {
            config.acceleration_init_short = config.acceleration_max_short;
        }
        if config.acceleration_short > config.acceleration_max_short {
            config.acceleration_short = config.acceleration_max_short;
        }
        Self {
            config,
            last_versions: (0, 0),
            initialized: false,
            is_long: true,
            prev_high: 0.0,
            prev_low: 0.0,
            ep: 0.0,
            sar: 0.0,
            af_long: config.acceleration_init_long,
            af_short: config.acceleration_init_short,
            cached_output: Value::NA,
        }
    }

    pub(crate) fn update(
        &mut self,
        high: &SeriesBuffer,
        low: &SeriesBuffer,
        pc: usize,
    ) -> Result<Value, RuntimeError> {
        let versions = (high.version(), low.version());
        if versions == self.last_versions {
            return Ok(self.cached_output.clone());
        }
        self.last_versions = versions;

        if high.len() < 2 || low.len() < 2 {
            self.cached_output = Value::NA;
            return Ok(Value::NA);
        }

        let current_high = expect_f64(high.get(0), pc)?;
        let current_low = expect_f64(low.get(0), pc)?;

        if !self.initialized {
            let previous_high = expect_f64(high.get(1), pc)?;
            let previous_low = expect_f64(low.get(1), pc)?;
            self.initialize(previous_high, previous_low, current_high, current_low);
            let output = self.advance(current_high, current_low);
            self.prev_high = current_high;
            self.prev_low = current_low;
            self.initialized = true;
            self.cached_output = Value::F64(output);
            return Ok(self.cached_output.clone());
        }

        let output = self.advance(current_high, current_low);
        self.prev_high = current_high;
        self.prev_low = current_low;
        self.cached_output = Value::F64(output);
        Ok(self.cached_output.clone())
    }

    fn initialize(
        &mut self,
        previous_high: f64,
        previous_low: f64,
        current_high: f64,
        current_low: f64,
    ) {
        if self.config.start_value == 0.0 {
            let diff_plus = current_high - previous_high;
            let diff_minus = previous_low - current_low;
            self.is_long = !(diff_minus > 0.0 && diff_minus > diff_plus);
            if self.is_long {
                self.ep = current_high;
                self.sar = previous_low;
            } else {
                self.ep = current_low;
                self.sar = previous_high;
            }
        } else if self.config.start_value > 0.0 {
            self.is_long = true;
            self.ep = current_high;
            self.sar = self.config.start_value;
        } else {
            self.is_long = false;
            self.ep = current_low;
            self.sar = self.config.start_value.abs();
        }
        self.prev_high = current_high;
        self.prev_low = current_low;
        self.af_long = self.config.acceleration_init_long;
        self.af_short = self.config.acceleration_init_short;
    }

    fn advance(&mut self, current_high: f64, current_low: f64) -> f64 {
        if self.is_long {
            if current_low <= self.sar {
                self.is_long = false;
                let mut sar = self.ep.max(self.prev_high).max(current_high);
                if self.config.offset_on_reverse != 0.0 {
                    sar += sar * self.config.offset_on_reverse;
                }
                self.af_short = self.config.acceleration_init_short;
                self.ep = current_low;
                self.sar = sar + self.af_short * (self.ep - sar);
                self.sar = self.sar.max(self.prev_high).max(current_high);
                return if self.config.signed_short { -sar } else { sar };
            }

            let output = self.sar;
            if current_high > self.ep {
                self.ep = current_high;
                self.af_long = (self.af_long + self.config.acceleration_long)
                    .min(self.config.acceleration_max_long);
            }
            self.sar = self.sar + self.af_long * (self.ep - self.sar);
            self.sar = self.sar.min(self.prev_low).min(current_low);
            output
        } else {
            if current_high >= self.sar {
                self.is_long = true;
                let mut sar = self.ep.min(self.prev_low).min(current_low);
                if self.config.offset_on_reverse != 0.0 {
                    sar -= sar * self.config.offset_on_reverse;
                }
                self.af_long = self.config.acceleration_init_long;
                self.ep = current_high;
                self.sar = sar + self.af_long * (self.ep - sar);
                self.sar = self.sar.min(self.prev_low).min(current_low);
                return sar;
            }

            let output = if self.config.signed_short {
                -self.sar
            } else {
                self.sar
            };
            if current_low < self.ep {
                self.ep = current_low;
                self.af_short = (self.af_short + self.config.acceleration_short)
                    .min(self.config.acceleration_max_short);
            }
            self.sar = self.sar + self.af_short * (self.ep - self.sar);
            self.sar = self.sar.max(self.prev_high).max(current_high);
            output
        }
    }
}

fn expect_f64(value: Value, pc: usize) -> Result<f64, RuntimeError> {
    match value {
        Value::F64(value) => Ok(value),
        Value::NA => Err(RuntimeError::TypeMismatch {
            pc,
            expected: "f64",
            found: "na",
        }),
        other => Err(RuntimeError::TypeMismatch {
            pc,
            expected: "f64",
            found: other.type_name(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::{SarConfig, SarState};
    use crate::types::Value;
    use crate::vm::SeriesBuffer;

    #[test]
    fn standard_sar_produces_value_after_second_bar() {
        let mut state = SarState::new(SarConfig::standard(0.02, 0.2));
        let mut high = SeriesBuffer::new(8);
        let mut low = SeriesBuffer::new(8);
        for (h, l) in [(11.0, 9.0), (12.0, 10.0)] {
            high.push(Value::F64(h));
            low.push(Value::F64(l));
        }
        assert!(matches!(
            state.update(&high, &low, 0).unwrap(),
            Value::F64(_)
        ));
    }

    #[test]
    fn sarext_outputs_negative_values_while_short() {
        let mut state = SarState::new(SarConfig {
            start_value: -12.0,
            offset_on_reverse: 0.0,
            acceleration_init_long: 0.02,
            acceleration_long: 0.02,
            acceleration_max_long: 0.2,
            acceleration_init_short: 0.02,
            acceleration_short: 0.02,
            acceleration_max_short: 0.2,
            signed_short: true,
        });
        let mut high = SeriesBuffer::new(8);
        let mut low = SeriesBuffer::new(8);
        for (h, l) in [(11.0, 9.0), (10.5, 8.5)] {
            high.push(Value::F64(h));
            low.push(Value::F64(l));
        }
        let value = state.update(&high, &low, 0).unwrap();
        match value {
            Value::F64(value) => assert!(value < 0.0),
            other => panic!("expected f64, got {other:?}"),
        }
    }
}
