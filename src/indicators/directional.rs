//! Wilder-style directional movement, ATR, and ADX helpers.

use std::collections::VecDeque;

use crate::diagnostic::RuntimeError;
use crate::types::Value;
use crate::vm::SeriesBuffer;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DmKind {
    Plus,
    Minus,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DirectionalKind {
    Atr,
    Natr,
    PlusDi,
    MinusDi,
    Dx,
    Adx,
    Adxr,
}

#[derive(Clone, Debug)]
pub(crate) struct DmState {
    window: usize,
    kind: DmKind,
    previous_high: Option<f64>,
    previous_low: Option<f64>,
    seeded: usize,
    smoothed_dm: f64,
    last_high_version: u64,
    last_low_version: u64,
    cached_output: Value,
}

impl DmState {
    pub(crate) fn new(window: usize, kind: DmKind) -> Self {
        Self {
            window,
            kind,
            previous_high: None,
            previous_low: None,
            seeded: 0,
            smoothed_dm: 0.0,
            last_high_version: 0,
            last_low_version: 0,
            cached_output: Value::NA,
        }
    }

    pub(crate) fn update(
        &mut self,
        high: &SeriesBuffer,
        low: &SeriesBuffer,
        pc: usize,
    ) -> Result<Value, RuntimeError> {
        if high.version() == self.last_high_version && low.version() == self.last_low_version {
            return Ok(self.cached_output.clone());
        }
        self.last_high_version = high.version();
        self.last_low_version = low.version();

        let high = expect_buffer_f64(high, 0, pc)?;
        let low = expect_buffer_f64(low, 0, pc)?;
        let (Some(previous_high), Some(previous_low)) = (self.previous_high, self.previous_low)
        else {
            self.previous_high = Some(high);
            self.previous_low = Some(low);
            self.cached_output = Value::NA;
            return Ok(Value::NA);
        };

        let (plus_dm, minus_dm) = directional_move(high, low, previous_high, previous_low);
        let current = if matches!(self.kind, DmKind::Plus) {
            plus_dm
        } else {
            minus_dm
        };

        self.previous_high = Some(high);
        self.previous_low = Some(low);

        if self.window <= 1 {
            self.cached_output = Value::F64(current);
            return Ok(self.cached_output.clone());
        }

        if self.seeded < self.window - 1 {
            self.smoothed_dm += current;
            self.seeded += 1;
            if self.seeded < self.window - 1 {
                self.cached_output = Value::NA;
            } else {
                self.cached_output = Value::F64(self.smoothed_dm);
            }
            return Ok(self.cached_output.clone());
        }

        self.smoothed_dm = self.smoothed_dm - (self.smoothed_dm / self.window as f64) + current;
        self.cached_output = Value::F64(self.smoothed_dm);
        Ok(self.cached_output.clone())
    }
}

#[derive(Clone, Debug)]
pub(crate) struct DirectionalState {
    window: usize,
    kind: DirectionalKind,
    previous_high: Option<f64>,
    previous_low: Option<f64>,
    previous_close: Option<f64>,
    seed_count: usize,
    tr_sum: f64,
    plus_dm_sum: f64,
    minus_dm_sum: f64,
    dx_seed_count: usize,
    dx_sum: f64,
    adx: f64,
    adx_history: VecDeque<f64>,
    last_versions: (u64, u64, u64),
    cached_output: Value,
}

impl DirectionalState {
    pub(crate) fn new(window: usize, kind: DirectionalKind) -> Self {
        Self {
            window,
            kind,
            previous_high: None,
            previous_low: None,
            previous_close: None,
            seed_count: 0,
            tr_sum: 0.0,
            plus_dm_sum: 0.0,
            minus_dm_sum: 0.0,
            dx_seed_count: 0,
            dx_sum: 0.0,
            adx: 0.0,
            adx_history: VecDeque::with_capacity(window.max(2)),
            last_versions: (0, 0, 0),
            cached_output: Value::NA,
        }
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

        let high = expect_buffer_f64(high, 0, pc)?;
        let low = expect_buffer_f64(low, 0, pc)?;
        let close = expect_buffer_f64(close, 0, pc)?;
        let (Some(previous_high), Some(previous_low), Some(previous_close)) =
            (self.previous_high, self.previous_low, self.previous_close)
        else {
            self.previous_high = Some(high);
            self.previous_low = Some(low);
            self.previous_close = Some(close);
            self.cached_output = Value::NA;
            return Ok(Value::NA);
        };

        let (plus_dm, minus_dm) = directional_move(high, low, previous_high, previous_low);
        let tr = true_range(high, low, previous_close);

        self.previous_high = Some(high);
        self.previous_low = Some(low);
        self.previous_close = Some(close);

        if self.window <= 1 {
            let result = self.compute_output(close, tr, plus_dm, minus_dm, 0.0, true);
            self.cached_output = result;
            return Ok(self.cached_output.clone());
        }

        let atr_family = matches!(self.kind, DirectionalKind::Atr | DirectionalKind::Natr);
        if atr_family {
            if self.seed_count < self.window {
                self.seed_count += 1;
                self.tr_sum += tr;
                self.plus_dm_sum += plus_dm;
                self.minus_dm_sum += minus_dm;
                if self.seed_count < self.window {
                    self.cached_output = Value::NA;
                    return Ok(Value::NA);
                }
                let atr = self.tr_sum / self.window as f64;
                self.cached_output = match self.kind {
                    DirectionalKind::Atr => Value::F64(atr),
                    DirectionalKind::Natr => {
                        if close != 0.0 {
                            Value::F64((atr / close) * 100.0)
                        } else {
                            Value::F64(0.0)
                        }
                    }
                    _ => unreachable!(),
                };
                return Ok(self.cached_output.clone());
            }

            self.tr_sum = self.tr_sum - (self.tr_sum / self.window as f64) + tr;
            let atr = self.tr_sum / self.window as f64;
            self.cached_output = match self.kind {
                DirectionalKind::Atr => Value::F64(atr),
                DirectionalKind::Natr => {
                    if close != 0.0 {
                        Value::F64((atr / close) * 100.0)
                    } else {
                        Value::F64(0.0)
                    }
                }
                _ => unreachable!(),
            };
            return Ok(self.cached_output.clone());
        }

        if self.seed_count < self.window - 1 {
            self.seed_count += 1;
            self.tr_sum += tr;
            self.plus_dm_sum += plus_dm;
            self.minus_dm_sum += minus_dm;
            self.cached_output = Value::NA;
            return Ok(Value::NA);
        }

        self.seed_count += 1;
        self.tr_sum = self.tr_sum - (self.tr_sum / self.window as f64) + tr;
        self.plus_dm_sum = self.plus_dm_sum - (self.plus_dm_sum / self.window as f64) + plus_dm;
        self.minus_dm_sum = self.minus_dm_sum - (self.minus_dm_sum / self.window as f64) + minus_dm;

        let dx = dx_value(self.tr_sum, self.plus_dm_sum, self.minus_dm_sum);
        self.cached_output = match self.kind {
            DirectionalKind::PlusDi => Value::F64(di_value(self.tr_sum, self.plus_dm_sum)),
            DirectionalKind::MinusDi => Value::F64(di_value(self.tr_sum, self.minus_dm_sum)),
            DirectionalKind::Dx => Value::F64(dx),
            DirectionalKind::Adx => self.update_adx(dx),
            DirectionalKind::Adxr => self.update_adxr(dx),
            _ => unreachable!(),
        };
        Ok(self.cached_output.clone())
    }

    fn update_adx(&mut self, dx: f64) -> Value {
        if self.dx_seed_count < self.window {
            self.dx_seed_count += 1;
            self.dx_sum += dx;
            if self.dx_seed_count < self.window {
                Value::NA
            } else {
                self.adx = self.dx_sum / self.window as f64;
                Value::F64(self.adx)
            }
        } else {
            self.adx = ((self.adx * (self.window as f64 - 1.0)) + dx) / self.window as f64;
            Value::F64(self.adx)
        }
    }

    fn update_adxr(&mut self, dx: f64) -> Value {
        let adx = match self.update_adx(dx) {
            Value::F64(value) => value,
            _ => return Value::NA,
        };
        self.adx_history.push_back(adx);
        if self.adx_history.len() < self.window {
            return Value::NA;
        }
        if self.adx_history.len() > self.window {
            self.adx_history.pop_front();
        }
        let lagged = *self.adx_history.front().expect("non-empty");
        Value::F64((adx + lagged) / 2.0)
    }

    fn compute_output(
        &self,
        close: f64,
        tr: f64,
        plus_dm: f64,
        minus_dm: f64,
        _dx: f64,
        instantaneous: bool,
    ) -> Value {
        match self.kind {
            DirectionalKind::Atr => Value::F64(tr),
            DirectionalKind::Natr => {
                if close != 0.0 {
                    Value::F64((tr / close) * 100.0)
                } else {
                    Value::F64(0.0)
                }
            }
            DirectionalKind::PlusDi => Value::F64(di_value(tr, plus_dm)),
            DirectionalKind::MinusDi => Value::F64(di_value(tr, minus_dm)),
            DirectionalKind::Dx => Value::F64(dx_value(tr, plus_dm, minus_dm)),
            DirectionalKind::Adx | DirectionalKind::Adxr => {
                if instantaneous {
                    Value::F64(dx_value(tr, plus_dm, minus_dm))
                } else {
                    Value::NA
                }
            }
        }
    }
}

fn directional_move(high: f64, low: f64, previous_high: f64, previous_low: f64) -> (f64, f64) {
    let diff_plus = high - previous_high;
    let diff_minus = previous_low - low;
    let plus_dm = if diff_plus > 0.0 && diff_plus > diff_minus {
        diff_plus
    } else {
        0.0
    };
    let minus_dm = if diff_minus > 0.0 && diff_minus > diff_plus {
        diff_minus
    } else {
        0.0
    };
    (plus_dm, minus_dm)
}

fn true_range(high: f64, low: f64, previous_close: f64) -> f64 {
    let mut value = high - low;
    let high_gap = (high - previous_close).abs();
    if high_gap > value {
        value = high_gap;
    }
    let low_gap = (low - previous_close).abs();
    if low_gap > value {
        value = low_gap;
    }
    value
}

fn di_value(tr: f64, dm: f64) -> f64 {
    if tr != 0.0 {
        100.0 * (dm / tr)
    } else {
        0.0
    }
}

fn dx_value(tr: f64, plus_dm: f64, minus_dm: f64) -> f64 {
    let plus_di = di_value(tr, plus_dm);
    let minus_di = di_value(tr, minus_dm);
    let sum = plus_di + minus_di;
    if sum != 0.0 {
        100.0 * ((plus_di - minus_di).abs() / sum)
    } else {
        0.0
    }
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

#[cfg(test)]
mod tests {
    use super::{DirectionalKind, DirectionalState, DmKind, DmState};
    use crate::types::Value;
    use crate::vm::SeriesBuffer;

    #[test]
    fn plus_dm_seeds_and_smooths() {
        let mut state = DmState::new(3, DmKind::Plus);
        let mut high = SeriesBuffer::new(8);
        let mut low = SeriesBuffer::new(8);
        for (h, l) in [(10.0, 9.0), (12.0, 10.0), (13.0, 11.0), (15.0, 12.0)] {
            high.push(Value::F64(h));
            low.push(Value::F64(l));
            let _ = state.update(&high, &low, 0).unwrap();
        }
        assert!(matches!(
            state.update(&high, &low, 0).unwrap(),
            Value::F64(_)
        ));
    }

    #[test]
    fn atr_returns_value_after_seed_window() {
        let mut state = DirectionalState::new(3, DirectionalKind::Atr);
        let mut high = SeriesBuffer::new(8);
        let mut low = SeriesBuffer::new(8);
        let mut close = SeriesBuffer::new(8);
        for (h, l, c) in [
            (10.0, 9.0, 9.5),
            (12.0, 10.0, 11.0),
            (13.0, 11.0, 12.0),
            (15.0, 12.0, 14.0),
        ] {
            high.push(Value::F64(h));
            low.push(Value::F64(l));
            close.push(Value::F64(c));
            let _ = state.update(&high, &low, &close, 0).unwrap();
        }
        assert!(matches!(
            state.update(&high, &low, &close, 0).unwrap(),
            Value::F64(_)
        ));
    }
}
