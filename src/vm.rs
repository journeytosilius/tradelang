//! Low-level PalmScript bytecode interpreter.
//!
//! This module contains the bounded series buffer, the per-bar VM execution
//! engine, and builtin dispatch used by the runtime hot path.

use std::collections::{HashMap, VecDeque};

use crate::builtins::BuiltinId;
use crate::bytecode::{Constant, Instruction, OpCode, Program};
use crate::diagnostic::RuntimeError;
use crate::indicators::{
    apply_unary_math, calculate_aroon, calculate_aroonosc, calculate_avgdev, calculate_beta,
    calculate_bop, calculate_cci, calculate_correl, calculate_linear_regression,
    calculate_max_index, calculate_min_index, calculate_min_max, calculate_min_max_index,
    calculate_stddev, calculate_sum, calculate_trange, calculate_var, calculate_willr,
    calculate_wma, BarsSinceState, CmoState, EmaState, FallingState, HighestState, IndicatorState,
    LowestState, MacdState, ObvState, OscillatorKind, PriceOscillatorState, RegressionOutput,
    RisingState, RsiState, SmaState, UnaryMathTransform, ValueWhenState,
};
use crate::output::{PlotPoint, StepOutput};
use crate::runtime::Bar;
use crate::talib::MaType;
use crate::types::{SlotKind, Value};

#[derive(Clone, Debug)]
pub struct SeriesBuffer {
    capacity: usize,
    values: VecDeque<Value>,
    version: u64,
}

impl SeriesBuffer {
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity,
            values: VecDeque::with_capacity(capacity),
            version: 0,
        }
    }

    pub fn push(&mut self, value: Value) {
        if self.values.len() == self.capacity {
            self.values.pop_front();
        }
        self.values.push_back(value);
        self.version += 1;
    }

    pub fn get(&self, offset: usize) -> Value {
        if offset >= self.values.len() {
            Value::NA
        } else {
            self.values[self.values.len() - 1 - offset].clone()
        }
    }

    pub fn len(&self) -> usize {
        self.values.len()
    }

    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    pub fn iter_recent(&self, count: usize) -> impl Iterator<Item = &Value> {
        self.values.iter().rev().take(count)
    }

    pub const fn version(&self) -> u64 {
        self.version
    }
}

pub struct Vm<'a> {
    program: &'a Program,
}

impl<'a> Vm<'a> {
    pub fn new(program: &'a Program) -> Self {
        Self { program }
    }

    pub(crate) fn execute(&self, engine: &mut VmEngine<'_>) -> Result<StepOutput, RuntimeError> {
        let mut pc = 0usize;
        let mut stack = Vec::new();
        let mut plots = Vec::new();
        let alerts = Vec::new();

        while pc < self.program.instructions.len() {
            engine.consume_steps(1, pc)?;
            let instruction = &self.program.instructions[pc];
            match instruction.opcode {
                OpCode::LoadConst => {
                    stack.push(engine.constant(instruction.a as usize)?.clone());
                }
                OpCode::LoadLocal => {
                    stack.push(engine.load_local(instruction.a as usize)?);
                }
                OpCode::StoreLocal => {
                    let value = pop(&mut stack, pc, instruction.opcode)?;
                    engine.store_local(instruction.a as usize, value)?;
                }
                OpCode::LoadSeries => {
                    engine.ensure_series_slot(instruction.a as usize)?;
                    stack.push(Value::SeriesRef(instruction.a as usize));
                }
                OpCode::SeriesGet => {
                    let series = pop(&mut stack, pc, instruction.opcode)?;
                    let slot = series_ref(series, pc)?;
                    let value = engine.load_series_value(slot, instruction.a as usize)?;
                    stack.push(value);
                }
                OpCode::Neg => {
                    let value = pop(&mut stack, pc, instruction.opcode)?;
                    stack.push(engine.neg(value, pc)?);
                }
                OpCode::Not => {
                    let value = pop(&mut stack, pc, instruction.opcode)?;
                    stack.push(engine.not(value, pc)?);
                }
                OpCode::Add
                | OpCode::Sub
                | OpCode::Mul
                | OpCode::Div
                | OpCode::And
                | OpCode::Or
                | OpCode::Eq
                | OpCode::Ne
                | OpCode::Lt
                | OpCode::Le
                | OpCode::Gt
                | OpCode::Ge => {
                    let right = pop(&mut stack, pc, instruction.opcode)?;
                    let left = pop(&mut stack, pc, instruction.opcode)?;
                    let value = engine.binary(instruction.opcode, left, right, pc)?;
                    stack.push(value);
                }
                OpCode::Pop => {
                    pop(&mut stack, pc, instruction.opcode)?;
                }
                OpCode::Jump => {
                    pc = jump_target(instruction, self.program.instructions.len(), pc)?;
                    continue;
                }
                OpCode::JumpIfFalse => {
                    let condition = pop(&mut stack, pc, instruction.opcode)?;
                    if engine.is_falsey(condition, pc)? {
                        pc = jump_target(instruction, self.program.instructions.len(), pc)?;
                        continue;
                    }
                }
                OpCode::CallBuiltin => {
                    let mut args = Vec::with_capacity(instruction.b as usize);
                    for _ in 0..instruction.b {
                        args.push(pop(&mut stack, pc, instruction.opcode)?);
                    }
                    args.reverse();
                    let result = engine.call_builtin(
                        instruction.a,
                        instruction.b as usize,
                        instruction.c,
                        args,
                        pc,
                        &mut plots,
                    )?;
                    if !matches!(result, Value::Void) {
                        stack.push(result);
                    }
                }
                OpCode::UnpackTuple => {
                    let value = pop(&mut stack, pc, instruction.opcode)?;
                    match value {
                        Value::Tuple2(values) if instruction.a as usize == 2 => {
                            stack.push(*values[0].clone());
                            stack.push(*values[1].clone());
                        }
                        Value::Tuple3(values) if instruction.a as usize == 3 => {
                            stack.push(*values[0].clone());
                            stack.push(*values[1].clone());
                            stack.push(*values[2].clone());
                        }
                        other => {
                            return Err(RuntimeError::TupleArityMismatch {
                                pc,
                                expected: instruction.a as usize,
                                found: other.type_name(),
                            });
                        }
                    }
                }
                OpCode::Return => {
                    break;
                }
            }
            pc += 1;
        }

        Ok(StepOutput {
            plots,
            exports: Vec::new(),
            triggers: Vec::new(),
            trigger_events: Vec::new(),
            alerts,
        })
    }
}

fn pop(stack: &mut Vec<Value>, pc: usize, opcode: OpCode) -> Result<Value, RuntimeError> {
    stack
        .pop()
        .ok_or(RuntimeError::StackUnderflow { pc, opcode })
}

fn series_ref(value: Value, pc: usize) -> Result<usize, RuntimeError> {
    match value {
        Value::SeriesRef(slot) => Ok(slot),
        other => Err(RuntimeError::TypeMismatch {
            pc,
            expected: "series-ref",
            found: other.type_name(),
        }),
    }
}

fn jump_target(
    instruction: &Instruction,
    instruction_len: usize,
    pc: usize,
) -> Result<usize, RuntimeError> {
    let target = instruction.a as usize;
    if target >= instruction_len {
        Err(RuntimeError::InvalidJump { pc, target })
    } else {
        Ok(target)
    }
}

pub(crate) struct VmEngine<'a> {
    pub program: &'a Program,
    pub bar_index: usize,
    pub current_bar: &'a Bar,
    pub current_values: &'a mut [Value],
    pub series_values: &'a mut [SeriesBuffer],
    pub remaining_steps: &'a mut usize,
    pub indicator_state: &'a mut HashMap<(BuiltinId, u16), IndicatorState>,
    pub advanced_mask: u32,
}

