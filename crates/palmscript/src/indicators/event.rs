//! Event-memory helper state.

use std::collections::VecDeque;

use crate::diagnostic::RuntimeError;
use crate::types::Value;
use crate::vm::SeriesBuffer;

#[derive(Clone, Debug)]
pub(crate) struct BarsSinceState {
    seen_true: bool,
    bars_since: usize,
    last_seen_version: u64,
    cached_output: Value,
}

#[derive(Clone, Debug)]
pub(crate) struct ValueWhenState {
    occurrence: usize,
    last_seen_version: u64,
    last_source_version: u64,
    cached_output: Value,
    matches: VecDeque<Value>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AnchoredExtremaMode {
    Highest,
    Lowest,
}

#[derive(Clone, Debug)]
pub(crate) struct AnchoredExtremaState {
    mode: AnchoredExtremaMode,
    active: bool,
    bars_in_epoch: usize,
    extrema_offset: usize,
    extrema_value: Option<f64>,
    last_anchor_version: u64,
    last_source_version: u64,
    cached_output: Value,
}

#[derive(Clone, Debug)]
pub(crate) struct AnchoredValueWhenState {
    occurrence: usize,
    active: bool,
    last_anchor_version: u64,
    last_condition_version: u64,
    last_source_version: u64,
    cached_output: Value,
    matches: VecDeque<Value>,
}

#[derive(Clone, Debug)]
pub(crate) struct AnchoredCountState {
    active: bool,
    count: usize,
    last_anchor_version: u64,
    last_condition_version: u64,
    cached_output: Value,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BoolEdgeMode {
    Activated,
    Deactivated,
}

#[derive(Clone, Debug)]
pub(crate) struct BoolEdgeState {
    mode: BoolEdgeMode,
    last_seen_version: u64,
    cached_output: Value,
}

#[derive(Clone, Debug)]
pub(crate) struct PersistentState {
    active: bool,
    last_enter_version: u64,
    last_exit_version: u64,
    cached_output: Value,
}

#[derive(Clone, Debug)]
pub(crate) struct CumState {
    total: f64,
    initialized: bool,
    cached_output: Value,
}

impl BoolEdgeState {
    pub(crate) fn new(mode: BoolEdgeMode) -> Self {
        Self {
            mode,
            last_seen_version: 0,
            cached_output: Value::Bool(false),
        }
    }

    pub(crate) fn update(
        &mut self,
        condition: &SeriesBuffer,
        pc: usize,
    ) -> Result<Value, RuntimeError> {
        if condition.version() == self.last_seen_version {
            return Ok(self.cached_output.clone());
        }
        self.last_seen_version = condition.version();
        let current = match condition.get(0) {
            Value::Bool(value) => Some(value),
            Value::NA => None,
            other => {
                return Err(RuntimeError::TypeMismatch {
                    pc,
                    expected: "bool",
                    found: other.type_name(),
                });
            }
        };
        let previous = match condition.get(1) {
            Value::Bool(value) => Some(value),
            Value::NA => None,
            other => {
                return Err(RuntimeError::TypeMismatch {
                    pc,
                    expected: "bool",
                    found: other.type_name(),
                });
            }
        };
        let edge = match self.mode {
            BoolEdgeMode::Activated => current == Some(true) && previous != Some(true),
            BoolEdgeMode::Deactivated => current == Some(false) && previous == Some(true),
        };
        self.cached_output = Value::Bool(edge);
        Ok(self.cached_output.clone())
    }
}

impl BarsSinceState {
    pub(crate) fn new() -> Self {
        Self {
            seen_true: false,
            bars_since: 0,
            last_seen_version: 0,
            cached_output: Value::NA,
        }
    }

