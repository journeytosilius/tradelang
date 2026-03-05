//! High-level runtime for executing compiled TradeLang programs over bars.
//!
//! This layer owns VM state across bars, including bounded series history,
//! indicator state, outputs, and execution limits.

use std::collections::{BTreeMap, HashMap};

use serde::{Deserialize, Serialize};

use crate::builtins::BuiltinId;
use crate::compiler::CompiledProgram;
use crate::diagnostic::RuntimeError;
use crate::indicators::IndicatorState;
use crate::interval::{Interval, MarketField, MarketSource};
use crate::output::{Outputs, PlotSeries};
use crate::types::{SlotKind, Value};
use crate::vm::{SeriesBuffer, Vm, VmEngine};

type SlotMap = [Option<u16>; 6];

const BASE_UPDATE_MASK: u32 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Bar {
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: f64,
    /// Candle open time in Unix milliseconds UTC.
    pub time: f64,
}

impl Bar {
    pub fn fields(self) -> [Value; 6] {
        [
            Value::F64(self.open),
            Value::F64(self.high),
            Value::F64(self.low),
            Value::F64(self.close),
            Value::F64(self.volume),
            Value::F64(self.time),
        ]
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct IntervalFeed {
    pub interval: Interval,
    pub bars: Vec<Bar>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MultiIntervalConfig {
    pub base_interval: Interval,
    pub supplemental: Vec<IntervalFeed>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct VmLimits {
    pub max_instructions_per_bar: usize,
    pub max_history_capacity: usize,
}

impl Default for VmLimits {
    fn default() -> Self {
        Self {
            max_instructions_per_bar: 10_000,
            max_history_capacity: 1_024,
        }
    }
}

struct FeedCursor {
    interval: Interval,
    bars: Vec<Bar>,
    next_index: usize,
    next_expected_open_time: Option<i64>,
    slot_map: SlotMap,
}

#[derive(Clone, Copy)]
enum FeedAction {
    Actual(Bar),
    Synthetic,
}

pub struct Engine {
    compiled: CompiledProgram,
    limits: VmLimits,
    current_values: Vec<Value>,
    series_values: Vec<SeriesBuffer>,
    indicator_state: HashMap<(BuiltinId, u16), IndicatorState>,
    outputs: Outputs,
    bar_index: usize,
    base_interval: Option<Interval>,
    base_slot_map: SlotMap,
    equal_interval_slot_map: SlotMap,
    feed_cursors: Vec<FeedCursor>,
    advanced_mask: u32,
}

impl Engine {
    pub fn new(compiled: CompiledProgram, limits: VmLimits) -> Self {
        Self::try_new(compiled, limits).expect("engine initialization should succeed")
    }

    pub fn try_new(compiled: CompiledProgram, limits: VmLimits) -> Result<Self, RuntimeError> {
        Self::build(compiled, None, limits)
    }

    pub fn new_multi_interval(
        compiled: CompiledProgram,
        config: MultiIntervalConfig,
        limits: VmLimits,
    ) -> Result<Self, RuntimeError> {
        Self::build(compiled, Some(config), limits)
    }

    fn build(
        compiled: CompiledProgram,
        config: Option<MultiIntervalConfig>,
        limits: VmLimits,
    ) -> Result<Self, RuntimeError> {
        for (slot, local) in compiled.program.locals.iter().enumerate() {
            if local.history_capacity > limits.max_history_capacity {
                return Err(RuntimeError::HistoryCapacityExceeded {
                    slot,
                    required: local.history_capacity,
                    limit: limits.max_history_capacity,
                });
            }
        }

        let local_count = compiled.program.locals.len();
        let current_values = vec![Value::NA; local_count];
        let series_values = compiled
            .program
            .locals
            .iter()
            .map(|local| {
                if matches!(local.kind, SlotKind::Series) {
                    SeriesBuffer::new(local.history_capacity.max(2))
                } else {
                    SeriesBuffer::new(1)
                }
            })
            .collect();
        let outputs = Outputs {
            plots: (0..compiled.program.plot_count)
                .map(|id| PlotSeries {
                    id,
                    name: None,
                    points: Vec::new(),
                })
                .collect(),
            alerts: Vec::new(),
        };

        let (base_interval, equal_interval_slot_map, feed_cursors) = match config {
            Some(config) => {
                let (equal_slots, cursors) = build_feed_cursors(&compiled, &config)?;
                (Some(config.base_interval), equal_slots, cursors)
            }
            None => (None, [None; 6], Vec::new()),
        };
        let base_slot_map = base_slot_map(&compiled);

        Ok(Self {
            compiled,
            limits,
            current_values,
            series_values,
            indicator_state: HashMap::new(),
            outputs,
            bar_index: 0,
            base_interval,
            base_slot_map,
            equal_interval_slot_map,
            feed_cursors,
            advanced_mask: 0,
        })
    }

    pub fn run_step(&mut self, bar: Bar) -> Result<crate::output::StepOutput, RuntimeError> {
        self.prepare_bar(bar)?;
        let mut remaining_steps = self.limits.max_instructions_per_bar;
        let program = &self.compiled.program;
        let mut vm_engine = VmEngine {
            program,
            bar_index: self.bar_index,
            current_bar: &bar,
            current_values: &mut self.current_values,
            series_values: &mut self.series_values,
            remaining_steps: &mut remaining_steps,
            indicator_state: &mut self.indicator_state,
            advanced_mask: self.advanced_mask,
        };
        let step = Vm::new(program).execute(&mut vm_engine)?;
        for point in &step.plots {
            if let Some(plot) = self.outputs.plots.get_mut(point.plot_id) {
                plot.points.push(point.clone());
            }
        }
        self.outputs.alerts.extend(step.alerts.clone());
        self.bar_index += 1;
        Ok(step)
    }

    pub fn finish(self) -> Outputs {
        self.outputs
    }

    fn prepare_bar(&mut self, bar: Bar) -> Result<(), RuntimeError> {
        self.advanced_mask = 0;
        for (slot, local) in self.compiled.program.locals.iter().enumerate() {
            if matches!(local.kind, SlotKind::Scalar) {
                self.current_values[slot] = Value::NA;
            }
        }

        if referenced_qualified_intervals(&self.compiled)
            .next()
            .is_some()
            && self.base_interval.is_none()
        {
            return Err(RuntimeError::MissingIntervalConfig);
        }

        let base_slot_map = self.base_slot_map;
        self.commit_bar(&base_slot_map, bar, BASE_UPDATE_MASK)?;

        if let Some(base_interval) = self.base_interval {
            let base_open = bar_open_time_ms(bar, base_interval)?;
            let base_close = base_interval.next_open_time(base_open).ok_or(
                RuntimeError::InvalidIntervalAlignment {
                    interval: base_interval,
                    open_time: base_open,
                },
            )?;
            let equal_interval_slot_map = self.equal_interval_slot_map;
            self.commit_bar(&equal_interval_slot_map, bar, base_interval.mask())?;
            for index in 0..self.feed_cursors.len() {
                self.advance_feed(index, base_close)?;
            }
        }

        Ok(())
    }

    fn advance_feed(&mut self, index: usize, base_close_time: i64) -> Result<(), RuntimeError> {
        loop {
            let Some((interval, slot_map, action)) = self.feed_action(index, base_close_time)?
            else {
                break;
            };
            match action {
                FeedAction::Actual(bar) => self.commit_bar(&slot_map, bar, interval.mask())?,
                FeedAction::Synthetic => {
                    self.commit_values(&slot_map, synthetic_values(), interval.mask())?;
                }
            }
        }
        Ok(())
    }

    fn feed_action(
        &mut self,
        index: usize,
        base_close_time: i64,
    ) -> Result<Option<(Interval, SlotMap, FeedAction)>, RuntimeError> {
        let cursor = &mut self.feed_cursors[index];
        let Some(expected_open) = cursor.next_expected_open_time else {
            return Ok(None);
        };
        let Some(expected_close) = cursor.interval.next_open_time(expected_open) else {
            return Ok(None);
        };
        if expected_close > base_close_time {
            return Ok(None);
        }

        let action = match cursor.bars.get(cursor.next_index).copied() {
            Some(bar) if bar_open_time_ms(bar, cursor.interval)? == expected_open => {
                cursor.next_index += 1;
                FeedAction::Actual(bar)
            }
            Some(bar) if bar_open_time_ms(bar, cursor.interval)? < expected_open => {
                return Err(RuntimeError::UnsortedIntervalFeed {
                    interval: cursor.interval,
                    open_time: bar_open_time_ms(bar, cursor.interval)?,
                });
            }
            _ => FeedAction::Synthetic,
        };
        cursor.next_expected_open_time = cursor.interval.next_open_time(expected_open);
        Ok(Some((cursor.interval, cursor.slot_map, action)))
    }

    fn commit_bar(&mut self, slot_map: &SlotMap, bar: Bar, mask: u32) -> Result<(), RuntimeError> {
        self.commit_values(slot_map, bar.fields(), mask)
    }

    fn commit_values(
        &mut self,
        slot_map: &SlotMap,
        values: [Value; 6],
        mask: u32,
    ) -> Result<(), RuntimeError> {
        let mut committed = false;
        for (index, slot) in slot_map.iter().enumerate() {
            let Some(slot) = slot else {
                continue;
            };
            committed = true;
            self.current_values[*slot as usize] = values[index].clone();
            self.series_values
                .get_mut(*slot as usize)
                .ok_or(RuntimeError::InvalidSeriesSlot {
                    slot: *slot as usize,
                })?
                .push(values[index].clone());
        }
        if committed {
            self.advanced_mask |= mask;
        }
        Ok(())
    }
}

pub fn run(
    compiled: &CompiledProgram,
    bars: &[Bar],
    limits: VmLimits,
) -> Result<Outputs, RuntimeError> {
    let mut engine = Engine::try_new(compiled.clone(), limits)?;
    for &bar in bars {
        engine.run_step(bar)?;
    }
    Ok(engine.finish())
}

pub fn run_multi_interval(
    compiled: &CompiledProgram,
    base_bars: &[Bar],
    config: MultiIntervalConfig,
    limits: VmLimits,
) -> Result<Outputs, RuntimeError> {
    let mut engine = Engine::new_multi_interval(compiled.clone(), config, limits)?;
    for &bar in base_bars {
        engine.run_step(bar)?;
    }
    Ok(engine.finish())
}

fn build_feed_cursors(
    compiled: &CompiledProgram,
    config: &MultiIntervalConfig,
) -> Result<(SlotMap, Vec<FeedCursor>), RuntimeError> {
    let base_interval = config.base_interval;
    let mut referenced = BTreeMap::<Interval, SlotMap>::new();
    let mut equal_slot_map = [None; 6];

    for local in &compiled.program.locals {
        let Some(binding) = local.market_binding else {
            continue;
        };
        let MarketSource::Qualified(interval) = binding.source else {
            continue;
        };
        if interval < base_interval {
            return Err(RuntimeError::LowerIntervalReference {
                base: base_interval,
                referenced: interval,
            });
        }
        if interval == base_interval {
            equal_slot_map[field_index(binding.field)] = Some(slot_for_local(compiled, local)?);
        } else {
            referenced.entry(interval).or_insert([None; 6])[field_index(binding.field)] =
                Some(slot_for_local(compiled, local)?);
        }
    }

    let mut feeds = BTreeMap::<Interval, Vec<Bar>>::new();
    for feed in &config.supplemental {
        if feed.interval <= base_interval {
            return Err(RuntimeError::UnexpectedIntervalFeed {
                interval: feed.interval,
            });
        }
        if !referenced.contains_key(&feed.interval) {
            return Err(RuntimeError::UnexpectedIntervalFeed {
                interval: feed.interval,
            });
        }
        if feeds.insert(feed.interval, feed.bars.clone()).is_some() {
            return Err(RuntimeError::DuplicateIntervalFeed {
                interval: feed.interval,
            });
        }
    }

    let mut cursors = Vec::new();
    for (interval, slot_map) in referenced {
        let bars = feeds
            .remove(&interval)
            .ok_or(RuntimeError::MissingIntervalFeed { interval })?;
        validate_feed(interval, &bars)?;
        let next_expected_open_time = bars
            .first()
            .map(|bar| bar_open_time_ms(*bar, interval))
            .transpose()?;
        cursors.push(FeedCursor {
            interval,
            bars,
            next_index: 0,
            next_expected_open_time,
            slot_map,
        });
    }

    Ok((equal_slot_map, cursors))
}

fn validate_feed(interval: Interval, bars: &[Bar]) -> Result<(), RuntimeError> {
    let mut previous = None;
    for &bar in bars {
        let open_time = bar_open_time_ms(bar, interval)?;
        if !interval.is_aligned(open_time) {
            return Err(RuntimeError::InvalidIntervalAlignment {
                interval,
                open_time,
            });
        }
        if let Some(prev) = previous {
            if open_time == prev {
                return Err(RuntimeError::DuplicateIntervalBar {
                    interval,
                    open_time,
                });
            }
            if open_time < prev {
                return Err(RuntimeError::UnsortedIntervalFeed {
                    interval,
                    open_time,
                });
            }
        }
        previous = Some(open_time);
    }
    Ok(())
}

fn bar_open_time_ms(bar: Bar, interval: Interval) -> Result<i64, RuntimeError> {
    if !bar.time.is_finite() || bar.time.fract() != 0.0 {
        return Err(RuntimeError::InvalidIntervalAlignment {
            interval,
            open_time: bar.time as i64,
        });
    }
    let open_time = bar.time as i64;
    if !interval.is_aligned(open_time) {
        return Err(RuntimeError::InvalidIntervalAlignment {
            interval,
            open_time,
        });
    }
    Ok(open_time)
}

fn base_slot_map(compiled: &CompiledProgram) -> SlotMap {
    let mut map = [None; 6];
    for (slot, local) in compiled.program.locals.iter().enumerate() {
        let Some(binding) = local.market_binding else {
            continue;
        };
        if matches!(binding.source, MarketSource::Base) {
            map[field_index(binding.field)] = Some(slot as u16);
        }
    }
    map
}

fn referenced_qualified_intervals(
    compiled: &CompiledProgram,
) -> impl Iterator<Item = Interval> + '_ {
    compiled.program.locals.iter().filter_map(|local| {
        let binding = local.market_binding?;
        match binding.source {
            MarketSource::Qualified(interval) => Some(interval),
            MarketSource::Base => None,
        }
    })
}

fn slot_for_local(
    compiled: &CompiledProgram,
    local: &crate::bytecode::LocalInfo,
) -> Result<u16, RuntimeError> {
    compiled
        .program
        .locals
        .iter()
        .position(|candidate| std::ptr::eq(candidate, local))
        .map(|slot| slot as u16)
        .ok_or(RuntimeError::InvalidLocalSlot { slot: usize::MAX })
}

fn field_index(field: MarketField) -> usize {
    field.ordinal() as usize
}

fn synthetic_values() -> [Value; 6] {
    [
        Value::NA,
        Value::NA,
        Value::NA,
        Value::NA,
        Value::NA,
        Value::NA,
    ]
}