impl<'a> VmEngine<'a> {
    pub fn constant(&self, index: usize) -> Result<&Value, RuntimeError> {
        match self.program.constants.get(index) {
            Some(Constant::Value(value)) => Ok(value),
            None => Err(RuntimeError::InvalidLocalSlot { slot: index }),
        }
    }

    pub fn consume_steps(&mut self, amount: usize, pc: usize) -> Result<(), RuntimeError> {
        if *self.remaining_steps < amount {
            return Err(RuntimeError::InstructionBudgetExceeded {
                bar_index: self.bar_index,
                pc,
            });
        }
        *self.remaining_steps -= amount;
        Ok(())
    }

    pub fn load_local(&self, slot: usize) -> Result<Value, RuntimeError> {
        self.current_values
            .get(slot)
            .cloned()
            .ok_or(RuntimeError::InvalidLocalSlot { slot })
    }

    pub fn store_local(&mut self, slot: usize, value: Value) -> Result<(), RuntimeError> {
        let local = self
            .program
            .locals
            .get(slot)
            .ok_or(RuntimeError::InvalidLocalSlot { slot })?;
        self.current_values[slot] = value.clone();
        if matches!(local.kind, SlotKind::Series)
            && (local.update_mask == 0 || (local.update_mask & self.advanced_mask) != 0)
        {
            self.series_values
                .get_mut(slot)
                .ok_or(RuntimeError::InvalidSeriesSlot { slot })?
                .push(value);
        }
        Ok(())
    }

    pub fn ensure_series_slot(&self, slot: usize) -> Result<(), RuntimeError> {
        if self.series_values.get(slot).is_some() {
            Ok(())
        } else {
            Err(RuntimeError::InvalidSeriesSlot { slot })
        }
    }

    pub fn load_series_value(&self, slot: usize, offset: usize) -> Result<Value, RuntimeError> {
        let buffer = self
            .series_values
            .get(slot)
            .ok_or(RuntimeError::InvalidSeriesSlot { slot })?;
        Ok(buffer.get(offset))
    }

    pub fn neg(&self, value: Value, pc: usize) -> Result<Value, RuntimeError> {
        match value {
            Value::NA => Ok(Value::NA),
            Value::F64(value) => Ok(Value::F64(-value)),
            other => Err(RuntimeError::TypeMismatch {
                pc,
                expected: "f64",
                found: other.type_name(),
            }),
        }
    }

    pub fn not(&self, value: Value, pc: usize) -> Result<Value, RuntimeError> {
        match value {
            Value::NA => Ok(Value::NA),
            Value::Bool(value) => Ok(Value::Bool(!value)),
            other => Err(RuntimeError::TypeMismatch {
                pc,
                expected: "bool",
                found: other.type_name(),
            }),
        }
    }

    pub fn binary(
        &self,
        opcode: OpCode,
        left: Value,
        right: Value,
        pc: usize,
    ) -> Result<Value, RuntimeError> {
        match opcode {
            OpCode::And => logical_and(left, right, pc),
            OpCode::Or => logical_or(left, right, pc),
            OpCode::Add | OpCode::Sub | OpCode::Mul | OpCode::Div => {
                if left.is_na() || right.is_na() {
                    return Ok(Value::NA);
                }
                let left = expect_f64(left, pc)?;
                let right = expect_f64(right, pc)?;
                let value = match opcode {
                    OpCode::Add => left + right,
                    OpCode::Sub => left - right,
                    OpCode::Mul => left * right,
                    OpCode::Div => left / right,
                    _ => unreachable!(),
                };
                Ok(Value::F64(value))
            }
            OpCode::Eq => {
                if left.is_na() || right.is_na() {
                    return Ok(Value::NA);
                }
                Ok(Value::Bool(eq_values(&left, &right)))
            }
            OpCode::Ne => {
                if left.is_na() || right.is_na() {
                    return Ok(Value::NA);
                }
                Ok(Value::Bool(!eq_values(&left, &right)))
            }
            OpCode::Lt | OpCode::Le | OpCode::Gt | OpCode::Ge => {
                if left.is_na() || right.is_na() {
                    return Ok(Value::NA);
                }
                let left = expect_f64(left, pc)?;
                let right = expect_f64(right, pc)?;
                let value = match opcode {
                    OpCode::Lt => left < right,
                    OpCode::Le => left <= right,
                    OpCode::Gt => left > right,
                    OpCode::Ge => left >= right,
                    _ => unreachable!(),
                };
                Ok(Value::Bool(value))
            }
            _ => unreachable!(),
        }
    }

    pub fn is_falsey(&self, value: Value, pc: usize) -> Result<bool, RuntimeError> {
        match value {
            Value::NA => Ok(true),
            Value::Bool(value) => Ok(!value),
            other => Err(RuntimeError::TypeMismatch {
                pc,
                expected: "bool-or-na",
                found: other.type_name(),
            }),
        }
    }