    pub(crate) fn update(
        &mut self,
        condition: &SeriesBuffer,
        pc: usize,
    ) -> Result<Value, RuntimeError> {
        if condition.version() == self.last_seen_version {
            return Ok(self.cached_output.clone());
        }
        self.last_seen_version = condition.version();
        match condition.get(0) {
            Value::Bool(true) => {
                self.seen_true = true;
                self.bars_since = 0;
                self.cached_output = Value::F64(0.0);
            }
            Value::Bool(false) => {
                if self.seen_true {
                    self.bars_since += 1;
                    self.cached_output = Value::F64(self.bars_since as f64);
                } else {
                    self.cached_output = Value::NA;
                }
            }
            Value::NA => {
                if self.seen_true {
                    self.bars_since += 1;
                }
                self.cached_output = Value::NA;
            }
            other => {
                return Err(RuntimeError::TypeMismatch {
                    pc,
                    expected: "bool",
                    found: other.type_name(),
                });
            }
        }
        Ok(self.cached_output.clone())
    }
}

impl PersistentState {
    pub(crate) fn new() -> Self {
        Self {
            active: false,
            last_enter_version: 0,
            last_exit_version: 0,
            cached_output: Value::Bool(false),
        }
    }

    pub(crate) fn update(
        &mut self,
        enter: &SeriesBuffer,
        exit: &SeriesBuffer,
        pc: usize,
    ) -> Result<Value, RuntimeError> {
        if enter.version() == self.last_enter_version && exit.version() == self.last_exit_version {
            return Ok(self.cached_output.clone());
        }
        self.last_enter_version = enter.version();
        self.last_exit_version = exit.version();

        let enter_now = match enter.get(0) {
            Value::Bool(value) => value,
            Value::NA => false,
            other => {
                return Err(RuntimeError::TypeMismatch {
                    pc,
                    expected: "bool",
                    found: other.type_name(),
                });
            }
        };
        let exit_now = match exit.get(0) {
            Value::Bool(value) => value,
            Value::NA => false,
            other => {
                return Err(RuntimeError::TypeMismatch {
                    pc,
                    expected: "bool",
                    found: other.type_name(),
                });
            }
        };

        match (enter_now, exit_now) {
            (true, false) => self.active = true,
            (false, true) => self.active = false,
            _ => {}
        }

        self.cached_output = Value::Bool(self.active);
        Ok(self.cached_output.clone())
    }
}

impl ValueWhenState {
    pub(crate) fn new(occurrence: usize) -> Self {
        Self {
            occurrence,
            last_seen_version: 0,
            last_source_version: 0,
            cached_output: Value::NA,
            matches: VecDeque::with_capacity(occurrence + 1),
        }
    }

    pub(crate) fn update(
        &mut self,
        condition: &SeriesBuffer,
        source: &SeriesBuffer,
        pc: usize,
    ) -> Result<Value, RuntimeError> {
        if condition.version() == self.last_seen_version
            && source.version() == self.last_source_version
        {
            return Ok(self.cached_output.clone());
        }
        self.last_seen_version = condition.version();
        self.last_source_version = source.version();

        match condition.get(0) {
            Value::Bool(true) => {
                let current = source.get(0);
                match current {
                    Value::F64(_) | Value::Bool(_) | Value::NA => {
                        if self.matches.len() == self.occurrence + 1 {
                            self.matches.pop_front();
                        }
                        self.matches.push_back(current);
                        self.cached_output = self.lookup_occurrence();
                    }
                    other => {
                        return Err(RuntimeError::TypeMismatch {
                            pc,
                            expected: "f64-or-bool",
                            found: other.type_name(),
                        });
                    }
                }
            }
            Value::Bool(false) => {
                self.cached_output = self.lookup_occurrence();
            }
            Value::NA => {
                self.cached_output = Value::NA;
            }
            other => {
                return Err(RuntimeError::TypeMismatch {
                    pc,
                    expected: "bool",
                    found: other.type_name(),
                });
            }
        }

        Ok(self.cached_output.clone())
    }

    fn lookup_occurrence(&self) -> Value {
        if self.matches.len() <= self.occurrence {
            Value::NA
        } else {
            self.matches[self.matches.len() - 1 - self.occurrence].clone()
        }
    }
}

impl AnchoredExtremaState {
    pub(crate) fn new(mode: AnchoredExtremaMode) -> Self {
        Self {
            mode,
            active: false,
            bars_in_epoch: 0,
            extrema_offset: 0,
            extrema_value: None,
            last_anchor_version: 0,
            last_source_version: 0,
            cached_output: Value::NA,
        }
    }

