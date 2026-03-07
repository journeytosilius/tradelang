//! Additional moving-average style indicators used by the TA-Lib expansion.

use crate::diagnostic::RuntimeError;
use crate::talib::MaType;
use crate::types::Value;
use crate::vm::SeriesBuffer;

use super::{calculate_stddev, calculate_wma, EmaState, SmaState};

#[derive(Clone, Debug)]
pub(crate) enum MovingAverageState {
    Sma(SmaState),
    Ema(EmaState),
    Wma(WindowCacheState),
    Dema(DemaState),
    Tema(TemaState),
    Trima(WindowCacheState),
    Kama(KamaState),
    T3(Box<T3State>),
}

impl MovingAverageState {
    pub(crate) fn new(ma_type: MaType, window: usize) -> Result<Self, RuntimeError> {
        Ok(match ma_type {
            MaType::Sma => Self::Sma(SmaState::new(window)),
            MaType::Ema => Self::Ema(EmaState::new(window)),
            MaType::Wma => Self::Wma(WindowCacheState::new(window)),
            MaType::Dema => Self::Dema(DemaState::new(window)),
            MaType::Tema => Self::Tema(TemaState::new(window)),
            MaType::Trima => Self::Trima(WindowCacheState::new(window)),
            MaType::Kama => Self::Kama(KamaState::new(window)),
            MaType::T3 => Self::T3(Box::new(T3State::new(window, 0.7))),
            MaType::Mama => {
                return Err(RuntimeError::UnsupportedMaType {
                    builtin: "ma-type",
                    ma_type: "mama",
                })
            }
        })
    }