    pub fn call_builtin(
        &mut self,
        builtin_id: u16,
        arity: usize,
        callsite: u16,
        args: Vec<Value>,
        pc: usize,
        plots: &mut Vec<PlotPoint>,
    ) -> Result<Value, RuntimeError> {
        let builtin =
            BuiltinId::from_u16(builtin_id).ok_or(RuntimeError::UnknownBuiltin { builtin_id })?;
        match builtin {
            BuiltinId::Plot => {
                if arity != 1 {
                    return Err(RuntimeError::ArityMismatch {
                        builtin: builtin.as_str(),
                        expected: 1,
                        found: arity,
                    });
                }
                let value = args.into_iter().next().unwrap_or(Value::NA);
                plots.push(PlotPoint {
                    plot_id: callsite as usize,
                    bar_index: self.bar_index,
                    time: Some(self.current_bar.time),
                    value: value.as_f64(),
                });
                Ok(Value::Void)
            }
            BuiltinId::Sma => self.call_sma(callsite, arity, args, pc),
            BuiltinId::Ema => self.call_ema(callsite, arity, args, pc),
            BuiltinId::Rsi => self.call_rsi(callsite, arity, args, pc),
            BuiltinId::Cmo => self.call_cmo(callsite, arity, args, pc),
            BuiltinId::Ma => self.call_ma(callsite, arity, args, pc),
            BuiltinId::Macd => self.call_macd(callsite, arity, args, pc),
            BuiltinId::Acos => self.call_unary_math(UnaryMathTransform::Acos, arity, args, pc),
            BuiltinId::Asin => self.call_unary_math(UnaryMathTransform::Asin, arity, args, pc),
            BuiltinId::Atan => self.call_unary_math(UnaryMathTransform::Atan, arity, args, pc),
            BuiltinId::Ceil => self.call_unary_math(UnaryMathTransform::Ceil, arity, args, pc),
            BuiltinId::Cos => self.call_unary_math(UnaryMathTransform::Cos, arity, args, pc),
            BuiltinId::Cosh => self.call_unary_math(UnaryMathTransform::Cosh, arity, args, pc),
            BuiltinId::Exp => self.call_unary_math(UnaryMathTransform::Exp, arity, args, pc),
            BuiltinId::Floor => self.call_unary_math(UnaryMathTransform::Floor, arity, args, pc),
            BuiltinId::Ln => self.call_unary_math(UnaryMathTransform::Ln, arity, args, pc),
            BuiltinId::Log10 => self.call_unary_math(UnaryMathTransform::Log10, arity, args, pc),
            BuiltinId::Sin => self.call_unary_math(UnaryMathTransform::Sin, arity, args, pc),
            BuiltinId::Sinh => self.call_unary_math(UnaryMathTransform::Sinh, arity, args, pc),
            BuiltinId::Sqrt => self.call_unary_math(UnaryMathTransform::Sqrt, arity, args, pc),
            BuiltinId::Tan => self.call_unary_math(UnaryMathTransform::Tan, arity, args, pc),
            BuiltinId::Tanh => self.call_unary_math(UnaryMathTransform::Tanh, arity, args, pc),
            BuiltinId::Cross | BuiltinId::Crossover | BuiltinId::Crossunder => {
                self.call_cross_builtin(builtin, arity, args, pc)
            }
            BuiltinId::Change => self.call_change(arity, args, pc),
            BuiltinId::Roc => self.call_roc(arity, args, pc),
            BuiltinId::Mom => self.call_mom(arity, args, pc),
            BuiltinId::Rocp => self.call_rocp(arity, args, pc),
            BuiltinId::Rocr => self.call_rocr(arity, args, pc),
            BuiltinId::Rocr100 => self.call_rocr100(arity, args, pc),
            BuiltinId::Apo => self.call_apo(callsite, arity, args, pc),
            BuiltinId::Ppo => self.call_ppo(callsite, arity, args, pc),
            BuiltinId::Highest => self.call_highest(callsite, arity, args, pc),
            BuiltinId::Lowest => self.call_lowest(callsite, arity, args, pc),
            BuiltinId::Sum => self.call_sum(arity, args, pc),
            BuiltinId::Rising => self.call_rising(callsite, arity, args, pc),
            BuiltinId::Falling => self.call_falling(callsite, arity, args, pc),
            BuiltinId::BarsSince => self.call_barssince(callsite, arity, args, pc),
            BuiltinId::ValueWhen => self.call_valuewhen(callsite, arity, args, pc),
            BuiltinId::Obv => self.call_obv(callsite, arity, args, pc),
            BuiltinId::Trange => self.call_trange(arity, args, pc),
            BuiltinId::Wma => self.call_wma(arity, args, pc),
            BuiltinId::Avgdev => self.call_avgdev(arity, args, pc),
            BuiltinId::MaxIndex => self.call_max_index(arity, args, pc),
            BuiltinId::MinIndex => self.call_min_index(arity, args, pc),
            BuiltinId::MinMax => self.call_min_max(arity, args, pc),
            BuiltinId::MinMaxIndex => self.call_min_max_index(arity, args, pc),
            BuiltinId::Stddev => self.call_stddev(arity, args, pc),
            BuiltinId::Var => self.call_var(arity, args, pc),
            BuiltinId::LinearReg => self.call_linearreg(arity, args, pc),
            BuiltinId::LinearRegAngle => self.call_linearreg_angle(arity, args, pc),
            BuiltinId::LinearRegIntercept => self.call_linearreg_intercept(arity, args, pc),
            BuiltinId::LinearRegSlope => self.call_linearreg_slope(arity, args, pc),
            BuiltinId::Tsf => self.call_tsf(arity, args, pc),
            BuiltinId::Beta => self.call_beta(arity, args, pc),
            BuiltinId::Correl => self.call_correl(arity, args, pc),
            BuiltinId::Willr => self.call_willr(arity, args, pc),
            BuiltinId::Aroon => self.call_aroon(arity, args, pc),
            BuiltinId::AroonOsc => self.call_aroonosc(arity, args, pc),
            BuiltinId::Bop => self.call_bop(arity, args, pc),
            BuiltinId::Cci => self.call_cci(arity, args, pc),
            _ => Err(RuntimeError::UnknownBuiltin { builtin_id }),
        }
    }

    fn call_unary_math(
        &mut self,
        transform: UnaryMathTransform,
        arity: usize,
        args: Vec<Value>,
        pc: usize,
    ) -> Result<Value, RuntimeError> {
        if arity != 1 {
            return Err(RuntimeError::ArityMismatch {
                builtin: "math transform",
                expected: 1,
                found: arity,
            });
        }
        apply_unary_math(args.into_iter().next().unwrap_or(Value::NA), transform, pc)
    }

    fn call_sma(
        &mut self,
        callsite: u16,
        arity: usize,
        args: Vec<Value>,
        pc: usize,
    ) -> Result<Value, RuntimeError> {
        if arity != 2 {
            return Err(RuntimeError::ArityMismatch {
                builtin: "sma",
                expected: 2,
                found: arity,
            });
        }
        let series_slot = series_ref(args[0].clone(), pc)?;
        let window = expect_window(args[1].clone(), pc)?;
        self.consume_steps(window, pc)?;
        let buffer = self
            .series_values
            .get(series_slot)
            .ok_or(RuntimeError::InvalidSeriesSlot { slot: series_slot })?;
        let key = (BuiltinId::Sma, callsite);
        let mut state = self
            .indicator_state
            .remove(&key)
            .unwrap_or(IndicatorState::Sma(SmaState::new(window)));
        let result = match &mut state {
            IndicatorState::Sma(state) => match state.update(buffer, pc)? {
                Some(value) => {
                    self.consume_steps(window, pc)?;
                    value
                }
                None => state.cached_output(),
            },
            _ => unreachable!(),
        };
        self.indicator_state.insert(key, state);
        Ok(result)
    }

    fn call_ema(
        &mut self,
        callsite: u16,
        arity: usize,
        args: Vec<Value>,
        pc: usize,
    ) -> Result<Value, RuntimeError> {
        if arity != 2 {
            return Err(RuntimeError::ArityMismatch {
                builtin: "ema",
                expected: 2,
                found: arity,
            });
        }
        let series_slot = series_ref(args[0].clone(), pc)?;
        let window = expect_window(args[1].clone(), pc)?;
        let key = (BuiltinId::Ema, callsite);
        let mut state = self
            .indicator_state
            .remove(&key)
            .unwrap_or(IndicatorState::Ema(EmaState::new(window)));
        let result = match &mut state {
            IndicatorState::Ema(state) => {
                let buffer_version = self
                    .series_values
                    .get(series_slot)
                    .ok_or(RuntimeError::InvalidSeriesSlot { slot: series_slot })?
                    .version();
                if buffer_version != state.last_seen_version() && !state.is_seeded() {
                    self.consume_steps(state.seed_window(), pc)?;
                }
                let buffer = self
                    .series_values
                    .get(series_slot)
                    .ok_or(RuntimeError::InvalidSeriesSlot { slot: series_slot })?;
                state.update(buffer, pc)?
            }
            _ => unreachable!(),
        };
        self.indicator_state.insert(key, state);
        Ok(result)
    }

    fn call_rsi(
        &mut self,
        callsite: u16,
        arity: usize,
        args: Vec<Value>,
        pc: usize,
    ) -> Result<Value, RuntimeError> {
        if arity != 2 {
            return Err(RuntimeError::ArityMismatch {
                builtin: "rsi",
                expected: 2,
                found: arity,
            });
        }
        let series_slot = series_ref(args[0].clone(), pc)?;
        let window = expect_window(args[1].clone(), pc)?;
        let key = (BuiltinId::Rsi, callsite);
        let mut state = self
            .indicator_state
            .remove(&key)
            .unwrap_or(IndicatorState::Rsi(RsiState::new(window)));
        let result = match &mut state {
            IndicatorState::Rsi(state) => {
                let buffer_version = self
                    .series_values
                    .get(series_slot)
                    .ok_or(RuntimeError::InvalidSeriesSlot { slot: series_slot })?
                    .version();
                if buffer_version != state.last_seen_version() && state.requires_seed_step() {
                    self.consume_steps(1, pc)?;
                }
                let buffer = self
                    .series_values
                    .get(series_slot)
                    .ok_or(RuntimeError::InvalidSeriesSlot { slot: series_slot })?;
                state.update(buffer)
            }
            _ => unreachable!(),
        };
        self.indicator_state.insert(key, state);
        Ok(result)
    }

