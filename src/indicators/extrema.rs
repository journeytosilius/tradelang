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

pub(crate) fn calculate_highest_bars(
    buffer: &SeriesBuffer,
    window: usize,
    pc: usize,
) -> Result<Value, RuntimeError> {
    extrema_window(buffer, window, pc).map(|window| match window {
        Some(window) => Value::F64((buffer.version() as usize - 1 - window.max_index) as f64),
        None => Value::NA,
    })
}

pub(crate) fn calculate_lowest_bars(
    buffer: &SeriesBuffer,
    window: usize,
    pc: usize,
) -> Result<Value, RuntimeError> {
    extrema_window(buffer, window, pc).map(|window| match window {
        Some(window) => Value::F64((buffer.version() as usize - 1 - window.min_index) as f64),
        None => Value::NA,
    })
}

pub(crate) fn calculate_aroon(
    high: &SeriesBuffer,
    low: &SeriesBuffer,
    window: usize,
    pc: usize,
) -> Result<Value, RuntimeError> {
    aroon_offsets(high, low, window, pc).map(|offsets| match offsets {
        Some(offsets) => {
            let factor = 100.0 / window as f64;
            Value::Tuple2([
                Box::new(Value::F64(factor * (window - offsets.lowest_offset) as f64)),
                Box::new(Value::F64(
                    factor * (window - offsets.highest_offset) as f64,
                )),
            ])
        }
        None => na_tuple2(),
    })
}

pub(crate) fn calculate_aroonosc(
    high: &SeriesBuffer,
    low: &SeriesBuffer,
    window: usize,
    pc: usize,
) -> Result<Value, RuntimeError> {
    aroon_offsets(high, low, window, pc).map(|offsets| match offsets {
        Some(offsets) => Value::F64(
            (100.0 / window as f64)
                * (offsets.lowest_offset as f64 - offsets.highest_offset as f64),
        ),
        None => Value::NA,
    })
}

pub(crate) fn calculate_willr(
    high: &SeriesBuffer,
    low: &SeriesBuffer,
    close: &SeriesBuffer,
    window: usize,
    pc: usize,
) -> Result<Value, RuntimeError> {
    if high.len() < window || low.len() < window || close.is_empty() {
        return Ok(Value::NA);
    }

    let mut highest = f64::NEG_INFINITY;
    for value in high.iter_recent(window) {
        match value {
            Value::F64(value) => highest = highest.max(*value),
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

    let mut lowest = f64::INFINITY;
    for value in low.iter_recent(window) {
        match value {
            Value::F64(value) => lowest = lowest.min(*value),
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

    let current_close = match close.get(0) {
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

    let denominator = (highest - lowest) / -100.0;
    if denominator != 0.0 {
        Ok(Value::F64((highest - current_close) / denominator))
    } else {
        Ok(Value::F64(0.0))
    }
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct AroonOffsets {
    highest_offset: usize,
    lowest_offset: usize,
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

fn aroon_offsets(
    high: &SeriesBuffer,
    low: &SeriesBuffer,
    window: usize,
    pc: usize,
) -> Result<Option<AroonOffsets>, RuntimeError> {
    let sample_count = window + 1;
    if high.len() < sample_count || low.len() < sample_count {
        return Ok(None);
    }

    let mut highest: Option<(f64, usize)> = None;
    let mut lowest: Option<(f64, usize)> = None;

    for (offset, (high_value, low_value)) in high
        .iter_recent(sample_count)
        .zip(low.iter_recent(sample_count))
        .enumerate()
    {
        let high_value = match high_value {
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
        let low_value = match low_value {
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

        match highest {
            Some((current, _)) if current >= high_value => {}
            _ => highest = Some((high_value, offset)),
        }
        match lowest {
            Some((current, _)) if current <= low_value => {}
            _ => lowest = Some((low_value, offset)),
        }
    }

    Ok(match (highest, lowest) {
        (Some((_, highest_offset)), Some((_, lowest_offset))) => Some(AroonOffsets {
            highest_offset,
            lowest_offset,
        }),
        _ => None,
    })
}

fn na_tuple2() -> Value {
    Value::Tuple2([Box::new(Value::NA), Box::new(Value::NA)])
}

#[cfg(test)]
mod tests {
    use super::{
        calculate_aroon, calculate_aroonosc, calculate_falling, calculate_highest,
        calculate_highest_bars, calculate_lowest, calculate_lowest_bars, calculate_max_index,
        calculate_min_index, calculate_min_max, calculate_min_max_index, calculate_rising,
        calculate_willr,
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

    #[test]
    fn bar_offset_helpers_return_recent_offsets() {
        let mut buffer = SeriesBuffer::new(8);
        for value in [4.0, 7.0, 2.0, 6.0] {
            buffer.push(Value::F64(value));
        }

        assert_eq!(
            calculate_highest_bars(&buffer, 4, 0).unwrap(),
            Value::F64(2.0)
        );
        assert_eq!(
            calculate_lowest_bars(&buffer, 4, 0).unwrap(),
            Value::F64(1.0)
        );
    }

    #[test]
    fn willr_uses_trailing_high_low_close_window() {
        let mut high = SeriesBuffer::new(8);
        let mut low = SeriesBuffer::new(8);
        let mut close = SeriesBuffer::new(8);
        for (high_value, low_value, close_value) in
            [(11.0, 9.0, 10.0), (12.0, 10.0, 11.0), (13.0, 11.0, 12.0)]
        {
            high.push(Value::F64(high_value));
            low.push(Value::F64(low_value));
            close.push(Value::F64(close_value));
        }

        assert_eq!(
            calculate_willr(&high, &low, &close, 3, 0).unwrap(),
            Value::F64(-25.0)
        );
    }

    #[test]
    fn aroon_returns_talib_down_up_order() {
        let mut high = SeriesBuffer::new(8);
        let mut low = SeriesBuffer::new(8);
        for (high_value, low_value) in [(1.0, -1.0), (2.0, 0.0), (3.0, 1.0), (4.0, 2.0)] {
            high.push(Value::F64(high_value));
            low.push(Value::F64(low_value));
        }

        assert_eq!(
            calculate_aroon(&high, &low, 3, 0).unwrap(),
            Value::Tuple2([Box::new(Value::F64(0.0)), Box::new(Value::F64(100.0))])
        );
        assert_eq!(
            calculate_aroonosc(&high, &low, 3, 0).unwrap(),
            Value::F64(100.0)
        );
    }

    #[test]
    fn aroon_prefers_newest_equal_extrema_like_talib() {
        let mut high = SeriesBuffer::new(8);
        let mut low = SeriesBuffer::new(8);
        for (high_value, low_value) in [(1.0, 0.0), (3.0, 1.0), (3.0, 2.0)] {
            high.push(Value::F64(high_value));
            low.push(Value::F64(low_value));
        }

        assert_eq!(
            calculate_aroon(&high, &low, 2, 0).unwrap(),
            Value::Tuple2([Box::new(Value::F64(0.0)), Box::new(Value::F64(100.0))])
        );
    }
}
