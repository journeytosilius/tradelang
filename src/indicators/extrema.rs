//! Rolling extrema and directional helper state.

use crate::diagnostic::RuntimeError;
use crate::types::Value;
use crate::vm::SeriesBuffer;

#[derive(Clone, Debug)]
pub(crate) struct HighestState {
    window: usize,
    last_seen_version: u64,
    cached_output: Value,
}

#[derive(Clone, Debug)]
pub(crate) struct LowestState {
    window: usize,
    last_seen_version: u64,
    cached_output: Value,
}

#[derive(Clone, Debug)]
pub(crate) struct RisingState {
    window: usize,
    last_seen_version: u64,
    cached_output: Value,
}

#[derive(Clone, Debug)]
pub(crate) struct FallingState {
    window: usize,
    last_seen_version: u64,
    cached_output: Value,
}

impl HighestState {
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
    ) -> Result<Value, RuntimeError> {
        if buffer.version() == self.last_seen_version {
            return Ok(self.cached_output.clone());
        }
        self.last_seen_version = buffer.version();
        self.cached_output = calculate_highest(buffer, self.window, pc)?;
        Ok(self.cached_output.clone())
    }
}

impl LowestState {
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
    ) -> Result<Value, RuntimeError> {
        if buffer.version() == self.last_seen_version {
            return Ok(self.cached_output.clone());
        }
        self.last_seen_version = buffer.version();
        self.cached_output = calculate_lowest(buffer, self.window, pc)?;
        Ok(self.cached_output.clone())
    }
}

impl RisingState {
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
    ) -> Result<Value, RuntimeError> {
        if buffer.version() == self.last_seen_version {
            return Ok(self.cached_output.clone());
        }
        self.last_seen_version = buffer.version();
        self.cached_output = calculate_rising(buffer, self.window, pc)?;
        Ok(self.cached_output.clone())
    }
}

impl FallingState {
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
    ) -> Result<Value, RuntimeError> {
        if buffer.version() == self.last_seen_version {
            return Ok(self.cached_output.clone());
        }
        self.last_seen_version = buffer.version();
        self.cached_output = calculate_falling(buffer, self.window, pc)?;
        Ok(self.cached_output.clone())
    }
}

pub(crate) fn calculate_highest(
    buffer: &SeriesBuffer,
    window: usize,
    pc: usize,
) -> Result<Value, RuntimeError> {
    fold_extrema(buffer, window, pc, true)
}

pub(crate) fn calculate_lowest(
    buffer: &SeriesBuffer,
    window: usize,
    pc: usize,
) -> Result<Value, RuntimeError> {
    fold_extrema(buffer, window, pc, false)
}

pub(crate) fn calculate_rising(
    buffer: &SeriesBuffer,
    window: usize,
    pc: usize,
) -> Result<Value, RuntimeError> {
    directional_compare(buffer, window, pc, true)
}

pub(crate) fn calculate_falling(
    buffer: &SeriesBuffer,
    window: usize,
    pc: usize,
) -> Result<Value, RuntimeError> {
    directional_compare(buffer, window, pc, false)
}

pub(crate) fn calculate_max_index(
    buffer: &SeriesBuffer,
    window: usize,
    pc: usize,
) -> Result<Value, RuntimeError> {
    extrema_window(buffer, window, pc).map(|window| match window {
        Some(window) => Value::F64(window.max_index as f64),
        None => Value::NA,
    })
}

pub(crate) fn calculate_min_index(
    buffer: &SeriesBuffer,
    window: usize,
    pc: usize,
) -> Result<Value, RuntimeError> {
    extrema_window(buffer, window, pc).map(|window| match window {
        Some(window) => Value::F64(window.min_index as f64),
        None => Value::NA,
    })
}

pub(crate) fn calculate_min_max(
    buffer: &SeriesBuffer,
    window: usize,
    pc: usize,
) -> Result<Value, RuntimeError> {
    extrema_window(buffer, window, pc).map(|window| match window {
        Some(window) => Value::Tuple2([
            Box::new(Value::F64(window.min_value)),
            Box::new(Value::F64(window.max_value)),
        ]),
        None => na_tuple2(),
    })
}

pub(crate) fn calculate_min_max_index(
    buffer: &SeriesBuffer,
    window: usize,
    pc: usize,
) -> Result<Value, RuntimeError> {
    extrema_window(buffer, window, pc).map(|window| match window {
        Some(window) => Value::Tuple2([
            Box::new(Value::F64(window.min_index as f64)),
            Box::new(Value::F64(window.max_index as f64)),
        ]),
        None => na_tuple2(),
    })
}