    fn call_cmo(
        &mut self,
        callsite: u16,
        arity: usize,
        args: Vec<Value>,
        pc: usize,
    ) -> Result<Value, RuntimeError> {
        if arity != 2 {
            return Err(RuntimeError::ArityMismatch {
                builtin: "cmo",
                expected: 2,
                found: arity,
            });
        }
        let series_slot = series_ref(args[0].clone(), pc)?;
        let window = expect_window(args[1].clone(), pc)?;
        let key = (BuiltinId::Cmo, callsite);
        let mut state = self
            .indicator_state
            .remove(&key)
            .unwrap_or(IndicatorState::Cmo(CmoState::new(window)));
        let result = match &mut state {
            IndicatorState::Cmo(state) => {
                let buffer_version = self
                    .series_values
                    .get(series_slot)
                    .ok_or(RuntimeError::InvalidSeriesSlot { slot: series_slot })?
                    .version();
                if buffer_version != state.last_seen_version() && state.requires_seed_step() {
                    self.consume_steps(1, pc)?;
                }
                let buffer = self
                    .series_values
                    .get(series_slot)
                    .ok_or(RuntimeError::InvalidSeriesSlot { slot: series_slot })?;
                state.update(buffer)
            }
            _ => unreachable!(),
        };
        self.indicator_state.insert(key, state);
        Ok(result)
    }

    fn call_ma(
        &mut self,
        callsite: u16,
        arity: usize,
        args: Vec<Value>,
        pc: usize,
    ) -> Result<Value, RuntimeError> {
        if arity != 3 {
            return Err(RuntimeError::ArityMismatch {
                builtin: "ma",
                expected: 3,
                found: arity,
            });
        }
        let ma_type = expect_ma_type(args[2].clone(), pc)?;
        match ma_type {
            MaType::Sma => self.call_sma(callsite, 2, vec![args[0].clone(), args[1].clone()], pc),
            MaType::Ema => self.call_ema(callsite, 2, vec![args[0].clone(), args[1].clone()], pc),
            MaType::Wma => {
                let series_slot = series_ref(args[0].clone(), pc)?;
                let window = expect_window(args[1].clone(), pc)?;
                self.consume_steps(window, pc)?;
                let buffer = self
                    .series_values
                    .get(series_slot)
                    .ok_or(RuntimeError::InvalidSeriesSlot { slot: series_slot })?;
                calculate_wma(buffer, window, pc)
            }
            other => Err(RuntimeError::UnsupportedMaType {
                builtin: "ma",
                ma_type: other.as_str(),
            }),
        }
    }

    fn call_macd(
        &mut self,
        callsite: u16,
        arity: usize,
        args: Vec<Value>,
        pc: usize,
    ) -> Result<Value, RuntimeError> {
        if arity != 4 {
            return Err(RuntimeError::ArityMismatch {
                builtin: "macd",
                expected: 4,
                found: arity,
            });
        }
        let series_slot = series_ref(args[0].clone(), pc)?;
        let fast = expect_window(args[1].clone(), pc)?;
        let slow = expect_window(args[2].clone(), pc)?;
        let signal = expect_window(args[3].clone(), pc)?;
        let key = (BuiltinId::Macd, callsite);
        let mut state = self
            .indicator_state
            .remove(&key)
            .unwrap_or(IndicatorState::Macd(MacdState::new(fast, slow, signal)));
        let result = match &mut state {
            IndicatorState::Macd(state) => {
                let buffer = self
                    .series_values
                    .get(series_slot)
                    .ok_or(RuntimeError::InvalidSeriesSlot { slot: series_slot })?;
                state.update(buffer, pc)?
            }
            _ => unreachable!(),
        };
        self.indicator_state.insert(key, state);
        Ok(result)
    }

    fn call_cross_builtin(
        &mut self,
        builtin: BuiltinId,
        arity: usize,
        args: Vec<Value>,
        pc: usize,
    ) -> Result<Value, RuntimeError> {
        if arity != 4 {
            return Err(RuntimeError::ArityMismatch {
                builtin: builtin.as_str(),
                expected: 4,
                found: arity,
            });
        }
        let current_left = expect_numeric_like(&args[0], pc)?;
        let current_right = expect_numeric_like(&args[1], pc)?;
        let prior_left = expect_numeric_like(&args[2], pc)?;
        let prior_right = expect_numeric_like(&args[3], pc)?;
        if [current_left, current_right, prior_left, prior_right]
            .iter()
            .any(|value| value.is_none())
        {
            return Ok(Value::NA);
        }
        let current_left = current_left.unwrap();
        let current_right = current_right.unwrap();
        let prior_left = prior_left.unwrap();
        let prior_right = prior_right.unwrap();
        let crossed_over = current_left > current_right && prior_left <= prior_right;
        let crossed_under = current_left < current_right && prior_left >= prior_right;
        let value = match builtin {
            BuiltinId::Cross => crossed_over || crossed_under,
            BuiltinId::Crossover => crossed_over,
            BuiltinId::Crossunder => crossed_under,
            _ => unreachable!(),
        };
        Ok(Value::Bool(value))
    }

    fn call_change(
        &mut self,
        arity: usize,
        args: Vec<Value>,
        pc: usize,
    ) -> Result<Value, RuntimeError> {
        if arity != 2 {
            return Err(RuntimeError::ArityMismatch {
                builtin: "change",
                expected: 2,
                found: arity,
            });
        }
        let series_slot = series_ref(args[0].clone(), pc)?;
        let window = expect_window(args[1].clone(), pc)?;
        self.consume_steps(window + 1, pc)?;
        let buffer = self
            .series_values
            .get(series_slot)
            .ok_or(RuntimeError::InvalidSeriesSlot { slot: series_slot })?;
        let current = expect_buffer_f64(buffer, 0, pc)?;
        let previous = expect_buffer_f64(buffer, window, pc)?;
        match (current, previous) {
            (Some(current), Some(previous)) => Ok(Value::F64(current - previous)),
            _ => Ok(Value::NA),
        }
    }

    fn call_roc(
        &mut self,
        arity: usize,
        args: Vec<Value>,
        pc: usize,
    ) -> Result<Value, RuntimeError> {
        self.call_rate_of_change_family(BuiltinId::Roc, arity, args, pc)
    }

    fn call_mom(
        &mut self,
        arity: usize,
        args: Vec<Value>,
        pc: usize,
    ) -> Result<Value, RuntimeError> {
        self.call_rate_of_change_family(BuiltinId::Mom, arity, args, pc)
    }

    fn call_rocp(
        &mut self,
        arity: usize,
        args: Vec<Value>,
        pc: usize,
    ) -> Result<Value, RuntimeError> {
        self.call_rate_of_change_family(BuiltinId::Rocp, arity, args, pc)
    }

    fn call_rocr(
        &mut self,
        arity: usize,
        args: Vec<Value>,
        pc: usize,
    ) -> Result<Value, RuntimeError> {
        self.call_rate_of_change_family(BuiltinId::Rocr, arity, args, pc)
    }

    fn call_rocr100(
        &mut self,
        arity: usize,
        args: Vec<Value>,
        pc: usize,
    ) -> Result<Value, RuntimeError> {
        self.call_rate_of_change_family(BuiltinId::Rocr100, arity, args, pc)
    }

