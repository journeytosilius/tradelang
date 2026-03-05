//! High-level runtime for executing compiled TradeLang programs over bars.
//!
//! This layer owns VM state across bars, including bounded series history,
//! indicator state, outputs, and execution limits.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::builtins::BuiltinId;
use crate::compiler::CompiledProgram;
use crate::diagnostic::RuntimeError;
use crate::indicators::IndicatorState;
use crate::output::{Outputs, PlotSeries};
use crate::types::{SlotKind, Value};
use crate::vm::{SeriesBuffer, Vm, VmEngine};

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Bar {
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: f64,
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

pub struct Engine {
    compiled: CompiledProgram,
    limits: VmLimits,
    current_values: Vec<Value>,
    series_values: Vec<SeriesBuffer>,
    indicator_state: HashMap<(BuiltinId, u16), IndicatorState>,
    outputs: Outputs,
    bar_index: usize,
}

impl Engine {
    pub fn new(compiled: CompiledProgram, limits: VmLimits) -> Self {
        let history = compiled
            .program
            .history_capacity
            .min(limits.max_history_capacity)
            .max(2);
        let local_count = compiled.program.locals.len();
        let current_values = vec![Value::NA; local_count];
        let series_values = compiled
            .program
            .locals
            .iter()
            .map(|local| {
                if matches!(local.kind, SlotKind::Series) {
                    SeriesBuffer::new(history)
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
        Self {
            compiled,
            limits,
            current_values,
            series_values,
            indicator_state: HashMap::new(),
            outputs,
            bar_index: 0,
        }
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
        for (slot, value) in bar.fields().into_iter().enumerate() {
            self.current_values[slot] = value.clone();
            self.series_values
                .get_mut(slot)
                .ok_or(RuntimeError::InvalidSeriesSlot { slot })?
                .push(value);
        }
        for slot in 6..self.compiled.program.locals.len() {
            if matches!(self.compiled.program.locals[slot].kind, SlotKind::Scalar) {
                self.current_values[slot] = Value::NA;
            }
        }
        Ok(())
    }
}

pub fn run(
    compiled: &CompiledProgram,
    bars: &[Bar],
    limits: VmLimits,
) -> Result<Outputs, RuntimeError> {
    let mut engine = Engine::new(compiled.clone(), limits);
    for &bar in bars {
        engine.run_step(bar)?;
    }
    Ok(engine.finish())
}