    pub(crate) fn update_value(
        &mut self,
        anchor: &SeriesBuffer,
        source: &SeriesBuffer,
        pc: usize,
    ) -> Result<Value, RuntimeError> {
        self.update(anchor, source, pc, false)
    }

    pub(crate) fn update_offset(
        &mut self,
        anchor: &SeriesBuffer,
        source: &SeriesBuffer,
        pc: usize,
    ) -> Result<Value, RuntimeError> {
        self.update(anchor, source, pc, true)
    }

    fn update(
        &mut self,
        anchor: &SeriesBuffer,
        source: &SeriesBuffer,
        pc: usize,
        return_offset: bool,
    ) -> Result<Value, RuntimeError> {
        if anchor.version() == self.last_anchor_version
            && source.version() == self.last_source_version
        {
            return Ok(self.cached_output.clone());
        }
        self.last_anchor_version = anchor.version();
        self.last_source_version = source.version();

        let anchor_now = anchor.get(0);
        match anchor_now {
            Value::Bool(true) => {
                self.active = true;
                self.bars_in_epoch = 0;
                self.extrema_offset = 0;
                self.extrema_value = None;
            }
            Value::Bool(false) => {
                if self.active {
                    self.bars_in_epoch += 1;
                }
            }
            Value::NA => {
                self.cached_output = Value::NA;
                return Ok(self.cached_output.clone());
            }
            other => {
                return Err(RuntimeError::TypeMismatch {
                    pc,
                    expected: "bool",
                    found: other.type_name(),
                });
            }
        }

        if !self.active {
            self.cached_output = Value::NA;
            return Ok(self.cached_output.clone());
        }

        match source.get(0) {
            Value::F64(value) => {
                let replace = match self.extrema_value {
                    None => true,
                    Some(best) => match self.mode {
                        AnchoredExtremaMode::Highest => value >= best,
                        AnchoredExtremaMode::Lowest => value <= best,
                    },
                };
                if replace {
                    self.extrema_value = Some(value);
                    self.extrema_offset = self.bars_in_epoch;
                }
                self.cached_output = if return_offset {
                    Value::F64((self.bars_in_epoch - self.extrema_offset) as f64)
                } else {
                    Value::F64(self.extrema_value.expect("extrema value should exist"))
                };
            }
            Value::NA => {
                self.cached_output = Value::NA;
            }
            other => {
                return Err(RuntimeError::TypeMismatch {
                    pc,
                    expected: "f64",
                    found: other.type_name(),
                });
            }
        }
        Ok(self.cached_output.clone())
    }
}

impl AnchoredValueWhenState {
    pub(crate) fn new(occurrence: usize) -> Self {
        Self {
            occurrence,
            active: false,
            last_anchor_version: 0,
            last_condition_version: 0,
            last_source_version: 0,
            cached_output: Value::NA,
            matches: VecDeque::with_capacity(occurrence + 1),
        }
    }

    pub(crate) fn update(
        &mut self,
        anchor: &SeriesBuffer,
        condition: &SeriesBuffer,
        source: &SeriesBuffer,
        pc: usize,
    ) -> Result<Value, RuntimeError> {
        if anchor.version() == self.last_anchor_version
            && condition.version() == self.last_condition_version
            && source.version() == self.last_source_version
        {
            return Ok(self.cached_output.clone());
        }
        self.last_anchor_version = anchor.version();
        self.last_condition_version = condition.version();
        self.last_source_version = source.version();

        match anchor.get(0) {
            Value::Bool(true) => {
                self.active = true;
                self.matches.clear();
            }
            Value::Bool(false) => {}
            Value::NA => {
                self.cached_output = Value::NA;
                return Ok(self.cached_output.clone());
            }
            other => {
                return Err(RuntimeError::TypeMismatch {
                    pc,
                    expected: "bool",
                    found: other.type_name(),
                });
            }
        }

        if !self.active {
            self.cached_output = Value::NA;
            return Ok(self.cached_output.clone());
        }

        match condition.get(0) {
            Value::Bool(true) => {
                let current = source.get(0);
                match current {
                    Value::F64(_) | Value::Bool(_) | Value::NA => {
                        if self.matches.len() == self.occurrence + 1 {
                            self.matches.pop_front();
                        }
                        self.matches.push_back(current);
                        self.cached_output = self.lookup_occurrence();
                    }
                    other => {
                        return Err(RuntimeError::TypeMismatch {
                            pc,
                            expected: "f64-or-bool",
                            found: other.type_name(),
                        });
                    }
                }
            }
            Value::Bool(false) => {
                self.cached_output = self.lookup_occurrence();
            }
            Value::NA => {
                self.cached_output = Value::NA;
            }
            other => {
                return Err(RuntimeError::TypeMismatch {
                    pc,
                    expected: "bool",
                    found: other.type_name(),
                });
            }
        }

        Ok(self.cached_output.clone())
    }