    fn call_rate_of_change_family(
        &mut self,
        builtin: BuiltinId,
        arity: usize,
        args: Vec<Value>,
        pc: usize,
    ) -> Result<Value, RuntimeError> {
        if arity != 2 {
            return Err(RuntimeError::ArityMismatch {
                builtin: builtin.as_str(),
                expected: 2,
                found: arity,
            });
        }
        let series_slot = series_ref(args[0].clone(), pc)?;
        let window = expect_window(args[1].clone(), pc)?;
        self.consume_steps(window + 1, pc)?;
        let buffer = self
            .series_values
            .get(series_slot)
            .ok_or(RuntimeError::InvalidSeriesSlot { slot: series_slot })?;
        let current = expect_buffer_f64(buffer, 0, pc)?;
        let previous = expect_buffer_f64(buffer, window, pc)?;
        match (current, previous) {
            (Some(current), Some(previous)) => match builtin {
                BuiltinId::Mom => Ok(Value::F64(current - previous)),
                BuiltinId::Roc if previous != 0.0 => {
                    Ok(Value::F64(((current - previous) / previous) * 100.0))
                }
                BuiltinId::Rocp if previous != 0.0 => {
                    Ok(Value::F64((current - previous) / previous))
                }
                BuiltinId::Rocr if previous != 0.0 => Ok(Value::F64(current / previous)),
                BuiltinId::Rocr100 if previous != 0.0 => {
                    Ok(Value::F64((current / previous) * 100.0))
                }
                BuiltinId::Roc | BuiltinId::Rocp | BuiltinId::Rocr | BuiltinId::Rocr100 => {
                    Ok(Value::NA)
                }
                _ => unreachable!(),
            },
            (None, _) | (_, None) => Ok(Value::NA),
        }
    }

    fn call_apo(
        &mut self,
        callsite: u16,
        arity: usize,
        args: Vec<Value>,
        pc: usize,
    ) -> Result<Value, RuntimeError> {
        self.call_ma_oscillator(
            BuiltinId::Apo,
            callsite,
            arity,
            args,
            pc,
            OscillatorKind::Absolute,
        )
    }

    fn call_ppo(
        &mut self,
        callsite: u16,
        arity: usize,
        args: Vec<Value>,
        pc: usize,
    ) -> Result<Value, RuntimeError> {
        self.call_ma_oscillator(
            BuiltinId::Ppo,
            callsite,
            arity,
            args,
            pc,
            OscillatorKind::Percentage,
        )
    }

    fn call_ma_oscillator(
        &mut self,
        builtin_id: BuiltinId,
        callsite: u16,
        arity: usize,
        args: Vec<Value>,
        pc: usize,
        kind: OscillatorKind,
    ) -> Result<Value, RuntimeError> {
        let builtin_name = builtin_id.as_str();
        if arity != 4 {
            return Err(RuntimeError::ArityMismatch {
                builtin: builtin_name,
                expected: 4,
                found: arity,
            });
        }
        let series_slot = series_ref(args[0].clone(), pc)?;
        let fast = expect_window(args[1].clone(), pc)?;
        let slow = expect_window(args[2].clone(), pc)?;
        let ma_type = expect_ma_type(args[3].clone(), pc)?;
        let key = (builtin_id, callsite);
        let mut state =
            self.indicator_state
                .remove(&key)
                .unwrap_or(IndicatorState::PriceOscillator(PriceOscillatorState::new(
                    builtin_name,
                    fast,
                    slow,
                    ma_type,
                    kind,
                )));
        let result = match &mut state {
            IndicatorState::PriceOscillator(state) => {
                let buffer = self
                    .series_values
                    .get(series_slot)
                    .ok_or(RuntimeError::InvalidSeriesSlot { slot: series_slot })?;
                state.update(buffer, pc)?
            }
            _ => unreachable!(),
        };
        self.indicator_state.insert(key, state);
        Ok(result)
    }

    fn call_sum(
        &mut self,
        arity: usize,
        args: Vec<Value>,
        pc: usize,
    ) -> Result<Value, RuntimeError> {
        if arity != 2 {
            return Err(RuntimeError::ArityMismatch {
                builtin: "sum",
                expected: 2,
                found: arity,
            });
        }
        let series_slot = series_ref(args[0].clone(), pc)?;
        let window = expect_window(args[1].clone(), pc)?;
        self.consume_steps(window, pc)?;
        let buffer = self
            .series_values
            .get(series_slot)
            .ok_or(RuntimeError::InvalidSeriesSlot { slot: series_slot })?;
        calculate_sum(buffer, window, pc)
    }

    fn call_wma(
        &mut self,
        arity: usize,
        args: Vec<Value>,
        pc: usize,
    ) -> Result<Value, RuntimeError> {
        self.call_stateless_window_builtin("wma", arity, args, pc, calculate_wma)
    }

    fn call_avgdev(
        &mut self,
        arity: usize,
        args: Vec<Value>,
        pc: usize,
    ) -> Result<Value, RuntimeError> {
        self.call_stateless_window_builtin("avgdev", arity, args, pc, calculate_avgdev)
    }

    fn call_max_index(
        &mut self,
        arity: usize,
        args: Vec<Value>,
        pc: usize,
    ) -> Result<Value, RuntimeError> {
        self.call_stateless_window_builtin("maxindex", arity, args, pc, calculate_max_index)
    }

    fn call_min_index(
        &mut self,
        arity: usize,
        args: Vec<Value>,
        pc: usize,
    ) -> Result<Value, RuntimeError> {
        self.call_stateless_window_builtin("minindex", arity, args, pc, calculate_min_index)
    }

    fn call_min_max(
        &mut self,
        arity: usize,
        args: Vec<Value>,
        pc: usize,
    ) -> Result<Value, RuntimeError> {
        self.call_stateless_window_builtin("minmax", arity, args, pc, calculate_min_max)
    }

    fn call_min_max_index(
        &mut self,
        arity: usize,
        args: Vec<Value>,
        pc: usize,
    ) -> Result<Value, RuntimeError> {
        self.call_stateless_window_builtin("minmaxindex", arity, args, pc, calculate_min_max_index)
    }

    fn call_stddev(
        &mut self,
        arity: usize,
        args: Vec<Value>,
        pc: usize,
    ) -> Result<Value, RuntimeError> {
        self.call_stateless_window_factor_builtin("stddev", arity, args, pc, calculate_stddev)
    }

    fn call_var(
        &mut self,
        arity: usize,
        args: Vec<Value>,
        pc: usize,
    ) -> Result<Value, RuntimeError> {
        self.call_stateless_window_factor_builtin("var", arity, args, pc, calculate_var)
    }

    fn call_linearreg(
        &mut self,
        arity: usize,
        args: Vec<Value>,
        pc: usize,
    ) -> Result<Value, RuntimeError> {
        self.call_stateless_window_builtin("linearreg", arity, args, pc, |buffer, window, pc| {
            calculate_linear_regression(buffer, window, RegressionOutput::Value, pc)
        })
    }

    fn call_linearreg_angle(
        &mut self,
        arity: usize,
        args: Vec<Value>,
        pc: usize,
    ) -> Result<Value, RuntimeError> {
        self.call_stateless_window_builtin(
            "linearreg_angle",
            arity,
            args,
            pc,
            |buffer, window, pc| {
                calculate_linear_regression(buffer, window, RegressionOutput::Angle, pc)
            },
        )
    }

