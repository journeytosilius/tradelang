//! Low-level TradeLang bytecode interpreter.
//!
//! This module contains the bounded series buffer, the per-bar VM execution
//! engine, and builtin dispatch used by the runtime hot path.

use std::collections::{HashMap, VecDeque};

use crate::builtins::BuiltinId;
use crate::bytecode::{Constant, Instruction, OpCode, Program};
use crate::diagnostic::RuntimeError;
use crate::indicators::{sma, EmaState, IndicatorState, RsiState};
use crate::output::{PlotPoint, StepOutput};
use crate::runtime::Bar;
use crate::types::{SlotKind, Value};

#[derive(Clone, Debug)]
pub struct SeriesBuffer {
    capacity: usize,
    values: VecDeque<Value>,
}

impl SeriesBuffer {
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity,
            values: VecDeque::with_capacity(capacity),
        }
    }

    pub fn push(&mut self, value: Value) {
        if self.values.len() == self.capacity {
            self.values.pop_front();
        }
        self.values.push_back(value);
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
                OpCode::Return => {
                    break;
                }
            }
            pc += 1;
        }

        Ok(StepOutput { plots, alerts })
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
        if matches!(local.kind, SlotKind::Series) {
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
            _ => Err(RuntimeError::UnknownBuiltin { builtin_id }),
        }
    }

    fn call_sma(
        &mut self,
        _callsite: u16,
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
        sma::calculate(buffer, window, pc)
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
        let current_price = self.load_series_value(series_slot, 0)?;
        if current_price.is_na() {
            return Ok(Value::NA);
        }
        let current_price = expect_f64(current_price, pc)?;
        let key = (BuiltinId::Ema, callsite);
        let mut state = self
            .indicator_state
            .remove(&key)
            .unwrap_or(IndicatorState::Ema(EmaState::new(window)));
        let result = match &mut state {
            IndicatorState::Ema(state) => {
                if !state.is_seeded() {
                    self.consume_steps(state.seed_window(), pc)?;
                }
                let buffer = self
                    .series_values
                    .get(series_slot)
                    .ok_or(RuntimeError::InvalidSeriesSlot { slot: series_slot })?;
                state.update(current_price, buffer, pc)?
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
        let current_price = self.load_series_value(series_slot, 0)?;
        if current_price.is_na() {
            return Ok(Value::NA);
        }
        let current_price = expect_f64(current_price, pc)?;
        let key = (BuiltinId::Rsi, callsite);
        let mut state = self
            .indicator_state
            .remove(&key)
            .unwrap_or(IndicatorState::Rsi(RsiState::new(window)));
        let result = match &mut state {
            IndicatorState::Rsi(state) => {
                if state.requires_seed_step() {
                    self.consume_steps(1, pc)?;
                }
                state.update(current_price)
            }
            _ => unreachable!(),
        };
        self.indicator_state.insert(key, state);
        Ok(result)
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
        (Value::Void, Value::Void) => true,
        (Value::SeriesRef(left), Value::SeriesRef(right)) => left == right,
        _ => false,
    }
}