    fn lookup_occurrence(&self) -> Value {
        if self.matches.len() <= self.occurrence {
            Value::NA
        } else {
            self.matches[self.matches.len() - 1 - self.occurrence].clone()
        }
    }
}

impl AnchoredCountState {
    pub(crate) fn new() -> Self {
        Self {
            active: false,
            count: 0,
            last_anchor_version: 0,
            last_condition_version: 0,
            cached_output: Value::NA,
        }
    }

    pub(crate) fn update(
        &mut self,
        anchor: &SeriesBuffer,
        condition: &SeriesBuffer,
        pc: usize,
    ) -> Result<Value, RuntimeError> {
        if anchor.version() == self.last_anchor_version
            && condition.version() == self.last_condition_version
        {
            return Ok(self.cached_output.clone());
        }
        self.last_anchor_version = anchor.version();
        self.last_condition_version = condition.version();

        match anchor.get(0) {
            Value::Bool(true) => {
                self.active = true;
                self.count = 0;
            }
            Value::Bool(false) => {}
            Value::NA => {
                self.cached_output = Value::NA;
                return Ok(self.cached_output.clone());
            }
            other => {
                return Err(RuntimeError::TypeMismatch {
                    pc,
                    expected: "bool",
                    found: other.type_name(),
                });
            }
        }

        if !self.active {
            self.cached_output = Value::NA;
            return Ok(self.cached_output.clone());
        }

        match condition.get(0) {
            Value::Bool(true) => {
                self.count += 1;
                self.cached_output = Value::F64(self.count as f64);
            }
            Value::Bool(false) => {
                self.cached_output = Value::F64(self.count as f64);
            }
            Value::NA => {
                self.cached_output = Value::NA;
            }
            other => {
                return Err(RuntimeError::TypeMismatch {
                    pc,
                    expected: "bool",
                    found: other.type_name(),
                });
            }
        }

        Ok(self.cached_output.clone())
    }
}

impl CumState {
    pub(crate) fn new() -> Self {
        Self {
            total: 0.0,
            initialized: false,
            cached_output: Value::NA,
        }
    }