    fn call_linearreg_intercept(
        &mut self,
        arity: usize,
        args: Vec<Value>,
        pc: usize,
    ) -> Result<Value, RuntimeError> {
        self.call_stateless_window_builtin(
            "linearreg_intercept",
            arity,
            args,
            pc,
            |buffer, window, pc| {
                calculate_linear_regression(buffer, window, RegressionOutput::Intercept, pc)
            },
        )
    }

    fn call_linearreg_slope(
        &mut self,
        arity: usize,
        args: Vec<Value>,
        pc: usize,
    ) -> Result<Value, RuntimeError> {
        self.call_stateless_window_builtin(
            "linearreg_slope",
            arity,
            args,
            pc,
            |buffer, window, pc| {
                calculate_linear_regression(buffer, window, RegressionOutput::Slope, pc)
            },
        )
    }

    fn call_tsf(
        &mut self,
        arity: usize,
        args: Vec<Value>,
        pc: usize,
    ) -> Result<Value, RuntimeError> {
        self.call_stateless_window_builtin("tsf", arity, args, pc, |buffer, window, pc| {
            calculate_linear_regression(buffer, window, RegressionOutput::Forecast, pc)
        })
    }

    fn call_beta(
        &mut self,
        arity: usize,
        args: Vec<Value>,
        pc: usize,
    ) -> Result<Value, RuntimeError> {
        self.call_stateless_double_window_builtin("beta", arity, args, pc, 1, calculate_beta)
    }

    fn call_correl(
        &mut self,
        arity: usize,
        args: Vec<Value>,
        pc: usize,
    ) -> Result<Value, RuntimeError> {
        self.call_stateless_double_window_builtin("correl", arity, args, pc, 0, calculate_correl)
    }

    fn call_aroon(
        &mut self,
        arity: usize,
        args: Vec<Value>,
        pc: usize,
    ) -> Result<Value, RuntimeError> {
        self.call_stateless_high_low_window_tuple_builtin(
            "aroon",
            arity,
            args,
            pc,
            1,
            calculate_aroon,
        )
    }

    fn call_aroonosc(
        &mut self,
        arity: usize,
        args: Vec<Value>,
        pc: usize,
    ) -> Result<Value, RuntimeError> {
        self.call_stateless_high_low_window_builtin(
            "aroonosc",
            arity,
            args,
            pc,
            1,
            calculate_aroonosc,
        )
    }

    fn call_bop(
        &mut self,
        arity: usize,
        args: Vec<Value>,
        pc: usize,
    ) -> Result<Value, RuntimeError> {
        if arity != 4 {
            return Err(RuntimeError::ArityMismatch {
                builtin: "bop",
                expected: 4,
                found: arity,
            });
        }
        let open_slot = series_ref(args[0].clone(), pc)?;
        let high_slot = series_ref(args[1].clone(), pc)?;
        let low_slot = series_ref(args[2].clone(), pc)?;
        let close_slot = series_ref(args[3].clone(), pc)?;
        let open = self
            .series_values
            .get(open_slot)
            .ok_or(RuntimeError::InvalidSeriesSlot { slot: open_slot })?;
        let high = self
            .series_values
            .get(high_slot)
            .ok_or(RuntimeError::InvalidSeriesSlot { slot: high_slot })?;
        let low = self
            .series_values
            .get(low_slot)
            .ok_or(RuntimeError::InvalidSeriesSlot { slot: low_slot })?;
        let close = self
            .series_values
            .get(close_slot)
            .ok_or(RuntimeError::InvalidSeriesSlot { slot: close_slot })?;
        calculate_bop(open, high, low, close, pc)
    }

    fn call_cci(
        &mut self,
        arity: usize,
        args: Vec<Value>,
        pc: usize,
    ) -> Result<Value, RuntimeError> {
        self.call_stateless_high_low_close_window_builtin("cci", arity, args, pc, 0, calculate_cci)
    }

    fn call_willr(
        &mut self,
        arity: usize,
        args: Vec<Value>,
        pc: usize,
    ) -> Result<Value, RuntimeError> {
        if arity != 4 {
            return Err(RuntimeError::ArityMismatch {
                builtin: "willr",
                expected: 4,
                found: arity,
            });
        }
        let high_slot = series_ref(args[0].clone(), pc)?;
        let low_slot = series_ref(args[1].clone(), pc)?;
        let close_slot = series_ref(args[2].clone(), pc)?;
        let window = expect_window(args[3].clone(), pc)?;
        self.consume_steps(window.max(1), pc)?;
        let high = self
            .series_values
            .get(high_slot)
            .ok_or(RuntimeError::InvalidSeriesSlot { slot: high_slot })?;
        let low = self
            .series_values
            .get(low_slot)
            .ok_or(RuntimeError::InvalidSeriesSlot { slot: low_slot })?;
        let close = self
            .series_values
            .get(close_slot)
            .ok_or(RuntimeError::InvalidSeriesSlot { slot: close_slot })?;
        calculate_willr(high, low, close, window, pc)
    }

    fn call_highest(
        &mut self,
        callsite: u16,
        arity: usize,
        args: Vec<Value>,
        pc: usize,
    ) -> Result<Value, RuntimeError> {
        self.call_extrema_stateful(callsite, arity, args, pc, BuiltinId::Highest)
    }

    fn call_lowest(
        &mut self,
        callsite: u16,
        arity: usize,
        args: Vec<Value>,
        pc: usize,
    ) -> Result<Value, RuntimeError> {
        self.call_extrema_stateful(callsite, arity, args, pc, BuiltinId::Lowest)
    }

    fn call_rising(
        &mut self,
        callsite: u16,
        arity: usize,
        args: Vec<Value>,
        pc: usize,
    ) -> Result<Value, RuntimeError> {
        self.call_extrema_stateful(callsite, arity, args, pc, BuiltinId::Rising)
    }

    fn call_falling(
        &mut self,
        callsite: u16,
        arity: usize,
        args: Vec<Value>,
        pc: usize,
    ) -> Result<Value, RuntimeError> {
        self.call_extrema_stateful(callsite, arity, args, pc, BuiltinId::Falling)
    }

    fn call_extrema_stateful(
        &mut self,
        callsite: u16,
        arity: usize,
        args: Vec<Value>,
        pc: usize,
        builtin: BuiltinId,
    ) -> Result<Value, RuntimeError> {
        if arity != 2 {
            return Err(RuntimeError::ArityMismatch {
                builtin: builtin.as_str(),
                expected: 2,
                found: arity,
            });
        }
        let series_slot = series_ref(args[0].clone(), pc)?;
        let window = expect_window(args[1].clone(), pc)?;
        let required_history = if matches!(builtin, BuiltinId::Rising | BuiltinId::Falling) {
            window + 1
        } else {
            window
        };
        self.consume_steps(required_history.max(1), pc)?;
        let buffer = self
            .series_values
            .get(series_slot)
            .ok_or(RuntimeError::InvalidSeriesSlot { slot: series_slot })?;
        let key = (builtin, callsite);
        let mut state = self
            .indicator_state
            .remove(&key)
            .unwrap_or_else(|| match builtin {
                BuiltinId::Highest => IndicatorState::Highest(HighestState::new(window)),
                BuiltinId::Lowest => IndicatorState::Lowest(LowestState::new(window)),
                BuiltinId::Rising => IndicatorState::Rising(RisingState::new(window)),
                BuiltinId::Falling => IndicatorState::Falling(FallingState::new(window)),
                _ => unreachable!(),
            });
        let result = match &mut state {
            IndicatorState::Highest(state) => state.update(buffer, pc)?,
            IndicatorState::Lowest(state) => state.update(buffer, pc)?,
            IndicatorState::Rising(state) => state.update(buffer, pc)?,
            IndicatorState::Falling(state) => state.update(buffer, pc)?,
            _ => unreachable!(),
        };
        self.indicator_state.insert(key, state);
        Ok(result)
    }

