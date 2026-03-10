//! High-level runtime for executing compiled PalmScript programs over bars.
//!
//! This layer owns VM state across bars, including bounded series history,
//! indicator state, outputs, and execution limits.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use serde::{Deserialize, Serialize};

use crate::builtins::BuiltinId;
use crate::bytecode::OutputKind;
use crate::compiler::CompiledProgram;
use crate::diagnostic::RuntimeError;
use crate::indicators::IndicatorState;
use crate::interval::{Interval, MarketField, MarketSource};
use crate::output::{
    OrderFieldSample, OrderFieldSeries, OutputSample, OutputSeries, OutputValue, Outputs,
    PlotSeries, TriggerEvent,
};
use crate::types::{SlotKind, Value};
use crate::vm::{SeriesBuffer, Vm, VmEngine};

type SlotMap = [Option<u16>; 6];
type OutputCollections = (
    Vec<OutputSample>,
    Vec<OutputSample>,
    Vec<OrderFieldSample>,
    Vec<TriggerEvent>,
);

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
pub struct SourceFeed {
    pub source_id: u16,
    pub interval: Interval,
    pub bars: Vec<Bar>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SourceRuntimeConfig {
    pub base_interval: Interval,
    pub feeds: Vec<SourceFeed>,
}

pub fn slice_runtime_window(
    runtime: &SourceRuntimeConfig,
    from_ms: i64,
    to_ms: i64,
) -> SourceRuntimeConfig {
    SourceRuntimeConfig {
        base_interval: runtime.base_interval,
        feeds: runtime
            .feeds
            .iter()
            .map(|feed| SourceFeed {
                source_id: feed.source_id,
                interval: feed.interval,
                bars: feed
                    .bars
                    .iter()
                    .copied()
                    .filter(|bar| {
                        let time = bar.time as i64;
                        time >= from_ms && time < to_ms
                    })
                    .collect(),
            })
            .collect(),
    }
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

struct SourceBaseCursor {
    bars: Vec<Bar>,
    next_index: usize,
    base_slot_map: SlotMap,
    equal_interval_slot_map: SlotMap,
}

struct SourceFeedCursor {
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
    advanced_mask: u32,
}

pub struct RuntimeStep {
    pub open_time: i64,
    pub bar: Bar,
    pub output: crate::output::StepOutput,
}

pub struct RuntimeStepper {
    engine: Engine,
    timeline: Vec<i64>,
    next_index: usize,
    base_interval: Interval,
    base_cursors: Vec<SourceBaseCursor>,
    supplemental_cursors: Vec<SourceFeedCursor>,
}

impl Engine {
    pub fn new(compiled: CompiledProgram, limits: VmLimits) -> Self {
        Self::try_new(compiled, limits).expect("engine initialization should succeed")
    }

    pub fn try_new(compiled: CompiledProgram, limits: VmLimits) -> Result<Self, RuntimeError> {
        Self::build(compiled, limits)
    }

    fn build(compiled: CompiledProgram, limits: VmLimits) -> Result<Self, RuntimeError> {
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
            exports: compiled
                .program
                .outputs
                .iter()
                .enumerate()
                .filter(|(_, decl)| matches!(decl.kind, OutputKind::ExportSeries))
                .map(|(id, decl)| OutputSeries {
                    id,
                    name: decl.name.clone(),
                    kind: decl.kind,
                    points: Vec::new(),
                })
                .collect(),
            triggers: compiled
                .program
                .outputs
                .iter()
                .enumerate()
                .filter(|(_, decl)| matches!(decl.kind, OutputKind::Trigger))
                .map(|(id, decl)| OutputSeries {
                    id,
                    name: decl.name.clone(),
                    kind: decl.kind,
                    points: Vec::new(),
                })
                .collect(),
            order_fields: compiled
                .program
                .order_fields
                .iter()
                .enumerate()
                .map(|(id, decl)| OrderFieldSeries {
                    id,
                    name: decl.name.clone(),
                    role: decl.role,
                    kind: decl.kind,
                    points: Vec::new(),
                })
                .collect(),
            trigger_events: Vec::new(),
            alerts: Vec::new(),
        };

        Ok(Self {
            compiled,
            limits,
            current_values,
            series_values,
            indicator_state: HashMap::new(),
            outputs,
            bar_index: 0,
            advanced_mask: 0,
        })
    }

    fn execute_prepared_step(
        &mut self,
        bar: Bar,
    ) -> Result<crate::output::StepOutput, RuntimeError> {
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
        let mut step = Vm::new(program).execute(&mut vm_engine)?;
        let (exports, triggers, order_fields, trigger_events) = self.collect_outputs(bar)?;
        step.exports = exports;
        step.triggers = triggers;
        step.order_fields = order_fields;
        step.trigger_events = trigger_events;
        for point in &step.plots {
            if let Some(plot) = self.outputs.plots.get_mut(point.plot_id) {
                plot.points.push(point.clone());
            }
        }
        let mut export_index = 0usize;
        let mut trigger_index = 0usize;
        for decl in &self.compiled.program.outputs {
            match decl.kind {
                OutputKind::ExportSeries => {
                    if let Some(sample) = step.exports.get(export_index).cloned() {
                        self.outputs.exports[export_index].points.push(sample);
                    }
                    export_index += 1;
                }
                OutputKind::Trigger => {
                    if let Some(sample) = step.triggers.get(trigger_index).cloned() {
                        self.outputs.triggers[trigger_index].points.push(sample);
                    }
                    trigger_index += 1;
                }
            }
        }
        for (index, sample) in step.order_fields.iter().cloned().enumerate() {
            self.outputs.order_fields[index].points.push(sample);
        }
        self.outputs
            .trigger_events
            .extend(step.trigger_events.clone());
        self.outputs.alerts.extend(step.alerts.clone());
        self.bar_index += 1;
        Ok(step)
    }

    pub fn finish(self) -> Outputs {
        self.outputs
    }

    fn prepare_source_step(
        &mut self,
        open_time: i64,
        base_interval: Interval,
        base_cursors: &mut [SourceBaseCursor],
        supplemental_cursors: &mut [SourceFeedCursor],
    ) -> Result<Bar, RuntimeError> {
        self.advanced_mask = 0;
        for (slot, local) in self.compiled.program.locals.iter().enumerate() {
            if matches!(local.kind, SlotKind::Scalar) {
                self.current_values[slot] = Value::NA;
            }
        }

        let synthetic_bar = Bar {
            open: f64::NAN,
            high: f64::NAN,
            low: f64::NAN,
            close: f64::NAN,
            volume: f64::NAN,
            time: open_time as f64,
        };

        for cursor in base_cursors {
            let action = match cursor.bars.get(cursor.next_index).copied() {
                Some(bar) if bar_open_time_ms(bar, base_interval)? == open_time => {
                    cursor.next_index += 1;
                    FeedAction::Actual(bar)
                }
                Some(bar) if bar_open_time_ms(bar, base_interval)? < open_time => {
                    return Err(RuntimeError::UnsortedIntervalFeed {
                        interval: base_interval,
                        open_time: bar_open_time_ms(bar, base_interval)?,
                    });
                }
                _ => FeedAction::Synthetic,
            };

            match action {
                FeedAction::Actual(bar) => {
                    self.commit_bar(&cursor.base_slot_map, bar, BASE_UPDATE_MASK)?;
                    self.commit_bar(&cursor.equal_interval_slot_map, bar, base_interval.mask())?;
                }
                FeedAction::Synthetic => {
                    self.commit_values(
                        &cursor.base_slot_map,
                        synthetic_values(),
                        BASE_UPDATE_MASK,
                    )?;
                    self.commit_values(
                        &cursor.equal_interval_slot_map,
                        synthetic_values(),
                        base_interval.mask(),
                    )?;
                }
            }
        }

        let base_close = base_interval.next_open_time(open_time).ok_or(
            RuntimeError::InvalidIntervalAlignment {
                interval: base_interval,
                open_time,
            },
        )?;
        for index in 0..supplemental_cursors.len() {
            self.advance_source_feed(index, supplemental_cursors, base_close)?;
        }
        Ok(synthetic_bar)
    }

    fn collect_outputs(&self, bar: Bar) -> Result<OutputCollections, RuntimeError> {
        let mut exports = Vec::new();
        let mut triggers = Vec::new();
        let mut order_fields = Vec::new();
        let mut trigger_events = Vec::new();

        for (output_id, decl) in self.compiled.program.outputs.iter().enumerate() {
            let value = self.current_values.get(decl.slot as usize).ok_or(
                RuntimeError::InvalidLocalSlot {
                    slot: decl.slot as usize,
                },
            )?;
            let sample = OutputSample {
                output_id,
                name: decl.name.clone(),
                bar_index: self.bar_index,
                time: Some(bar.time),
                value: output_value_for_decl(decl.ty, value, &decl.name)?,
            };
            match decl.kind {
                OutputKind::ExportSeries => exports.push(sample),
                OutputKind::Trigger => {
                    if matches!(sample.value, OutputValue::Bool(true)) {
                        trigger_events.push(TriggerEvent {
                            output_id,
                            name: decl.name.clone(),
                            bar_index: self.bar_index,
                            time: Some(bar.time),
                        });
                    }
                    triggers.push(sample);
                }
            }
        }

        for (field_id, decl) in self.compiled.program.order_fields.iter().enumerate() {
            let value = self.current_values.get(decl.slot as usize).ok_or(
                RuntimeError::InvalidLocalSlot {
                    slot: decl.slot as usize,
                },
            )?;
            order_fields.push(OrderFieldSample {
                field_id,
                name: decl.name.clone(),
                role: decl.role,
                kind: decl.kind,
                bar_index: self.bar_index,
                time: Some(bar.time),
                value: output_value_for_order_field(value, &decl.name)?,
            });
        }

        Ok((exports, triggers, order_fields, trigger_events))
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

    fn advance_source_feed(
        &mut self,
        index: usize,
        cursors: &mut [SourceFeedCursor],
        base_close_time: i64,
    ) -> Result<(), RuntimeError> {
        loop {
            let Some((interval, slot_map, action)) =
                source_feed_action(&mut cursors[index], base_close_time)?
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

    fn set_local_override(&mut self, slot: usize, value: Value) -> Result<(), RuntimeError> {
        let local = self
            .compiled
            .program
            .locals
            .get(slot)
            .ok_or(RuntimeError::InvalidLocalSlot { slot })?;
        match local.kind {
            SlotKind::Scalar => {
                self.current_values[slot] = value;
            }
            SlotKind::Series => {
                self.current_values[slot] = value.clone();
                self.series_values
                    .get_mut(slot)
                    .ok_or(RuntimeError::InvalidSeriesSlot { slot })?
                    .push(value);
            }
        }
        Ok(())
    }
}

impl RuntimeStepper {
    pub fn try_new(
        compiled: &CompiledProgram,
        config: SourceRuntimeConfig,
        limits: VmLimits,
    ) -> Result<Self, RuntimeError> {
        let base_interval = config.base_interval;
        let timeline = source_timeline(&config, base_interval)?;
        let (base_cursors, supplemental_cursors) = build_source_feed_cursors(compiled, &config)?;
        Ok(Self {
            engine: Engine::try_new(compiled.clone(), limits)?,
            timeline,
            next_index: 0,
            base_interval,
            base_cursors,
            supplemental_cursors,
        })
    }

    pub fn step_with_overrides(
        &mut self,
        overrides: &[(u16, Value)],
    ) -> Result<Option<RuntimeStep>, RuntimeError> {
        let Some(open_time) = self.timeline.get(self.next_index).copied() else {
            return Ok(None);
        };
        let bar = self.engine.prepare_source_step(
            open_time,
            self.base_interval,
            &mut self.base_cursors,
            &mut self.supplemental_cursors,
        )?;
        let mut overridden_slots = std::collections::HashSet::with_capacity(overrides.len());
        for (slot, _) in overrides {
            overridden_slots.insert(*slot);
        }
        let default_event_slots: Vec<u16> = self
            .engine
            .compiled
            .program
            .position_event_fields
            .iter()
            .map(|decl| decl.slot)
            .collect();
        for slot in default_event_slots {
            if overridden_slots.contains(&slot) {
                continue;
            }
            self.engine
                .set_local_override(slot as usize, Value::Bool(false))?;
        }
        let default_last_exit_slots: Vec<u16> = self
            .engine
            .compiled
            .program
            .last_exit_fields
            .iter()
            .map(|decl| decl.slot)
            .collect();
        for slot in default_last_exit_slots {
            if overridden_slots.contains(&slot) {
                continue;
            }
            self.engine.set_local_override(slot as usize, Value::NA)?;
        }
        for (slot, value) in overrides {
            self.engine
                .set_local_override(*slot as usize, value.clone())?;
        }
        let output = self.engine.execute_prepared_step(bar)?;
        self.next_index += 1;
        Ok(Some(RuntimeStep {
            open_time,
            bar,
            output,
        }))
    }

    pub fn peek_open_time(&self) -> Option<i64> {
        self.timeline.get(self.next_index).copied()
    }

    pub fn finish(self) -> Outputs {
        self.engine.finish()
    }
}

pub fn run_with_sources(
    compiled: &CompiledProgram,
    config: SourceRuntimeConfig,
    limits: VmLimits,
) -> Result<Outputs, RuntimeError> {
    let mut stepper = RuntimeStepper::try_new(compiled, config, limits)?;
    while stepper.step_with_overrides(&[])?.is_some() {}
    Ok(stepper.finish())
}

fn build_source_feed_cursors(
    compiled: &CompiledProgram,
    config: &SourceRuntimeConfig,
) -> Result<(Vec<SourceBaseCursor>, Vec<SourceFeedCursor>), RuntimeError> {
    let base_interval = config.base_interval;
    let mut base_slot_maps = BTreeMap::<u16, SlotMap>::new();
    let mut equal_slot_maps = BTreeMap::<u16, SlotMap>::new();
    let mut referenced = BTreeMap::<(u16, Interval), SlotMap>::new();

    for local in &compiled.program.locals {
        let Some(binding) = local.market_binding else {
            continue;
        };
        let MarketSource::Named {
            source_id,
            interval,
        } = binding.source;
        match interval {
            None => {
                base_slot_maps.entry(source_id).or_insert([None; 6])[field_index(binding.field)] =
                    Some(slot_for_local(compiled, local)?);
            }
            Some(interval) if interval < base_interval => {
                return Err(RuntimeError::LowerIntervalReference {
                    base: base_interval,
                    referenced: interval,
                });
            }
            Some(interval) if interval == base_interval => {
                equal_slot_maps.entry(source_id).or_insert([None; 6])[field_index(binding.field)] =
                    Some(slot_for_local(compiled, local)?);
            }
            Some(interval) => {
                referenced.entry((source_id, interval)).or_insert([None; 6])
                    [field_index(binding.field)] = Some(slot_for_local(compiled, local)?);
            }
        }
    }

    let mut base_feeds = BTreeMap::<u16, Vec<Bar>>::new();
    let mut supplemental = BTreeMap::<(u16, Interval), Vec<Bar>>::new();
    for feed in &config.feeds {
        validate_feed(feed.interval, &feed.bars)?;
        if feed.interval == base_interval {
            if base_feeds
                .insert(feed.source_id, feed.bars.clone())
                .is_some()
            {
                return Err(RuntimeError::DuplicateSourceBaseFeed {
                    source_id: feed.source_id,
                });
            }
            continue;
        }
        if !referenced.contains_key(&(feed.source_id, feed.interval)) {
            return Err(RuntimeError::UnexpectedSourceFeed {
                source_id: feed.source_id,
                interval: feed.interval,
            });
        }
        if supplemental
            .insert((feed.source_id, feed.interval), feed.bars.clone())
            .is_some()
        {
            return Err(RuntimeError::DuplicateSourceIntervalFeed {
                source_id: feed.source_id,
                interval: feed.interval,
            });
        }
    }

    let mut base_cursors = Vec::new();
    for source in &compiled.program.declared_sources {
        let bars = base_feeds
            .remove(&source.id)
            .ok_or(RuntimeError::MissingSourceBaseFeed {
                source_id: source.id,
            })?;
        base_cursors.push(SourceBaseCursor {
            bars,
            next_index: 0,
            base_slot_map: base_slot_maps.remove(&source.id).unwrap_or([None; 6]),
            equal_interval_slot_map: equal_slot_maps.remove(&source.id).unwrap_or([None; 6]),
        });
    }

    let mut supplemental_cursors = Vec::new();
    for ((source_id, interval), slot_map) in referenced {
        let bars = supplemental.remove(&(source_id, interval)).ok_or(
            RuntimeError::MissingSourceIntervalFeed {
                source_id,
                interval,
            },
        )?;
        let next_expected_open_time = bars
            .first()
            .map(|bar| bar_open_time_ms(*bar, interval))
            .transpose()?;
        supplemental_cursors.push(SourceFeedCursor {
            interval,
            bars,
            next_index: 0,
            next_expected_open_time,
            slot_map,
        });
    }

    Ok((base_cursors, supplemental_cursors))
}

fn source_timeline(
    config: &SourceRuntimeConfig,
    base_interval: Interval,
) -> Result<Vec<i64>, RuntimeError> {
    let mut opens = BTreeSet::new();
    for feed in &config.feeds {
        if feed.interval != base_interval {
            continue;
        }
        for &bar in &feed.bars {
            opens.insert(bar_open_time_ms(bar, base_interval)?);
        }
    }
    Ok(opens.into_iter().collect())
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

fn source_feed_action(
    cursor: &mut SourceFeedCursor,
    base_close_time: i64,
) -> Result<Option<(Interval, SlotMap, FeedAction)>, RuntimeError> {
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

fn output_value_for_decl(
    ty: crate::types::Type,
    value: &Value,
    name: &str,
) -> Result<OutputValue, RuntimeError> {
    match ty {
        crate::types::Type::SeriesF64 => match value {
            Value::F64(value) => Ok(OutputValue::F64(*value)),
            Value::NA => Ok(OutputValue::NA),
            other => Err(RuntimeError::OutputTypeMismatch {
                name: name.to_string(),
                expected: "series<float>",
                found: other.type_name(),
            }),
        },
        crate::types::Type::SeriesBool => match value {
            Value::Bool(value) => Ok(OutputValue::Bool(*value)),
            Value::NA => Ok(OutputValue::NA),
            other => Err(RuntimeError::OutputTypeMismatch {
                name: name.to_string(),
                expected: "series<bool>",
                found: other.type_name(),
            }),
        },
        _ => Err(RuntimeError::OutputTypeMismatch {
            name: name.to_string(),
            expected: "series output",
            found: value.type_name(),
        }),
    }
}

fn output_value_for_order_field(value: &Value, name: &str) -> Result<OutputValue, RuntimeError> {
    match value {
        Value::F64(value) => Ok(OutputValue::F64(*value)),
        Value::NA => Ok(OutputValue::NA),
        other => Err(RuntimeError::OutputTypeMismatch {
            name: name.to_string(),
            expected: "series<float>",
            found: other.type_name(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slice_runtime_window_filters_bars_to_half_open_range() {
        let runtime = SourceRuntimeConfig {
            base_interval: Interval::Hour4,
            feeds: vec![SourceFeed {
                source_id: 0,
                interval: Interval::Hour4,
                bars: vec![
                    Bar {
                        open: 1.0,
                        high: 1.0,
                        low: 1.0,
                        close: 1.0,
                        volume: 1.0,
                        time: 0.0,
                    },
                    Bar {
                        open: 2.0,
                        high: 2.0,
                        low: 2.0,
                        close: 2.0,
                        volume: 2.0,
                        time: 100.0,
                    },
                    Bar {
                        open: 3.0,
                        high: 3.0,
                        low: 3.0,
                        close: 3.0,
                        volume: 3.0,
                        time: 200.0,
                    },
                ],
            }],
        };

        let sliced = slice_runtime_window(&runtime, 100, 200);
        assert_eq!(sliced.feeds.len(), 1);
        assert_eq!(sliced.feeds[0].bars.len(), 1);
        assert_eq!(sliced.feeds[0].bars[0].time as i64, 100);
    }
}
