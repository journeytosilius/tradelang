//! Exponential moving average indicator state and update logic.
//!
//! EMA seeds from an initial SMA window and then updates incrementally with a
//! stable smoothing factor on each new bar.

use crate::diagnostic::RuntimeError;
use crate::types::Value;
use crate::vm::SeriesBuffer;

#[derive(Clone, Debug)]
pub(crate) struct EmaState {
    seeded: bool,
    alpha: f64,
    value: f64,
    seed_window: usize,
}

impl EmaState {
    pub(crate) fn new(window: usize) -> Self {
        Self {
            seeded: false,
            alpha: 2.0 / (window as f64 + 1.0),
            value: 0.0,
            seed_window: window,
        }
    }

    pub(crate) const fn is_seeded(&self) -> bool {
        self.seeded
    }

    pub(crate) const fn seed_window(&self) -> usize {
        self.seed_window
    }

    pub(crate) fn update(
        &mut self,
        current_price: f64,
        buffer: &SeriesBuffer,
        pc: usize,
    ) -> Result<Value, RuntimeError> {
        if !self.seeded {
            if buffer.len() < self.seed_window {
                return Ok(Value::NA);
            }

            let mut sum = 0.0;
            for sample in buffer.iter_recent(self.seed_window) {
                match sample {
                    Value::F64(sample) => sum += sample,
                    Value::NA => return Ok(Value::NA),
                    other => {
                        return Err(RuntimeError::TypeMismatch {
                            pc,
                            expected: "f64",
                            found: other.type_name(),
                        });
                    }
                }
            }

            self.value = sum / self.seed_window as f64;
            self.seeded = true;
            return Ok(Value::F64(self.value));
        }

        self.value = self.alpha * current_price + (1.0 - self.alpha) * self.value;
        Ok(Value::F64(self.value))
    }
}

#[cfg(test)]
mod tests {
    use super::EmaState;
    use crate::types::Value;
    use crate::vm::SeriesBuffer;

    fn assert_f64_eq(value: Value, expected: f64) {
        match value {
            Value::F64(value) => assert!((value - expected).abs() < 1e-9),
            other => panic!("expected f64, got {other:?}"),
        }
    }

    #[test]
    fn seeds_from_simple_moving_average_then_updates_incrementally() {
        let mut state = EmaState::new(3);
        let mut buffer = SeriesBuffer::new(8);
        buffer.push(Value::F64(1.0));
        buffer.push(Value::F64(2.0));
        buffer.push(Value::F64(3.0));

        let seeded = state
            .update(3.0, &buffer, 0)
            .expect("ema seed should succeed");
        assert_f64_eq(seeded, 2.0);
        assert!(state.is_seeded());

        buffer.push(Value::F64(4.0));
        let next = state
            .update(4.0, &buffer, 0)
            .expect("ema update should succeed");
        assert_f64_eq(next, 3.0);
    }

    #[test]
    fn returns_na_until_seed_window_is_available() {
        let mut state = EmaState::new(3);
        let mut buffer = SeriesBuffer::new(4);
        buffer.push(Value::F64(1.0));
        buffer.push(Value::F64(2.0));

        let value = state
            .update(2.0, &buffer, 0)
            .expect("ema should succeed with short history");

        assert_eq!(value, Value::NA);
        assert!(!state.is_seeded());
    }

    #[test]
    fn returns_na_when_seed_window_contains_na() {
        let mut state = EmaState::new(3);
        let mut buffer = SeriesBuffer::new(4);
        buffer.push(Value::F64(1.0));
        buffer.push(Value::NA);
        buffer.push(Value::F64(3.0));

        let value = state
            .update(3.0, &buffer, 0)
            .expect("ema should succeed when seed window contains na");

        assert_eq!(value, Value::NA);
        assert!(!state.is_seeded());
    }
}
