//! Relative strength index indicator state and update logic.
//!
//! RSI tracks bounded rolling gain and loss averages without heap growth in the
//! VM hot path.

use crate::types::Value;

#[derive(Clone, Debug)]
pub(crate) struct RsiState {
    seeded: bool,
    avg_gain: f64,
    avg_loss: f64,
    last_price: Option<f64>,
    seed_gain_sum: f64,
    seed_loss_sum: f64,
    seed_count: usize,
    len: usize,
}

impl RsiState {
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
        }
    }

    pub(crate) const fn requires_seed_step(&self) -> bool {
        !self.seeded && self.last_price.is_some()
    }

    pub(crate) fn update(&mut self, current_price: f64) -> Value {
        let Some(prev_price) = self.last_price else {
            self.last_price = Some(current_price);
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
                return Value::NA;
            }

            self.avg_gain = self.seed_gain_sum / self.len as f64;
            self.avg_loss = self.seed_loss_sum / self.len as f64;
            self.seeded = true;
        } else {
            self.avg_gain = ((self.avg_gain * (self.len as f64 - 1.0)) + gain) / self.len as f64;
            self.avg_loss = ((self.avg_loss * (self.len as f64 - 1.0)) + loss) / self.len as f64;
        }

        if self.avg_loss == 0.0 {
            Value::F64(100.0)
        } else {
            let rs = self.avg_gain / self.avg_loss;
            Value::F64(100.0 - (100.0 / (1.0 + rs)))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::RsiState;
    use crate::types::Value;

    fn assert_f64_eq(value: Value, expected: f64) {
        match value {
            Value::F64(value) => assert!((value - expected).abs() < 1e-9),
            other => panic!("expected f64, got {other:?}"),
        }
    }

    #[test]
    fn seeds_after_enough_deltas_and_returns_hundred_for_rising_prices() {
        let mut state = RsiState::new(3);

        assert!(!state.requires_seed_step());
        assert_eq!(state.update(1.0), Value::NA);
        assert!(state.requires_seed_step());

        assert_eq!(state.update(2.0), Value::NA);
        assert!(state.requires_seed_step());

        assert_eq!(state.update(3.0), Value::NA);
        assert!(state.requires_seed_step());

        let seeded = state.update(4.0);
        assert_f64_eq(seeded, 100.0);
    }

    #[test]
    fn smooths_gain_and_loss_after_seed() {
        let mut state = RsiState::new(3);

        assert_eq!(state.update(1.0), Value::NA);
        assert_eq!(state.update(2.0), Value::NA);
        assert_eq!(state.update(3.0), Value::NA);
        assert_f64_eq(state.update(4.0), 100.0);

        let next = state.update(3.0);
        assert_f64_eq(next, 66.66666666666666);
    }
}