    fn call_barssince(
        &mut self,
        callsite: u16,
        arity: usize,
        args: Vec<Value>,
        pc: usize,
    ) -> Result<Value, RuntimeError> {
        if arity != 1 {
            return Err(RuntimeError::ArityMismatch {
                builtin: "barssince",
                expected: 1,
                found: arity,
            });
        }
        let condition_slot = series_ref(args[0].clone(), pc)?;
        let condition =
            self.series_values
                .get(condition_slot)
                .ok_or(RuntimeError::InvalidSeriesSlot {
                    slot: condition_slot,
                })?;
        let key = (BuiltinId::BarsSince, callsite);
        let mut state = self
            .indicator_state
            .remove(&key)
            .unwrap_or(IndicatorState::BarsSince(BarsSinceState::new()));
        let result = match &mut state {
            IndicatorState::BarsSince(state) => state.update(condition, pc)?,
            _ => unreachable!(),
        };
        self.indicator_state.insert(key, state);
        Ok(result)
    }

    fn call_valuewhen(
        &mut self,
        callsite: u16,
        arity: usize,
        args: Vec<Value>,
        pc: usize,
    ) -> Result<Value, RuntimeError> {
        if arity != 3 {
            return Err(RuntimeError::ArityMismatch {
                builtin: "valuewhen",
                expected: 3,
                found: arity,
            });
        }
        let condition_slot = series_ref(args[0].clone(), pc)?;
        let source_slot = series_ref(args[1].clone(), pc)?;
        let occurrence = expect_window(args[2].clone(), pc)?;
        let condition =
            self.series_values
                .get(condition_slot)
                .ok_or(RuntimeError::InvalidSeriesSlot {
                    slot: condition_slot,
                })?;
        let source = self
            .series_values
            .get(source_slot)
            .ok_or(RuntimeError::InvalidSeriesSlot { slot: source_slot })?;
        let key = (BuiltinId::ValueWhen, callsite);
        let mut state = self
            .indicator_state
            .remove(&key)
            .unwrap_or_else(|| IndicatorState::ValueWhen(ValueWhenState::new(occurrence)));
        let result = match &mut state {
            IndicatorState::ValueWhen(state) => state.update(condition, source, pc)?,
            _ => unreachable!(),
        };
        self.indicator_state.insert(key, state);
        Ok(result)
    }

    fn call_obv(
        &mut self,
        callsite: u16,
        arity: usize,
        args: Vec<Value>,
        pc: usize,
    ) -> Result<Value, RuntimeError> {
        if arity != 2 {
            return Err(RuntimeError::ArityMismatch {
                builtin: "obv",
                expected: 2,
                found: arity,
            });
        }
        let price_slot = series_ref(args[0].clone(), pc)?;
        let volume_slot = series_ref(args[1].clone(), pc)?;
        let price = self
            .series_values
            .get(price_slot)
            .ok_or(RuntimeError::InvalidSeriesSlot { slot: price_slot })?;
        let volume = self
            .series_values
            .get(volume_slot)
            .ok_or(RuntimeError::InvalidSeriesSlot { slot: volume_slot })?;
        let key = (BuiltinId::Obv, callsite);
        let mut state = self
            .indicator_state
            .remove(&key)
            .unwrap_or(IndicatorState::Obv(ObvState::new()));
        let result = match &mut state {
            IndicatorState::Obv(state) => state.update(price, volume, pc)?,
            _ => unreachable!(),
        };
        self.indicator_state.insert(key, state);
        Ok(result)
    }

    fn call_trange(
        &mut self,
        arity: usize,
        args: Vec<Value>,
        pc: usize,
    ) -> Result<Value, RuntimeError> {
        if arity != 3 {
            return Err(RuntimeError::ArityMismatch {
                builtin: "trange",
                expected: 3,
                found: arity,
            });
        }
        let high_slot = series_ref(args[0].clone(), pc)?;
        let low_slot = series_ref(args[1].clone(), pc)?;
        let close_slot = series_ref(args[2].clone(), pc)?;
        self.consume_steps(2, pc)?;
        let high = self
            .series_values
            .get(high_slot)
            .ok_or(RuntimeError::InvalidSeriesSlot { slot: high_slot })?;
        let low = self
            .series_values
            .get(low_slot)
            .ok_or(RuntimeError::InvalidSeriesSlot { slot: low_slot })?;
        let close = self
            .series_values
            .get(close_slot)
            .ok_or(RuntimeError::InvalidSeriesSlot { slot: close_slot })?;
        calculate_trange(high, low, close, pc)
    }

    fn call_stateless_window_builtin<F>(
        &mut self,
        builtin: &'static str,
        arity: usize,
        args: Vec<Value>,
        pc: usize,
        calculate: F,
    ) -> Result<Value, RuntimeError>
    where
        F: FnOnce(&SeriesBuffer, usize, usize) -> Result<Value, RuntimeError>,
    {
        if arity != 2 {
            return Err(RuntimeError::ArityMismatch {
                builtin,
                expected: 2,
                found: arity,
            });
        }
        let series_slot = series_ref(args[0].clone(), pc)?;
        let window = expect_window(args[1].clone(), pc)?;
        self.consume_steps(window, pc)?;
        let buffer = self
            .series_values
            .get(series_slot)
            .ok_or(RuntimeError::InvalidSeriesSlot { slot: series_slot })?;
        calculate(buffer, window, pc)
    }

    fn call_stateless_window_factor_builtin<F>(
        &mut self,
        builtin: &'static str,
        arity: usize,
        args: Vec<Value>,
        pc: usize,
        calculate: F,
    ) -> Result<Value, RuntimeError>
    where
        F: FnOnce(&SeriesBuffer, usize, f64, usize) -> Result<Value, RuntimeError>,
    {
        if arity != 3 {
            return Err(RuntimeError::ArityMismatch {
                builtin,
                expected: 3,
                found: arity,
            });
        }
        let series_slot = series_ref(args[0].clone(), pc)?;
        let window = expect_window(args[1].clone(), pc)?;
        let factor = expect_f64(args[2].clone(), pc)?;
        self.consume_steps(window.max(1), pc)?;
        let buffer = self
            .series_values
            .get(series_slot)
            .ok_or(RuntimeError::InvalidSeriesSlot { slot: series_slot })?;
        calculate(buffer, window, factor, pc)
    }

    fn call_stateless_double_window_builtin<F>(
        &mut self,
        builtin: &'static str,
        arity: usize,
        args: Vec<Value>,
        pc: usize,
        extra_history: usize,
        calculate: F,
    ) -> Result<Value, RuntimeError>
    where
        F: FnOnce(&SeriesBuffer, &SeriesBuffer, usize, usize) -> Result<Value, RuntimeError>,
    {
        if arity != 3 {
            return Err(RuntimeError::ArityMismatch {
                builtin,
                expected: 3,
                found: arity,
            });
        }
        let left_slot = series_ref(args[0].clone(), pc)?;
        let right_slot = series_ref(args[1].clone(), pc)?;
        let window = expect_window(args[2].clone(), pc)?;
        self.consume_steps((window + extra_history).max(1), pc)?;
        let left = self
            .series_values
            .get(left_slot)
            .ok_or(RuntimeError::InvalidSeriesSlot { slot: left_slot })?;
        let right = self
            .series_values
            .get(right_slot)
            .ok_or(RuntimeError::InvalidSeriesSlot { slot: right_slot })?;
        calculate(left, right, window, pc)
    }

