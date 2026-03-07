//! Chande Momentum Oscillator state and update logic.

use crate::types::Value;
use crate::vm::SeriesBuffer;

#[derive(Clone, Debug)]
pub(crate) struct CmoState {
    seeded: bool,
    avg_gain: f64,
    avg_loss: f64,
    last_price: Option<f64>,
    seed_gain_sum: f64,
    seed_loss_sum: f64,
    seed_count: usize,
    len: usize,
    last_seen_version: u64,
    cached_output: Value,
}

impl CmoState {
    pub(crate) fn new(len: usize) -> Self {
        Self {
            seeded: false,
            avg_gain: 0.0,
            avg_loss: 0.0,
            last_price: None,
            seed_gain_sum: 0.0,
            seed_loss_sum: 0.0,
            seed_count: 0,
            len,
            last_seen_version: 0,
            cached_output: Value::NA,
        }
    }

    pub(crate) const fn requires_seed_step(&self) -> bool {
        !self.seeded && self.last_price.is_some()
    }

    pub(crate) const fn last_seen_version(&self) -> u64 {
        self.last_seen_version
    }

    pub(crate) fn update(&mut self, buffer: &SeriesBuffer) -> Value {
        let version = buffer.version();
        if version == self.last_seen_version {
            return self.cached_output.clone();
        }
        self.last_seen_version = version;

        let current_price = match buffer.get(0) {
            Value::F64(value) => value,
            Value::NA => {
                self.cached_output = Value::NA;
                return Value::NA;
            }
            _ => {
                self.cached_output = Value::NA;
                return Value::NA;
            }
        };

        let Some(prev_price) = self.last_price else {
            self.last_price = Some(current_price);
            self.cached_output = Value::NA;
            return Value::NA;
        };

        let delta = current_price - prev_price;
        let gain = delta.max(0.0);
        let loss = (-delta).max(0.0);
        self.last_price = Some(current_price);

        if !self.seeded {
            self.seed_gain_sum += gain;
            self.seed_loss_sum += loss;
            self.seed_count += 1;

            if self.seed_count < self.len {
                self.cached_output = Value::NA;
                return Value::NA;
            }

            self.avg_gain = self.seed_gain_sum / self.len as f64;
            self.avg_loss = self.seed_loss_sum / self.len as f64;
            self.seeded = true;
        } else {
            self.avg_gain = ((self.avg_gain * (self.len as f64 - 1.0)) + gain) / self.len as f64;
            self.avg_loss = ((self.avg_loss * (self.len as f64 - 1.0)) + loss) / self.len as f64;
        }

        let sum = self.avg_gain + self.avg_loss;
        self.cached_output = if sum != 0.0 {
            Value::F64(100.0 * ((self.avg_gain - self.avg_loss) / sum))
        } else {
            Value::F64(0.0)
        };
        self.cached_output.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::CmoState;
    use crate::types::Value;
    use crate::vm::SeriesBuffer;

    fn assert_f64_eq(value: Value, expected: f64) {
        match value {
            Value::F64(value) => assert!((value - expected).abs() < 1e-9),
            other => panic!("expected f64, got {other:?}"),
        }
    }

    #[test]
    fn seeds_after_enough_deltas_and_returns_hundred_for_rising_prices() {
        let mut state = CmoState::new(3);
        let mut buffer = SeriesBuffer::new(8);

        buffer.push(Value::F64(1.0));
        assert_eq!(state.update(&buffer), Value::NA);
        assert!(state.requires_seed_step());

        buffer.push(Value::F64(2.0));
        assert_eq!(state.update(&buffer), Value::NA);
        buffer.push(Value::F64(3.0));
        assert_eq!(state.update(&buffer), Value::NA);

        buffer.push(Value::F64(4.0));
        assert_f64_eq(state.update(&buffer), 100.0);
    }

    #[test]
    fn smooths_gain_and_loss_after_seed() {
        let mut state = CmoState::new(3);
        let mut buffer = SeriesBuffer::new(8);

        buffer.push(Value::F64(1.0));
        assert_eq!(state.update(&buffer), Value::NA);
        buffer.push(Value::F64(2.0));
        assert_eq!(state.update(&buffer), Value::NA);
        buffer.push(Value::F64(3.0));
        assert_eq!(state.update(&buffer), Value::NA);
        buffer.push(Value::F64(4.0));
        assert_f64_eq(state.update(&buffer), 100.0);

        buffer.push(Value::F64(3.0));
        assert_f64_eq(state.update(&buffer), 33.33333333333333);
    }
}