fn fold_extrema(
    buffer: &SeriesBuffer,
    window: usize,
    pc: usize,
    highest: bool,
) -> Result<Value, RuntimeError> {
    if buffer.len() < window {
        return Ok(Value::NA);
    }

    let mut extrema = if highest {
        f64::NEG_INFINITY
    } else {
        f64::INFINITY
    };
    for value in buffer.iter_recent(window) {
        match value {
            Value::F64(value) => {
                extrema = if highest {
                    extrema.max(*value)
                } else {
                    extrema.min(*value)
                };
            }
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

    Ok(Value::F64(extrema))
}

fn directional_compare(
    buffer: &SeriesBuffer,
    window: usize,
    pc: usize,
    rising: bool,
) -> Result<Value, RuntimeError> {
    if buffer.len() < window + 1 {
        return Ok(Value::NA);
    }

    let current = match buffer.get(0) {
        Value::F64(value) => value,
        Value::NA => return Ok(Value::NA),
        other => {
            return Err(RuntimeError::TypeMismatch {
                pc,
                expected: "f64",
                found: other.type_name(),
            });
        }
    };

    for offset in 1..=window {
        match buffer.get(offset) {
            Value::F64(value) => {
                if rising {
                    if current <= value {
                        return Ok(Value::Bool(false));
                    }
                } else if current >= value {
                    return Ok(Value::Bool(false));
                }
            }
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

    Ok(Value::Bool(true))
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct ExtremaWindow {
    min_value: f64,
    min_index: usize,
    max_value: f64,
    max_index: usize,
}

fn extrema_window(
    buffer: &SeriesBuffer,
    window: usize,
    pc: usize,
) -> Result<Option<ExtremaWindow>, RuntimeError> {
    if buffer.len() < window {
        return Ok(None);
    }

    let latest_index = buffer.version() as usize - 1;
    let mut extrema: Option<ExtremaWindow> = None;
    for (offset, value) in buffer.iter_recent(window).enumerate() {
        let absolute_index = latest_index - offset;
        let value = match value {
            Value::F64(value) => *value,
            Value::NA => return Ok(None),
            other => {
                return Err(RuntimeError::TypeMismatch {
                    pc,
                    expected: "f64",
                    found: other.type_name(),
                });
            }
        };

        match &mut extrema {
            Some(extrema) => {
                if value > extrema.max_value {
                    extrema.max_value = value;
                    extrema.max_index = absolute_index;
                }
                if value < extrema.min_value {
                    extrema.min_value = value;
                    extrema.min_index = absolute_index;
                }
            }
            None => {
                extrema = Some(ExtremaWindow {
                    min_value: value,
                    min_index: absolute_index,
                    max_value: value,
                    max_index: absolute_index,
                });
            }
        }
    }

    Ok(extrema)
}

fn na_tuple2() -> Value {
    Value::Tuple2([Box::new(Value::NA), Box::new(Value::NA)])
}

#[cfg(test)]
mod tests {
    use super::{
        calculate_falling, calculate_highest, calculate_lowest, calculate_max_index,
        calculate_min_index, calculate_min_max, calculate_min_max_index, calculate_rising,
    };
    use crate::types::Value;
    use crate::vm::SeriesBuffer;

    #[test]
    fn highest_and_lowest_use_trailing_window() {
        let mut buffer = SeriesBuffer::new(8);
        for value in [1.0, 4.0, 2.0, 3.0] {
            buffer.push(Value::F64(value));
        }

        assert_eq!(calculate_highest(&buffer, 3, 0).unwrap(), Value::F64(4.0));
        assert_eq!(calculate_lowest(&buffer, 3, 0).unwrap(), Value::F64(2.0));
    }

    #[test]
    fn rising_and_falling_compare_against_prior_window() {
        let mut rising = SeriesBuffer::new(8);
        for value in [1.0, 2.0, 3.0] {
            rising.push(Value::F64(value));
        }
        assert_eq!(calculate_rising(&rising, 2, 0).unwrap(), Value::Bool(true));

        let mut falling = SeriesBuffer::new(8);
        for value in [3.0, 2.0, 1.0] {
            falling.push(Value::F64(value));
        }
        assert_eq!(
            calculate_falling(&falling, 2, 0).unwrap(),
            Value::Bool(true)
        );
    }

    #[test]
    fn extrema_helpers_propagate_na() {
        let mut buffer = SeriesBuffer::new(4);
        buffer.push(Value::F64(1.0));
        buffer.push(Value::NA);
        buffer.push(Value::F64(3.0));
        assert_eq!(calculate_highest(&buffer, 3, 0).unwrap(), Value::NA);
        assert_eq!(calculate_rising(&buffer, 2, 0).unwrap(), Value::NA);
    }

    #[test]
    fn index_helpers_return_absolute_indices() {
        let mut buffer = SeriesBuffer::new(8);
        for value in [3.0, 5.0, 2.0, 4.0] {
            buffer.push(Value::F64(value));
        }

        assert_eq!(calculate_max_index(&buffer, 3, 0).unwrap(), Value::F64(1.0));
        assert_eq!(calculate_min_index(&buffer, 3, 0).unwrap(), Value::F64(2.0));
    }

    #[test]
    fn index_helpers_prefer_newest_equal_value_like_talib() {
        let mut buffer = SeriesBuffer::new(8);
        for value in [4.0, 2.0, 4.0, 2.0] {
            buffer.push(Value::F64(value));
        }

        assert_eq!(calculate_max_index(&buffer, 4, 0).unwrap(), Value::F64(2.0));
        assert_eq!(calculate_min_index(&buffer, 4, 0).unwrap(), Value::F64(3.0));
    }

    #[test]
    fn tuple_extrema_helpers_return_talib_order() {
        let mut buffer = SeriesBuffer::new(8);
        for value in [3.0, 5.0, 2.0, 4.0] {
            buffer.push(Value::F64(value));
        }

        assert_eq!(
            calculate_min_max(&buffer, 3, 0).unwrap(),
            Value::Tuple2([Box::new(Value::F64(2.0)), Box::new(Value::F64(5.0))])
        );
        assert_eq!(
            calculate_min_max_index(&buffer, 3, 0).unwrap(),
            Value::Tuple2([Box::new(Value::F64(2.0)), Box::new(Value::F64(1.0))])
        );
    }
}