    fn call_stateless_high_low_window_builtin<F>(
        &mut self,
        builtin: &'static str,
        arity: usize,
        args: Vec<Value>,
        pc: usize,
        extra_history: usize,
        calculate: F,
    ) -> Result<Value, RuntimeError>
    where
        F: FnOnce(&SeriesBuffer, &SeriesBuffer, usize, usize) -> Result<Value, RuntimeError>,
    {
        if arity != 3 {
            return Err(RuntimeError::ArityMismatch {
                builtin,
                expected: 3,
                found: arity,
            });
        }
        let high_slot = series_ref(args[0].clone(), pc)?;
        let low_slot = series_ref(args[1].clone(), pc)?;
        let window = expect_window(args[2].clone(), pc)?;
        self.consume_steps((window + extra_history).max(1), pc)?;
        let high = self
            .series_values
            .get(high_slot)
            .ok_or(RuntimeError::InvalidSeriesSlot { slot: high_slot })?;
        let low = self
            .series_values
            .get(low_slot)
            .ok_or(RuntimeError::InvalidSeriesSlot { slot: low_slot })?;
        calculate(high, low, window, pc)
    }

    fn call_stateless_high_low_window_tuple_builtin<F>(
        &mut self,
        builtin: &'static str,
        arity: usize,
        args: Vec<Value>,
        pc: usize,
        extra_history: usize,
        calculate: F,
    ) -> Result<Value, RuntimeError>
    where
        F: FnOnce(&SeriesBuffer, &SeriesBuffer, usize, usize) -> Result<Value, RuntimeError>,
    {
        self.call_stateless_high_low_window_builtin(
            builtin,
            arity,
            args,
            pc,
            extra_history,
            calculate,
        )
    }

    fn call_stateless_high_low_close_window_builtin<F>(
        &mut self,
        builtin: &'static str,
        arity: usize,
        args: Vec<Value>,
        pc: usize,
        extra_history: usize,
        calculate: F,
    ) -> Result<Value, RuntimeError>
    where
        F: FnOnce(
            &SeriesBuffer,
            &SeriesBuffer,
            &SeriesBuffer,
            usize,
            usize,
        ) -> Result<Value, RuntimeError>,
    {
        if arity != 4 {
            return Err(RuntimeError::ArityMismatch {
                builtin,
                expected: 4,
                found: arity,
            });
        }
        let high_slot = series_ref(args[0].clone(), pc)?;
        let low_slot = series_ref(args[1].clone(), pc)?;
        let close_slot = series_ref(args[2].clone(), pc)?;
        let window = expect_window(args[3].clone(), pc)?;
        self.consume_steps((window + extra_history).max(1), pc)?;
        let high = self
            .series_values
            .get(high_slot)
            .ok_or(RuntimeError::InvalidSeriesSlot { slot: high_slot })?;
        let low = self
            .series_values
            .get(low_slot)
            .ok_or(RuntimeError::InvalidSeriesSlot { slot: low_slot })?;
        let close = self
            .series_values
            .get(close_slot)
            .ok_or(RuntimeError::InvalidSeriesSlot { slot: close_slot })?;
        calculate(high, low, close, window, pc)
    }
}

fn expect_f64(value: Value, pc: usize) -> Result<f64, RuntimeError> {
    match value {
        Value::F64(value) => Ok(value),
        other => Err(RuntimeError::TypeMismatch {
            pc,
            expected: "f64",
            found: other.type_name(),
        }),
    }
}

fn expect_window(value: Value, pc: usize) -> Result<usize, RuntimeError> {
    let value = expect_f64(value, pc)?;
    Ok(value as usize)
}

fn expect_ma_type(value: Value, pc: usize) -> Result<MaType, RuntimeError> {
    match value {
        Value::MaType(value) => Ok(value),
        other => Err(RuntimeError::TypeMismatch {
            pc,
            expected: "ma-type",
            found: other.type_name(),
        }),
    }
}

fn expect_numeric_like(value: &Value, pc: usize) -> Result<Option<f64>, RuntimeError> {
    match value {
        Value::F64(value) => Ok(Some(*value)),
        Value::NA => Ok(None),
        other => Err(RuntimeError::TypeMismatch {
            pc,
            expected: "f64",
            found: other.type_name(),
        }),
    }
}

fn expect_buffer_f64(
    buffer: &SeriesBuffer,
    offset: usize,
    pc: usize,
) -> Result<Option<f64>, RuntimeError> {
    match buffer.get(offset) {
        Value::F64(value) => Ok(Some(value)),
        Value::NA => Ok(None),
        other => Err(RuntimeError::TypeMismatch {
            pc,
            expected: "f64",
            found: other.type_name(),
        }),
    }
}

fn logical_and(left: Value, right: Value, pc: usize) -> Result<Value, RuntimeError> {
    match left {
        Value::Bool(false) => match right {
            Value::Bool(_) | Value::NA => Ok(Value::Bool(false)),
            other => Err(RuntimeError::TypeMismatch {
                pc,
                expected: "bool",
                found: other.type_name(),
            }),
        },
        Value::Bool(true) => match right {
            Value::Bool(value) => Ok(Value::Bool(value)),
            Value::NA => Ok(Value::NA),
            other => Err(RuntimeError::TypeMismatch {
                pc,
                expected: "bool",
                found: other.type_name(),
            }),
        },
        Value::NA => match right {
            Value::Bool(false) => Ok(Value::Bool(false)),
            Value::Bool(true) | Value::NA => Ok(Value::NA),
            other => Err(RuntimeError::TypeMismatch {
                pc,
                expected: "bool",
                found: other.type_name(),
            }),
        },
        other => Err(RuntimeError::TypeMismatch {
            pc,
            expected: "bool",
            found: other.type_name(),
        }),
    }
}

fn logical_or(left: Value, right: Value, pc: usize) -> Result<Value, RuntimeError> {
    match left {
        Value::Bool(true) => match right {
            Value::Bool(_) | Value::NA => Ok(Value::Bool(true)),
            other => Err(RuntimeError::TypeMismatch {
                pc,
                expected: "bool",
                found: other.type_name(),
            }),
        },
        Value::Bool(false) => match right {
            Value::Bool(value) => Ok(Value::Bool(value)),
            Value::NA => Ok(Value::NA),
            other => Err(RuntimeError::TypeMismatch {
                pc,
                expected: "bool",
                found: other.type_name(),
            }),
        },
        Value::NA => match right {
            Value::Bool(true) => Ok(Value::Bool(true)),
            Value::Bool(false) | Value::NA => Ok(Value::NA),
            other => Err(RuntimeError::TypeMismatch {
                pc,
                expected: "bool",
                found: other.type_name(),
            }),
        },
        other => Err(RuntimeError::TypeMismatch {
            pc,
            expected: "bool",
            found: other.type_name(),
        }),
    }
}

fn eq_values(left: &Value, right: &Value) -> bool {
    match (left, right) {
        (Value::F64(left), Value::F64(right)) => left == right,
        (Value::Bool(left), Value::Bool(right)) => left == right,
        (Value::MaType(left), Value::MaType(right)) => left == right,
        (Value::Void, Value::Void) => true,
        (Value::SeriesRef(left), Value::SeriesRef(right)) => left == right,
        (Value::Tuple2(left), Value::Tuple2(right)) => {
            eq_values(&left[0], &right[0]) && eq_values(&left[1], &right[1])
        }
        (Value::Tuple3(left), Value::Tuple3(right)) => {
            eq_values(&left[0], &right[0])
                && eq_values(&left[1], &right[1])
                && eq_values(&left[2], &right[2])
        }
        _ => false,
    }
}