    pub(crate) fn update(&mut self, value: Value, pc: usize) -> Result<Value, RuntimeError> {
        match value {
            Value::F64(value) => {
                self.total += value;
                self.initialized = true;
                self.cached_output = Value::F64(self.total);
            }
            Value::NA => {
                self.cached_output = Value::NA;
            }
            other => {
                return Err(RuntimeError::TypeMismatch {
                    pc,
                    expected: "f64",
                    found: other.type_name(),
                });
            }
        }
        if self.initialized {
            Ok(self.cached_output.clone())
        } else {
            Ok(Value::NA)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        AnchoredCountState, AnchoredExtremaMode, AnchoredExtremaState, AnchoredValueWhenState,
        BarsSinceState, BoolEdgeMode, BoolEdgeState, CumState, ValueWhenState,
    };
    use crate::types::Value;
    use crate::vm::SeriesBuffer;

    #[test]
    fn barssince_tracks_true_events() {
        let mut state = BarsSinceState::new();
        let mut cond = SeriesBuffer::new(8);

        cond.push(Value::Bool(false));
        assert_eq!(state.update(&cond, 0).unwrap(), Value::NA);
        cond.push(Value::Bool(true));
        assert_eq!(state.update(&cond, 0).unwrap(), Value::F64(0.0));
        cond.push(Value::Bool(false));
        assert_eq!(state.update(&cond, 0).unwrap(), Value::F64(1.0));
    }

    #[test]
    fn activated_marks_false_to_true_and_initial_true_edges() {
        let mut state = BoolEdgeState::new(BoolEdgeMode::Activated);
        let mut cond = SeriesBuffer::new(8);

        cond.push(Value::NA);
        assert_eq!(state.update(&cond, 0).unwrap(), Value::Bool(false));

        cond.push(Value::Bool(true));
        assert_eq!(state.update(&cond, 0).unwrap(), Value::Bool(true));

        cond.push(Value::Bool(true));
        assert_eq!(state.update(&cond, 0).unwrap(), Value::Bool(false));

        cond.push(Value::Bool(false));
        assert_eq!(state.update(&cond, 0).unwrap(), Value::Bool(false));

        cond.push(Value::Bool(true));
        assert_eq!(state.update(&cond, 0).unwrap(), Value::Bool(true));
    }

    #[test]
    fn deactivated_marks_true_to_false_edges_only() {
        let mut state = BoolEdgeState::new(BoolEdgeMode::Deactivated);
        let mut cond = SeriesBuffer::new(8);

        cond.push(Value::Bool(true));
        assert_eq!(state.update(&cond, 0).unwrap(), Value::Bool(false));

        cond.push(Value::Bool(false));
        assert_eq!(state.update(&cond, 0).unwrap(), Value::Bool(true));

        cond.push(Value::Bool(false));
        assert_eq!(state.update(&cond, 0).unwrap(), Value::Bool(false));

        cond.push(Value::NA);
        assert_eq!(state.update(&cond, 0).unwrap(), Value::Bool(false));
    }

    #[test]
    fn valuewhen_returns_recent_matches_by_occurrence() {
        let mut state = ValueWhenState::new(1);
        let mut cond = SeriesBuffer::new(8);
        let mut source = SeriesBuffer::new(8);

        source.push(Value::F64(1.0));
        cond.push(Value::Bool(true));
        assert_eq!(state.update(&cond, &source, 0).unwrap(), Value::NA);

        source.push(Value::F64(2.0));
        cond.push(Value::Bool(false));
        assert_eq!(state.update(&cond, &source, 0).unwrap(), Value::NA);

        source.push(Value::F64(3.0));
        cond.push(Value::Bool(true));
        assert_eq!(state.update(&cond, &source, 0).unwrap(), Value::F64(1.0));
    }

    #[test]
    fn valuewhen_propagates_na_condition_without_erasing_history() {
        let mut state = ValueWhenState::new(0);
        let mut cond = SeriesBuffer::new(8);
        let mut source = SeriesBuffer::new(8);

        source.push(Value::Bool(true));
        cond.push(Value::Bool(true));
        assert_eq!(state.update(&cond, &source, 0).unwrap(), Value::Bool(true));

        source.push(Value::Bool(false));
        cond.push(Value::NA);
        assert_eq!(state.update(&cond, &source, 0).unwrap(), Value::NA);

        source.push(Value::Bool(false));
        cond.push(Value::Bool(false));
        assert_eq!(state.update(&cond, &source, 0).unwrap(), Value::Bool(true));
    }

    #[test]
    fn cum_accumulates_numeric_values_and_skips_state_reset_on_na() {
        let mut state = CumState::new();
        assert_eq!(state.update(Value::F64(2.0), 0).unwrap(), Value::F64(2.0));
        assert_eq!(state.update(Value::NA, 0).unwrap(), Value::NA);
        assert_eq!(state.update(Value::F64(3.0), 0).unwrap(), Value::F64(5.0));
    }

    #[test]
    fn anchored_extrema_resets_on_anchor_and_includes_anchor_bar() {
        let mut state = AnchoredExtremaState::new(AnchoredExtremaMode::Highest);
        let mut anchor = SeriesBuffer::new(8);
        let mut source = SeriesBuffer::new(8);

        anchor.push(Value::Bool(false));
        source.push(Value::F64(1.0));
        assert_eq!(state.update_value(&anchor, &source, 0).unwrap(), Value::NA);

        anchor.push(Value::Bool(true));
        source.push(Value::F64(2.0));
        assert_eq!(
            state.update_value(&anchor, &source, 0).unwrap(),
            Value::F64(2.0)
        );

        anchor.push(Value::Bool(false));
        source.push(Value::F64(1.5));
        assert_eq!(
            state.update_value(&anchor, &source, 0).unwrap(),
            Value::F64(2.0)
        );

        anchor.push(Value::Bool(false));
        source.push(Value::F64(3.0));
        assert_eq!(
            state.update_offset(&anchor, &source, 0).unwrap(),
            Value::F64(0.0)
        );
    }

    #[test]
    fn anchored_valuewhen_forgets_pre_anchor_history() {
        let mut state = AnchoredValueWhenState::new(0);
        let mut anchor = SeriesBuffer::new(8);
        let mut cond = SeriesBuffer::new(8);
        let mut source = SeriesBuffer::new(8);

        anchor.push(Value::Bool(false));
        cond.push(Value::Bool(true));
        source.push(Value::F64(1.0));
        assert_eq!(state.update(&anchor, &cond, &source, 0).unwrap(), Value::NA);

        anchor.push(Value::Bool(true));
        cond.push(Value::Bool(false));
        source.push(Value::F64(2.0));
        assert_eq!(state.update(&anchor, &cond, &source, 0).unwrap(), Value::NA);

        anchor.push(Value::Bool(false));
        cond.push(Value::Bool(true));
        source.push(Value::F64(3.0));
        assert_eq!(
            state.update(&anchor, &cond, &source, 0).unwrap(),
            Value::F64(3.0)
        );
    }

    #[test]
    fn anchored_count_resets_on_anchor_and_includes_anchor_bar() {
        let mut state = AnchoredCountState::new();
        let mut anchor = SeriesBuffer::new(8);
        let mut condition = SeriesBuffer::new(8);

        anchor.push(Value::Bool(false));
        condition.push(Value::Bool(true));
        assert_eq!(state.update(&anchor, &condition, 0).unwrap(), Value::NA);

        anchor.push(Value::Bool(true));
        condition.push(Value::Bool(true));
        assert_eq!(
            state.update(&anchor, &condition, 0).unwrap(),
            Value::F64(1.0)
        );

        anchor.push(Value::Bool(false));
        condition.push(Value::Bool(false));
        assert_eq!(
            state.update(&anchor, &condition, 0).unwrap(),
            Value::F64(1.0)
        );

        anchor.push(Value::Bool(false));
        condition.push(Value::Bool(true));
        assert_eq!(
            state.update(&anchor, &condition, 0).unwrap(),
            Value::F64(2.0)
        );

        anchor.push(Value::Bool(true));
        condition.push(Value::Bool(false));
        assert_eq!(
            state.update(&anchor, &condition, 0).unwrap(),
            Value::F64(0.0)
        );
    }

    #[test]
    fn anchored_count_propagates_na_without_erasing_count() {
        let mut state = AnchoredCountState::new();
        let mut anchor = SeriesBuffer::new(8);
        let mut condition = SeriesBuffer::new(8);

        anchor.push(Value::Bool(true));
        condition.push(Value::Bool(true));
        assert_eq!(
            state.update(&anchor, &condition, 0).unwrap(),
            Value::F64(1.0)
        );

        anchor.push(Value::Bool(false));
        condition.push(Value::NA);
        assert_eq!(state.update(&anchor, &condition, 0).unwrap(), Value::NA);

        anchor.push(Value::Bool(false));
        condition.push(Value::Bool(false));
        assert_eq!(
            state.update(&anchor, &condition, 0).unwrap(),
            Value::F64(1.0)
        );
    }
}