    pub(crate) fn update(
        &mut self,
        buffer: &SeriesBuffer,
        pc: usize,
    ) -> Result<Value, RuntimeError> {
        match self {
            Self::Sma(state) => Ok(match state.update(buffer, pc)? {
                Some(value) => value,
                None => state.cached_output(),
            }),
            Self::Ema(state) => state.update(buffer, pc),
            Self::Wma(cache) => cache.update(buffer, pc, calculate_wma),
            Self::Dema(state) => state.update(buffer, pc),
            Self::Tema(state) => state.update(buffer, pc),
            Self::Trima(cache) => cache.update(buffer, pc, calculate_trima),
            Self::Kama(state) => state.update(buffer, pc),
            Self::T3(state) => state.update(buffer, pc),
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct BbandsState {
    middle: MovingAverageState,
    window: usize,
    deviations_up: f64,
    deviations_down: f64,
    last_seen_version: u64,
    cached_output: Value,
}

impl BbandsState {
    pub(crate) fn new(
        window: usize,
        deviations_up: f64,
        deviations_down: f64,
        ma_type: MaType,
    ) -> Result<Self, RuntimeError> {
        Ok(Self {
            middle: MovingAverageState::new(ma_type, window)?,
            window,
            deviations_up,
            deviations_down,
            last_seen_version: 0,
            cached_output: na_tuple3(),
        })
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

        let middle = self.middle.update(buffer, pc)?;
        let stddev = calculate_stddev(buffer, self.window, 1.0, pc)?;
        self.cached_output = match (middle, stddev) {
            (Value::F64(middle), Value::F64(stddev)) => Value::Tuple3([
                Box::new(Value::F64(middle + stddev * self.deviations_up)),
                Box::new(Value::F64(middle)),
                Box::new(Value::F64(middle - stddev * self.deviations_down)),
            ]),
            (Value::NA, _) | (_, Value::NA) => na_tuple3(),
            (other, _) => {
                return Err(RuntimeError::TypeMismatch {
                    pc,
                    expected: "f64",
                    found: other.type_name(),
                })
            }
        };
        Ok(self.cached_output.clone())
    }
}

#[derive(Clone, Debug)]
pub(crate) struct WindowCacheState {
    window: usize,
    last_seen_version: u64,
    cached_output: Value,
}

impl WindowCacheState {
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
        calculate: fn(&SeriesBuffer, usize, usize) -> Result<Value, RuntimeError>,
    ) -> Result<Value, RuntimeError> {
        if buffer.version() == self.last_seen_version {
            return Ok(self.cached_output.clone());
        }
        self.last_seen_version = buffer.version();
        self.cached_output = calculate(buffer, self.window, pc)?;
        Ok(self.cached_output.clone())
    }
}

#[derive(Clone, Debug)]
pub(crate) struct DemaState {
    ema1: EmaState,
    ema2: EmaState,
    ema1_buffer: SeriesBuffer,
    last_seen_version: u64,
    cached_output: Value,
}

impl DemaState {
    pub(crate) fn new(window: usize) -> Self {
        Self {
            ema1: EmaState::new(window),
            ema2: EmaState::new(window),
            ema1_buffer: SeriesBuffer::new(window.max(2)),
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

        let ema1 = self.ema1.update(buffer, pc)?;
        let Value::F64(ema1) = ema1 else {
            self.cached_output = Value::NA;
            return Ok(Value::NA);
        };
        self.ema1_buffer.push(Value::F64(ema1));
        let ema2 = self.ema2.update(&self.ema1_buffer, pc)?;
        let Value::F64(ema2) = ema2 else {
            self.cached_output = Value::NA;
            return Ok(Value::NA);
        };
        self.cached_output = Value::F64((2.0 * ema1) - ema2);
        Ok(self.cached_output.clone())
    }
}

#[derive(Clone, Debug)]
pub(crate) struct TemaState {
    ema1: EmaState,
    ema2: EmaState,
    ema3: EmaState,
    ema1_buffer: SeriesBuffer,
    ema2_buffer: SeriesBuffer,
    last_seen_version: u64,
    cached_output: Value,
}

impl TemaState {
    pub(crate) fn new(window: usize) -> Self {
        Self {
            ema1: EmaState::new(window),
            ema2: EmaState::new(window),
            ema3: EmaState::new(window),
            ema1_buffer: SeriesBuffer::new(window.max(2)),
            ema2_buffer: SeriesBuffer::new(window.max(2)),
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

        let ema1 = self.ema1.update(buffer, pc)?;
        let Value::F64(ema1) = ema1 else {
            self.cached_output = Value::NA;
            return Ok(Value::NA);
        };
        self.ema1_buffer.push(Value::F64(ema1));
        let ema2 = self.ema2.update(&self.ema1_buffer, pc)?;
        let Value::F64(ema2) = ema2 else {
            self.cached_output = Value::NA;
            return Ok(Value::NA);
        };
        self.ema2_buffer.push(Value::F64(ema2));
        let ema3 = self.ema3.update(&self.ema2_buffer, pc)?;
        let Value::F64(ema3) = ema3 else {
            self.cached_output = Value::NA;
            return Ok(Value::NA);
        };
        self.cached_output = Value::F64((3.0 * ema1) - (3.0 * ema2) + ema3);
        Ok(self.cached_output.clone())
    }
}

#[derive(Clone, Debug)]
pub(crate) struct TrixState {
    tema: TemaCoreState,
    prev_ema3: Option<f64>,
    last_seen_version: u64,
    cached_output: Value,
}

impl TrixState {
    pub(crate) fn new(window: usize) -> Self {
        Self {
            tema: TemaCoreState::new(window),
            prev_ema3: None,
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

        let Some((_ema1, _ema2, ema3)) = self.tema.update(buffer, pc)? else {
            self.cached_output = Value::NA;
            return Ok(Value::NA);
        };

        let output = match self.prev_ema3 {
            Some(previous) if previous != 0.0 => Value::F64(((ema3 / previous) - 1.0) * 100.0),
            _ => Value::NA,
        };
        self.prev_ema3 = Some(ema3);
        self.cached_output = output.clone();
        Ok(output)
    }
}

#[derive(Clone, Debug)]
struct TemaCoreState {
    ema1: EmaState,
    ema2: EmaState,
    ema3: EmaState,
    ema1_buffer: SeriesBuffer,
    ema2_buffer: SeriesBuffer,
}

impl TemaCoreState {
    fn new(window: usize) -> Self {
        Self {
            ema1: EmaState::new(window),
            ema2: EmaState::new(window),
            ema3: EmaState::new(window),
            ema1_buffer: SeriesBuffer::new(window.max(2)),
            ema2_buffer: SeriesBuffer::new(window.max(2)),
        }
    }

    fn update(
        &mut self,
        buffer: &SeriesBuffer,
        pc: usize,
    ) -> Result<Option<(f64, f64, f64)>, RuntimeError> {
        let ema1 = self.ema1.update(buffer, pc)?;
        let Value::F64(ema1) = ema1 else {
            return Ok(None);
        };
        self.ema1_buffer.push(Value::F64(ema1));
        let ema2 = self.ema2.update(&self.ema1_buffer, pc)?;
        let Value::F64(ema2) = ema2 else {
            return Ok(None);
        };
        self.ema2_buffer.push(Value::F64(ema2));
        let ema3 = self.ema3.update(&self.ema2_buffer, pc)?;
        let Value::F64(ema3) = ema3 else {
            return Ok(None);
        };
        Ok(Some((ema1, ema2, ema3)))
    }
}

#[derive(Clone, Debug)]
pub(crate) struct T3State {
    ema1: EmaState,
    ema2: EmaState,
    ema3: EmaState,
    ema4: EmaState,
    ema5: EmaState,
    ema6: EmaState,
    buffer1: SeriesBuffer,
    buffer2: SeriesBuffer,
    buffer3: SeriesBuffer,
    buffer4: SeriesBuffer,
    buffer5: SeriesBuffer,
    volume_factor: f64,
    last_seen_version: u64,
    cached_output: Value,
}

impl T3State {
    pub(crate) fn new(window: usize, volume_factor: f64) -> Self {
        Self {
            ema1: EmaState::new(window),
            ema2: EmaState::new(window),
            ema3: EmaState::new(window),
            ema4: EmaState::new(window),
            ema5: EmaState::new(window),
            ema6: EmaState::new(window),
            buffer1: SeriesBuffer::new(window.max(2)),
            buffer2: SeriesBuffer::new(window.max(2)),
            buffer3: SeriesBuffer::new(window.max(2)),
            buffer4: SeriesBuffer::new(window.max(2)),
            buffer5: SeriesBuffer::new(window.max(2)),
            volume_factor,
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

        let Some((_, _, e3, e4, e5, e6)) = self.update_chain(buffer, pc)? else {
            self.cached_output = Value::NA;
            return Ok(Value::NA);
        };

        let a = self.volume_factor;
        let c1 = -(a * a * a);
        let c2 = 3.0 * a * a + 3.0 * a * a * a;
        let c3 = -6.0 * a * a - 3.0 * a - 3.0 * a * a * a;
        let c4 = 1.0 + 3.0 * a + 3.0 * a * a + a * a * a;
        self.cached_output = Value::F64((c1 * e6) + (c2 * e5) + (c3 * e4) + (c4 * e3));
        Ok(self.cached_output.clone())
    }

    fn update_chain(
        &mut self,
        buffer: &SeriesBuffer,
        pc: usize,
    ) -> Result<Option<T3ChainValues>, RuntimeError> {
        let e1 = self.ema1.update(buffer, pc)?;
        let Value::F64(e1) = e1 else {
            return Ok(None);
        };
        self.buffer1.push(Value::F64(e1));
        let e2 = self.ema2.update(&self.buffer1, pc)?;
        let Value::F64(e2) = e2 else {
            return Ok(None);
        };
        self.buffer2.push(Value::F64(e2));
        let e3 = self.ema3.update(&self.buffer2, pc)?;
        let Value::F64(e3) = e3 else {
            return Ok(None);
        };
        self.buffer3.push(Value::F64(e3));
        let e4 = self.ema4.update(&self.buffer3, pc)?;
        let Value::F64(e4) = e4 else {
            return Ok(None);
        };
        self.buffer4.push(Value::F64(e4));
        let e5 = self.ema5.update(&self.buffer4, pc)?;
        let Value::F64(e5) = e5 else {
            return Ok(None);
        };
        self.buffer5.push(Value::F64(e5));
        let e6 = self.ema6.update(&self.buffer5, pc)?;
        let Value::F64(e6) = e6 else {
            return Ok(None);
        };
        Ok(Some((e1, e2, e3, e4, e5, e6)))
    }
}

type T3ChainValues = (f64, f64, f64, f64, f64, f64);

#[derive(Clone, Debug)]
pub(crate) struct KamaState {
    window: usize,
    previous: Option<f64>,
    last_seen_version: u64,
    cached_output: Value,
}

impl KamaState {
    pub(crate) fn new(window: usize) -> Self {
        Self {
            window,
            previous: None,
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

        if buffer.len() < self.window + 1 {
            self.cached_output = Value::NA;
            return Ok(Value::NA);
        }

        let current = expect_buffer_f64(buffer, 0, pc)?;
        let trailing = expect_buffer_f64(buffer, self.window, pc)?;
        let previous_price = expect_buffer_f64(buffer, 1, pc)?;
        let period_roc = current - trailing;

        let mut sum_roc1 = 0.0;
        for offset in 0..self.window {
            let newer = expect_buffer_f64(buffer, offset, pc)?;
            let older = expect_buffer_f64(buffer, offset + 1, pc)?;
            sum_roc1 += (newer - older).abs();
        }

        let efficiency = if sum_roc1 == 0.0 || sum_roc1 <= period_roc.abs() {
            1.0
        } else {
            (period_roc / sum_roc1).abs()
        };
        let slow = 2.0 / 31.0;
        let fast = 2.0 / 3.0;
        let smoothing = (efficiency * (fast - slow) + slow).powi(2);
        let previous = self.previous.unwrap_or(previous_price);
        let next = ((current - previous) * smoothing) + previous;
        self.previous = Some(next);
        self.cached_output = Value::F64(next);
        Ok(self.cached_output.clone())
    }
}

pub(crate) fn calculate_trima(
    buffer: &SeriesBuffer,
    window: usize,
    pc: usize,
) -> Result<Value, RuntimeError> {
    if buffer.len() < window {
        return Ok(Value::NA);
    }

    let even = window.is_multiple_of(2);
    let peak = if even { window / 2 } else { window.div_ceil(2) };
    let total_weight = if even { peak * (peak + 1) } else { peak * peak } as f64;

    let mut sum = 0.0;
    for index in 0..window {
        let value = buffer.get(window - 1 - index);
        let weight = if even {
            if index < peak {
                index + 1
            } else {
                window - index
            }
        } else if index < peak {
            index + 1
        } else {
            window - index
        } as f64;

        match value {
            Value::F64(value) => sum += value * weight,
            Value::NA => return Ok(Value::NA),
            other => {
                return Err(RuntimeError::TypeMismatch {
                    pc,
                    expected: "f64",
                    found: other.type_name(),
                })
            }
        }
    }

    Ok(Value::F64(sum / total_weight))
}

fn expect_buffer_f64(buffer: &SeriesBuffer, offset: usize, pc: usize) -> Result<f64, RuntimeError> {
    match buffer.get(offset) {
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

fn na_tuple3() -> Value {
    Value::Tuple3([
        Box::new(Value::NA),
        Box::new(Value::NA),
        Box::new(Value::NA),
    ])
}

#[cfg(test)]
mod tests {
    use super::{calculate_trima, DemaState, KamaState, T3State, TemaState, TrixState};
    use crate::types::Value;
    use crate::vm::SeriesBuffer;

    #[test]
    fn trima_matches_triangle_weights_for_odd_window() {
        let mut buffer = SeriesBuffer::new(8);
        for value in [1.0, 2.0, 3.0, 4.0, 5.0] {
            buffer.push(Value::F64(value));
        }
        let value = calculate_trima(&buffer, 5, 0).unwrap();
        assert_eq!(value, Value::F64(3.0));
    }

    #[test]
    fn dema_seeds_after_nested_ema_is_ready() {
        let mut state = DemaState::new(3);
        let mut buffer = SeriesBuffer::new(8);
        for value in [1.0, 2.0, 3.0, 4.0, 5.0] {
            buffer.push(Value::F64(value));
            let _ = state.update(&buffer, 0).unwrap();
        }
        assert!(matches!(state.update(&buffer, 0).unwrap(), Value::F64(_)));
    }

    #[test]
    fn tema_seeds_after_triple_ema_is_ready() {
        let mut state = TemaState::new(3);
        let mut buffer = SeriesBuffer::new(16);
        for value in [1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0] {
            buffer.push(Value::F64(value));
            let _ = state.update(&buffer, 0).unwrap();
        }
        assert!(matches!(state.update(&buffer, 0).unwrap(), Value::F64(_)));
    }

    #[test]
    fn t3_seeds_after_six_ema_chain_is_ready() {
        let mut state = T3State::new(3, 0.7);
        let mut buffer = SeriesBuffer::new(32);
        for value in 1..=24 {
            buffer.push(Value::F64(value as f64));
            let _ = state.update(&buffer, 0).unwrap();
        }
        assert!(matches!(state.update(&buffer, 0).unwrap(), Value::F64(_)));
    }

    #[test]
    fn kama_uses_previous_close_as_first_seed() {
        let mut state = KamaState::new(3);
        let mut buffer = SeriesBuffer::new(8);
        for value in [1.0, 2.0, 3.0, 4.0] {
            buffer.push(Value::F64(value));
        }
        assert!(matches!(state.update(&buffer, 0).unwrap(), Value::F64(_)));
    }

    #[test]
    fn trix_returns_na_before_previous_triple_ema_exists() {
        let mut state = TrixState::new(3);
        let mut buffer = SeriesBuffer::new(24);
        for value in 1..=16 {
            buffer.push(Value::F64(value as f64));
            let _ = state.update(&buffer, 0).unwrap();
        }
        assert!(matches!(state.update(&buffer, 0).unwrap(), Value::F64(_)));
    }
}
