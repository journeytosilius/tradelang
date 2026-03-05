//! Simple moving average indicator logic.
//!
//! SMA is computed from a bounded recent window over a series buffer and
//! returns `na` until enough history is available.

use crate::diagnostic::RuntimeError;
use crate::types::Value;
use crate::vm::SeriesBuffer;

#[derive(Clone, Debug)]
pub(crate) struct SmaState {
    window: usize,
    last_seen_version: u64,
    cached_output: Value,
}

impl SmaState {
    pub(crate) fn new(window: usize) -> Self {
        Self {
            window,
            last_seen_version: 0,
            cached_output: Value::NA,
        }
    }

    pub(crate) fn update(
        &mut self,
        buffer: &SeriesBuffer,
        pc: usize,
    ) -> Result<Option<Value>, RuntimeError> {
        let version = buffer.version();
        if version == self.last_seen_version {
            return Ok(None);
        }
        self.last_seen_version = version;
        let value = calculate(buffer, self.window, pc)?;
        self.cached_output = value.clone();
        Ok(Some(value))
    }

    pub(crate) fn cached_output(&self) -> Value {
        self.cached_output.clone()
    }
}

pub(crate) fn calculate(
    buffer: &SeriesBuffer,
    window: usize,
    pc: usize,
) -> Result<Value, RuntimeError> {
    if buffer.len() < window {
        return Ok(Value::NA);
    }

    let mut sum = 0.0;
    for value in buffer.iter_recent(window) {
        match value {
            Value::F64(value) => sum += value,
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

    Ok(Value::F64(sum / window as f64))
}

#[cfg(test)]
mod tests {
    use super::calculate;
    use crate::types::Value;
    use crate::vm::SeriesBuffer;

    #[test]
    fn calculates_average_for_recent_window() {
        let mut buffer = SeriesBuffer::new(8);
        buffer.push(Value::F64(1.0));
        buffer.push(Value::F64(2.0));
        buffer.push(Value::F64(3.0));
        buffer.push(Value::F64(4.0));

        let value = calculate(&buffer, 3, 0).expect("sma should succeed");

        assert_eq!(value, Value::F64(3.0));
    }

    #[test]
    fn returns_na_when_history_is_too_short() {
        let mut buffer = SeriesBuffer::new(4);
        buffer.push(Value::F64(1.0));
        buffer.push(Value::F64(2.0));

        let value = calculate(&buffer, 3, 0).expect("sma should succeed");

        assert_eq!(value, Value::NA);
    }

    #[test]
    fn returns_na_when_window_contains_na() {
        let mut buffer = SeriesBuffer::new(4);
        buffer.push(Value::F64(1.0));
        buffer.push(Value::NA);
        buffer.push(Value::F64(3.0));

        let value = calculate(&buffer, 3, 0).expect("sma should succeed");

        assert_eq!(value, Value::NA);
    }
}
