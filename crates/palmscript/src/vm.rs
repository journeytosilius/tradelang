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
    calculate_bop, calculate_cci, calculate_correl, calculate_highest_bars, calculate_imi,
    calculate_linear_regression, calculate_lowest_bars, calculate_max_index, calculate_mfi,
    calculate_min_index, calculate_min_max, calculate_min_max_index, calculate_stddev,
    calculate_sum, calculate_trange, calculate_var, calculate_willr, calculate_wma, AccbandsState,
    AdOscState, AdState, AnchoredCountState, AnchoredExtremaMode, AnchoredExtremaState,
    AnchoredValueWhenState, BarsSinceState, BbandsState, BoolEdgeMode, BoolEdgeState, CmoState,
    CumState, DirectionalKind, DirectionalState, DmKind, DmState, EmaState, FallingState,
    HighestState, HtDcPeriodState, HtDcPhaseState, HtPhasorState, HtSineState, HtTrendModeState,
    HtTrendlineState, IndicatorState, LowestState, MacdExtState, MacdState, MamaState, MavpState,
    MovingAverageState, ObvState, OscillatorKind, PersistentState, PriceOscillatorState,
    RegressionOutput, RisingState, RsiState, SarConfig, SarState, SmaState, StochFastState,
    StochRsiState, StochState, TrixState, UnaryMathTransform, ValueWhenState,
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
            order_fields: Vec::new(),
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

    fn materialize_value(&self, value: Value, pc: usize) -> Result<Value, RuntimeError> {
        match value {
            Value::SeriesRef(slot) => self.load_series_value(slot, 0),
            other @ (Value::Tuple2(_) | Value::Tuple3(_) | Value::Void) => {
                Err(RuntimeError::TypeMismatch {
                    pc,
                    expected: "scalar-value",
                    found: other.type_name(),
                })
            }
            other => Ok(other),
        }
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
            BuiltinId::Activated => self.call_activated(callsite, arity, args, pc),
            BuiltinId::Deactivated => self.call_deactivated(callsite, arity, args, pc),
            BuiltinId::State => self.call_state_machine(callsite, arity, args, pc),
            BuiltinId::BarsSince => self.call_barssince(callsite, arity, args, pc),
            BuiltinId::ValueWhen => self.call_valuewhen(callsite, arity, args, pc),
            BuiltinId::HighestSince => {
                self.call_anchored_extrema(callsite, arity, args, pc, builtin)
            }
            BuiltinId::LowestSince => {
                self.call_anchored_extrema(callsite, arity, args, pc, builtin)
            }
            BuiltinId::HighestBarsSince => {
                self.call_anchored_extrema(callsite, arity, args, pc, builtin)
            }
            BuiltinId::LowestBarsSince => {
                self.call_anchored_extrema(callsite, arity, args, pc, builtin)
            }
            BuiltinId::ValueWhenSince => self.call_valuewhen_since(callsite, arity, args, pc),
            BuiltinId::CountSince => self.call_count_since(callsite, arity, args, pc),
            BuiltinId::Nz => self.call_nz(arity, args, pc),
            BuiltinId::NaFunc => self.call_na(arity, args, pc),
            BuiltinId::Coalesce => self.call_coalesce(arity, args, pc),
            BuiltinId::Cum => self.call_cum(callsite, arity, args, pc),
            BuiltinId::HighestBars => self.call_highest_bars(arity, args, pc),
            BuiltinId::LowestBars => self.call_lowest_bars(arity, args, pc),
            BuiltinId::Atr => self.call_directional(callsite, arity, args, pc, BuiltinId::Atr),
            BuiltinId::Natr => self.call_directional(callsite, arity, args, pc, BuiltinId::Natr),
            BuiltinId::PlusDm => self.call_dm(callsite, arity, args, pc, BuiltinId::PlusDm),
            BuiltinId::MinusDm => self.call_dm(callsite, arity, args, pc, BuiltinId::MinusDm),
            BuiltinId::PlusDi => {
                self.call_directional(callsite, arity, args, pc, BuiltinId::PlusDi)
            }
            BuiltinId::MinusDi => {
                self.call_directional(callsite, arity, args, pc, BuiltinId::MinusDi)
            }
            BuiltinId::Dx => self.call_directional(callsite, arity, args, pc, BuiltinId::Dx),
            BuiltinId::Adx => self.call_directional(callsite, arity, args, pc, BuiltinId::Adx),
            BuiltinId::Adxr => self.call_directional(callsite, arity, args, pc, BuiltinId::Adxr),
            BuiltinId::Ad => self.call_ad(callsite, arity, args, pc),
            BuiltinId::Adosc => self.call_adosc(callsite, arity, args, pc),
            BuiltinId::Mfi => self.call_mfi(arity, args, pc),
            BuiltinId::Imi => self.call_imi(arity, args, pc),
            BuiltinId::Macdfix => self.call_macdfix(callsite, arity, args, pc),
            BuiltinId::Bbands => self.call_bbands(callsite, arity, args, pc),
            BuiltinId::Dema => {
                self.call_moving_average_builtin(callsite, arity, args, pc, BuiltinId::Dema)
            }
            BuiltinId::Tema => {
                self.call_moving_average_builtin(callsite, arity, args, pc, BuiltinId::Tema)
            }
            BuiltinId::Trima => {
                self.call_moving_average_builtin(callsite, arity, args, pc, BuiltinId::Trima)
            }
            BuiltinId::Kama => {
                self.call_moving_average_builtin(callsite, arity, args, pc, BuiltinId::Kama)
            }
            BuiltinId::T3 => self.call_t3(callsite, arity, args, pc),
            BuiltinId::Trix => self.call_trix(callsite, arity, args, pc),
            BuiltinId::Accbands => self.call_accbands(callsite, arity, args, pc),
            BuiltinId::Macdext => self.call_macdext(callsite, arity, args, pc),
            BuiltinId::Mavp => self.call_mavp(callsite, arity, args, pc),
            BuiltinId::Sar => self.call_sar(callsite, arity, args, pc),
            BuiltinId::Sarext => self.call_sarext(callsite, arity, args, pc),
            BuiltinId::Stoch => self.call_stoch(callsite, arity, args, pc),
            BuiltinId::Stochf => self.call_stochf(callsite, arity, args, pc),
            BuiltinId::Stochrsi => self.call_stochrsi(callsite, arity, args, pc),
            BuiltinId::HtDcPeriod => self.call_ht_dcperiod(callsite, arity, args, pc),
            BuiltinId::HtDcPhase => self.call_ht_dcphase(callsite, arity, args, pc),
            BuiltinId::HtPhasor => self.call_ht_phasor(callsite, arity, args, pc),
            BuiltinId::HtSine => self.call_ht_sine(callsite, arity, args, pc),
            BuiltinId::HtTrendline => self.call_ht_trendline(callsite, arity, args, pc),
            BuiltinId::HtTrendmode => self.call_ht_trendmode(callsite, arity, args, pc),
            BuiltinId::Mama => self.call_mama(callsite, arity, args, pc),
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
        let series_slot = series_ref(args[0].clone(), pc)?;
        let window = expect_window(args[1].clone(), pc)?;
        let key = (BuiltinId::Ma, callsite);
        let mut state = self
            .indicator_state
            .remove(&key)
            .unwrap_or(IndicatorState::MovingAverage(Box::new(
                MovingAverageState::new(ma_type, window)?,
            )));
        let result = match &mut state {
            IndicatorState::MovingAverage(state) => {
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

    fn call_macdfix(
        &mut self,
        callsite: u16,
        arity: usize,
        args: Vec<Value>,
        pc: usize,
    ) -> Result<Value, RuntimeError> {
        if arity != 2 {
            return Err(RuntimeError::ArityMismatch {
                builtin: "macdfix",
                expected: 2,
                found: arity,
            });
        }
        let series_slot = series_ref(args[0].clone(), pc)?;
        let signal = expect_window(args[1].clone(), pc)?;
        let key = (BuiltinId::Macdfix, callsite);
        let mut state = self
            .indicator_state
            .remove(&key)
            .unwrap_or(IndicatorState::Macd(MacdState::new(12, 26, signal)));
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

    fn call_moving_average_builtin(
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
        let ma_type = match builtin {
            BuiltinId::Dema => MaType::Dema,
            BuiltinId::Tema => MaType::Tema,
            BuiltinId::Trima => MaType::Trima,
            BuiltinId::Kama => MaType::Kama,
            _ => unreachable!(),
        };
        let key = (builtin, callsite);
        let mut state = self
            .indicator_state
            .remove(&key)
            .unwrap_or(IndicatorState::MovingAverage(Box::new(
                MovingAverageState::new(ma_type, window)?,
            )));
        let result = match &mut state {
            IndicatorState::MovingAverage(state) => {
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

    fn call_t3(
        &mut self,
        callsite: u16,
        arity: usize,
        args: Vec<Value>,
        pc: usize,
    ) -> Result<Value, RuntimeError> {
        if arity != 3 {
            return Err(RuntimeError::ArityMismatch {
                builtin: "t3",
                expected: 3,
                found: arity,
            });
        }
        let series_slot = series_ref(args[0].clone(), pc)?;
        let window = expect_window(args[1].clone(), pc)?;
        let volume_factor = expect_f64(args[2].clone(), pc)?;
        let key = (BuiltinId::T3, callsite);
        let mut state = self
            .indicator_state
            .remove(&key)
            .unwrap_or(IndicatorState::T3(Box::new(
                crate::indicators::T3State::new(window, volume_factor),
            )));
        let result = match &mut state {
            IndicatorState::T3(state) => {
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

    fn call_trix(
        &mut self,
        callsite: u16,
        arity: usize,
        args: Vec<Value>,
        pc: usize,
    ) -> Result<Value, RuntimeError> {
        if arity != 2 {
            return Err(RuntimeError::ArityMismatch {
                builtin: "trix",
                expected: 2,
                found: arity,
            });
        }
        let series_slot = series_ref(args[0].clone(), pc)?;
        let window = expect_window(args[1].clone(), pc)?;
        let key = (BuiltinId::Trix, callsite);
        let mut state = self
            .indicator_state
            .remove(&key)
            .unwrap_or(IndicatorState::Trix(TrixState::new(window)));
        let result = match &mut state {
            IndicatorState::Trix(state) => {
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

    fn call_bbands(
        &mut self,
        callsite: u16,
        arity: usize,
        args: Vec<Value>,
        pc: usize,
    ) -> Result<Value, RuntimeError> {
        if arity != 5 {
            return Err(RuntimeError::ArityMismatch {
                builtin: "bbands",
                expected: 5,
                found: arity,
            });
        }
        let series_slot = series_ref(args[0].clone(), pc)?;
        let window = expect_window(args[1].clone(), pc)?;
        let deviations_up = expect_f64(args[2].clone(), pc)?;
        let deviations_down = expect_f64(args[3].clone(), pc)?;
        let ma_type = expect_ma_type(args[4].clone(), pc)?;
        let key = (BuiltinId::Bbands, callsite);
        let mut state = self
            .indicator_state
            .remove(&key)
            .unwrap_or(IndicatorState::Bbands(Box::new(BbandsState::new(
                window,
                deviations_up,
                deviations_down,
                ma_type,
            )?)));
        let result = match &mut state {
            IndicatorState::Bbands(state) => {
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

    fn call_accbands(
        &mut self,
        callsite: u16,
        arity: usize,
        args: Vec<Value>,
        pc: usize,
    ) -> Result<Value, RuntimeError> {
        if arity != 4 {
            return Err(RuntimeError::ArityMismatch {
                builtin: "accbands",
                expected: 4,
                found: arity,
            });
        }
        let high_slot = series_ref(args[0].clone(), pc)?;
        let low_slot = series_ref(args[1].clone(), pc)?;
        let close_slot = series_ref(args[2].clone(), pc)?;
        let window = expect_window(args[3].clone(), pc)?;
        let key = (BuiltinId::Accbands, callsite);
        let mut state = self
            .indicator_state
            .remove(&key)
            .unwrap_or(IndicatorState::Accbands(Box::new(AccbandsState::new(
                window,
            ))));
        let result = match &mut state {
            IndicatorState::Accbands(state) => {
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
                state.update(high, low, close, pc)?
            }
            _ => unreachable!(),
        };
        self.indicator_state.insert(key, state);
        Ok(result)
    }

    fn call_macdext(
        &mut self,
        callsite: u16,
        arity: usize,
        args: Vec<Value>,
        pc: usize,
    ) -> Result<Value, RuntimeError> {
        if arity != 7 {
            return Err(RuntimeError::ArityMismatch {
                builtin: "macdext",
                expected: 7,
                found: arity,
            });
        }
        let series_slot = series_ref(args[0].clone(), pc)?;
        let fast = expect_window(args[1].clone(), pc)?;
        let fast_ma = expect_ma_type(args[2].clone(), pc)?;
        let slow = expect_window(args[3].clone(), pc)?;
        let slow_ma = expect_ma_type(args[4].clone(), pc)?;
        let signal = expect_window(args[5].clone(), pc)?;
        let signal_ma = expect_ma_type(args[6].clone(), pc)?;
        let key = (BuiltinId::Macdext, callsite);
        let mut state = self
            .indicator_state
            .remove(&key)
            .unwrap_or(IndicatorState::MacdExt(Box::new(MacdExtState::new(
                fast, fast_ma, slow, slow_ma, signal, signal_ma,
            )?)));
        let result = match &mut state {
            IndicatorState::MacdExt(state) => {
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

    fn call_mavp(
        &mut self,
        callsite: u16,
        arity: usize,
        args: Vec<Value>,
        pc: usize,
    ) -> Result<Value, RuntimeError> {
        if arity != 5 {
            return Err(RuntimeError::ArityMismatch {
                builtin: "mavp",
                expected: 5,
                found: arity,
            });
        }
        let series_slot = series_ref(args[0].clone(), pc)?;
        let periods_slot = series_ref(args[1].clone(), pc)?;
        let min_period = expect_window(args[2].clone(), pc)?;
        let max_period = expect_window(args[3].clone(), pc)?;
        let ma_type = expect_ma_type(args[4].clone(), pc)?;
        let (min_period, max_period) = if max_period < min_period {
            (max_period, min_period)
        } else {
            (min_period, max_period)
        };
        let key = (BuiltinId::Mavp, callsite);
        let mut state = self
            .indicator_state
            .remove(&key)
            .unwrap_or(IndicatorState::Mavp(Box::new(MavpState::new(
                min_period, max_period, ma_type,
            )?)));
        let result = match &mut state {
            IndicatorState::Mavp(state) => {
                let prices = self
                    .series_values
                    .get(series_slot)
                    .ok_or(RuntimeError::InvalidSeriesSlot { slot: series_slot })?;
                let periods = self
                    .series_values
                    .get(periods_slot)
                    .ok_or(RuntimeError::InvalidSeriesSlot { slot: periods_slot })?;
                state.update(prices, periods, pc)?
            }
            _ => unreachable!(),
        };
        self.indicator_state.insert(key, state);
        Ok(result)
    }

    fn call_sar(
        &mut self,
        callsite: u16,
        arity: usize,
        args: Vec<Value>,
        pc: usize,
    ) -> Result<Value, RuntimeError> {
        if arity != 4 {
            return Err(RuntimeError::ArityMismatch {
                builtin: "sar",
                expected: 4,
                found: arity,
            });
        }
        let high_slot = series_ref(args[0].clone(), pc)?;
        let low_slot = series_ref(args[1].clone(), pc)?;
        let acceleration = expect_f64(args[2].clone(), pc)?;
        let maximum = expect_f64(args[3].clone(), pc)?;
        let key = (BuiltinId::Sar, callsite);
        let mut state = self
            .indicator_state
            .remove(&key)
            .unwrap_or(IndicatorState::Sar(Box::new(SarState::new(
                SarConfig::standard(acceleration, maximum),
            ))));
        let result = match &mut state {
            IndicatorState::Sar(state) => {
                let high = self
                    .series_values
                    .get(high_slot)
                    .ok_or(RuntimeError::InvalidSeriesSlot { slot: high_slot })?;
                let low = self
                    .series_values
                    .get(low_slot)
                    .ok_or(RuntimeError::InvalidSeriesSlot { slot: low_slot })?;
                state.update(high, low, pc)?
            }
            _ => unreachable!(),
        };
        self.indicator_state.insert(key, state);
        Ok(result)
    }

    fn call_sarext(
        &mut self,
        callsite: u16,
        arity: usize,
        args: Vec<Value>,
        pc: usize,
    ) -> Result<Value, RuntimeError> {
        if arity != 10 {
            return Err(RuntimeError::ArityMismatch {
                builtin: "sarext",
                expected: 10,
                found: arity,
            });
        }
        let high_slot = series_ref(args[0].clone(), pc)?;
        let low_slot = series_ref(args[1].clone(), pc)?;
        let key = (BuiltinId::Sarext, callsite);
        let mut state = self
            .indicator_state
            .remove(&key)
            .unwrap_or(IndicatorState::Sar(Box::new(SarState::new(SarConfig {
                start_value: expect_f64(args[2].clone(), pc)?,
                offset_on_reverse: expect_f64(args[3].clone(), pc)?,
                acceleration_init_long: expect_f64(args[4].clone(), pc)?,
                acceleration_long: expect_f64(args[5].clone(), pc)?,
                acceleration_max_long: expect_f64(args[6].clone(), pc)?,
                acceleration_init_short: expect_f64(args[7].clone(), pc)?,
                acceleration_short: expect_f64(args[8].clone(), pc)?,
                acceleration_max_short: expect_f64(args[9].clone(), pc)?,
                signed_short: true,
            }))));
        let result = match &mut state {
            IndicatorState::Sar(state) => {
                let high = self
                    .series_values
                    .get(high_slot)
                    .ok_or(RuntimeError::InvalidSeriesSlot { slot: high_slot })?;
                let low = self
                    .series_values
                    .get(low_slot)
                    .ok_or(RuntimeError::InvalidSeriesSlot { slot: low_slot })?;
                state.update(high, low, pc)?
            }
            _ => unreachable!(),
        };
        self.indicator_state.insert(key, state);
        Ok(result)
    }

    fn call_stoch(
        &mut self,
        callsite: u16,
        arity: usize,
        args: Vec<Value>,
        pc: usize,
    ) -> Result<Value, RuntimeError> {
        if arity != 8 {
            return Err(RuntimeError::ArityMismatch {
                builtin: "stoch",
                expected: 8,
                found: arity,
            });
        }
        let high_slot = series_ref(args[0].clone(), pc)?;
        let low_slot = series_ref(args[1].clone(), pc)?;
        let close_slot = series_ref(args[2].clone(), pc)?;
        let fast_k = expect_window(args[3].clone(), pc)?;
        let slow_k = expect_window(args[4].clone(), pc)?;
        let slow_k_ma = expect_ma_type(args[5].clone(), pc)?;
        let slow_d = expect_window(args[6].clone(), pc)?;
        let slow_d_ma = expect_ma_type(args[7].clone(), pc)?;
        let key = (BuiltinId::Stoch, callsite);
        let mut state = self
            .indicator_state
            .remove(&key)
            .unwrap_or(IndicatorState::Stoch(Box::new(StochState::new(
                fast_k, slow_k, slow_k_ma, slow_d, slow_d_ma,
            )?)));
        let result = match &mut state {
            IndicatorState::Stoch(state) => {
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
                state.update(high, low, close, pc)?
            }
            _ => unreachable!(),
        };
        self.indicator_state.insert(key, state);
        Ok(result)
    }

    fn call_stochf(
        &mut self,
        callsite: u16,
        arity: usize,
        args: Vec<Value>,
        pc: usize,
    ) -> Result<Value, RuntimeError> {
        if arity != 6 {
            return Err(RuntimeError::ArityMismatch {
                builtin: "stochf",
                expected: 6,
                found: arity,
            });
        }
        let high_slot = series_ref(args[0].clone(), pc)?;
        let low_slot = series_ref(args[1].clone(), pc)?;
        let close_slot = series_ref(args[2].clone(), pc)?;
        let fast_k = expect_window(args[3].clone(), pc)?;
        let fast_d = expect_window(args[4].clone(), pc)?;
        let fast_d_ma = expect_ma_type(args[5].clone(), pc)?;
        let key = (BuiltinId::Stochf, callsite);
        let mut state = self
            .indicator_state
            .remove(&key)
            .unwrap_or(IndicatorState::StochFast(Box::new(StochFastState::new(
                fast_k, fast_d, fast_d_ma,
            )?)));
        let result = match &mut state {
            IndicatorState::StochFast(state) => {
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
                state.update(high, low, close, pc)?
            }
            _ => unreachable!(),
        };
        self.indicator_state.insert(key, state);
        Ok(result)
    }

    fn call_stochrsi(
        &mut self,
        callsite: u16,
        arity: usize,
        args: Vec<Value>,
        pc: usize,
    ) -> Result<Value, RuntimeError> {
        if arity != 5 {
            return Err(RuntimeError::ArityMismatch {
                builtin: "stochrsi",
                expected: 5,
                found: arity,
            });
        }
        let series_slot = series_ref(args[0].clone(), pc)?;
        let time_period = expect_window(args[1].clone(), pc)?;
        let fast_k = expect_window(args[2].clone(), pc)?;
        let fast_d = expect_window(args[3].clone(), pc)?;
        let fast_d_ma = expect_ma_type(args[4].clone(), pc)?;
        let key = (BuiltinId::Stochrsi, callsite);
        let mut state = self
            .indicator_state
            .remove(&key)
            .unwrap_or(IndicatorState::StochRsi(Box::new(StochRsiState::new(
                time_period,
                fast_k,
                fast_d,
                fast_d_ma,
            )?)));
        let result = match &mut state {
            IndicatorState::StochRsi(state) => {
                let series = self
                    .series_values
                    .get(series_slot)
                    .ok_or(RuntimeError::InvalidSeriesSlot { slot: series_slot })?;
                state.update(series, pc)?
            }
            _ => unreachable!(),
        };
        self.indicator_state.insert(key, state);
        Ok(result)
    }

    fn call_ht_dcperiod(
        &mut self,
        callsite: u16,
        arity: usize,
        args: Vec<Value>,
        pc: usize,
    ) -> Result<Value, RuntimeError> {
        if arity != 1 {
            return Err(RuntimeError::ArityMismatch {
                builtin: "ht_dcperiod",
                expected: 1,
                found: arity,
            });
        }
        let series_slot = series_ref(args[0].clone(), pc)?;
        let key = (BuiltinId::HtDcPeriod, callsite);
        let mut state = self
            .indicator_state
            .remove(&key)
            .unwrap_or(IndicatorState::HtDcPeriod(Box::new(HtDcPeriodState::new())));
        let result = match &mut state {
            IndicatorState::HtDcPeriod(state) => {
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

    fn call_ht_dcphase(
        &mut self,
        callsite: u16,
        arity: usize,
        args: Vec<Value>,
        pc: usize,
    ) -> Result<Value, RuntimeError> {
        if arity != 1 {
            return Err(RuntimeError::ArityMismatch {
                builtin: "ht_dcphase",
                expected: 1,
                found: arity,
            });
        }
        let series_slot = series_ref(args[0].clone(), pc)?;
        let key = (BuiltinId::HtDcPhase, callsite);
        let mut state = self
            .indicator_state
            .remove(&key)
            .unwrap_or(IndicatorState::HtDcPhase(Box::new(HtDcPhaseState::new())));
        let result = match &mut state {
            IndicatorState::HtDcPhase(state) => {
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

    fn call_ht_phasor(
        &mut self,
        callsite: u16,
        arity: usize,
        args: Vec<Value>,
        pc: usize,
    ) -> Result<Value, RuntimeError> {
        if arity != 1 {
            return Err(RuntimeError::ArityMismatch {
                builtin: "ht_phasor",
                expected: 1,
                found: arity,
            });
        }
        let series_slot = series_ref(args[0].clone(), pc)?;
        let key = (BuiltinId::HtPhasor, callsite);
        let mut state = self
            .indicator_state
            .remove(&key)
            .unwrap_or(IndicatorState::HtPhasor(Box::new(HtPhasorState::new())));
        let result = match &mut state {
            IndicatorState::HtPhasor(state) => {
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

    fn call_ht_sine(
        &mut self,
        callsite: u16,
        arity: usize,
        args: Vec<Value>,
        pc: usize,
    ) -> Result<Value, RuntimeError> {
        if arity != 1 {
            return Err(RuntimeError::ArityMismatch {
                builtin: "ht_sine",
                expected: 1,
                found: arity,
            });
        }
        let series_slot = series_ref(args[0].clone(), pc)?;
        let key = (BuiltinId::HtSine, callsite);
        let mut state = self
            .indicator_state
            .remove(&key)
            .unwrap_or(IndicatorState::HtSine(Box::new(HtSineState::new())));
        let result = match &mut state {
            IndicatorState::HtSine(state) => {
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

    fn call_ht_trendline(
        &mut self,
        callsite: u16,
        arity: usize,
        args: Vec<Value>,
        pc: usize,
    ) -> Result<Value, RuntimeError> {
        if arity != 1 {
            return Err(RuntimeError::ArityMismatch {
                builtin: "ht_trendline",
                expected: 1,
                found: arity,
            });
        }
        let series_slot = series_ref(args[0].clone(), pc)?;
        let key = (BuiltinId::HtTrendline, callsite);
        let mut state = self
            .indicator_state
            .remove(&key)
            .unwrap_or(IndicatorState::HtTrendline(Box::new(
                HtTrendlineState::new(),
            )));
        let result = match &mut state {
            IndicatorState::HtTrendline(state) => {
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

    fn call_ht_trendmode(
        &mut self,
        callsite: u16,
        arity: usize,
        args: Vec<Value>,
        pc: usize,
    ) -> Result<Value, RuntimeError> {
        if arity != 1 {
            return Err(RuntimeError::ArityMismatch {
                builtin: "ht_trendmode",
                expected: 1,
                found: arity,
            });
        }
        let series_slot = series_ref(args[0].clone(), pc)?;
        let key = (BuiltinId::HtTrendmode, callsite);
        let mut state = self
            .indicator_state
            .remove(&key)
            .unwrap_or(IndicatorState::HtTrendMode(Box::new(
                HtTrendModeState::new(),
            )));
        let result = match &mut state {
            IndicatorState::HtTrendMode(state) => {
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

    fn call_mama(
        &mut self,
        callsite: u16,
        arity: usize,
        args: Vec<Value>,
        pc: usize,
    ) -> Result<Value, RuntimeError> {
        if arity != 3 {
            return Err(RuntimeError::ArityMismatch {
                builtin: "mama",
                expected: 3,
                found: arity,
            });
        }
        let series_slot = series_ref(args[0].clone(), pc)?;
        let fast_limit = expect_f64(args[1].clone(), pc)?;
        let slow_limit = expect_f64(args[2].clone(), pc)?;
        let key = (BuiltinId::Mama, callsite);
        let mut state = self
            .indicator_state
            .remove(&key)
            .unwrap_or(IndicatorState::Mama(Box::new(MamaState::new(
                fast_limit, slow_limit,
            ))));
        let result = match &mut state {
            IndicatorState::Mama(state) => {
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
        if matches!(ma_type, MaType::Mama) {
            return Err(RuntimeError::UnsupportedMaType {
                builtin: builtin_name,
                ma_type: ma_type.as_str(),
            });
        }
        let key = (builtin_id, callsite);
        let mut state =
            self.indicator_state
                .remove(&key)
                .unwrap_or(IndicatorState::PriceOscillator(Box::new(
                    PriceOscillatorState::new(builtin_name, fast, slow, ma_type, kind),
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

    fn call_activated(
        &mut self,
        callsite: u16,
        arity: usize,
        args: Vec<Value>,
        pc: usize,
    ) -> Result<Value, RuntimeError> {
        self.call_bool_edge(callsite, arity, args, pc, BuiltinId::Activated)
    }

    fn call_deactivated(
        &mut self,
        callsite: u16,
        arity: usize,
        args: Vec<Value>,
        pc: usize,
    ) -> Result<Value, RuntimeError> {
        self.call_bool_edge(callsite, arity, args, pc, BuiltinId::Deactivated)
    }

    fn call_bool_edge(
        &mut self,
        callsite: u16,
        arity: usize,
        args: Vec<Value>,
        pc: usize,
        builtin: BuiltinId,
    ) -> Result<Value, RuntimeError> {
        if arity != 1 {
            return Err(RuntimeError::ArityMismatch {
                builtin: builtin.as_str(),
                expected: 1,
                found: arity,
            });
        }
        let condition_slot = series_ref(args[0].clone(), pc)?;
        self.consume_steps(2, pc)?;
        let condition =
            self.series_values
                .get(condition_slot)
                .ok_or(RuntimeError::InvalidSeriesSlot {
                    slot: condition_slot,
                })?;
        let key = (builtin, callsite);
        let mut state = self.indicator_state.remove(&key).unwrap_or_else(|| {
            let mode = match builtin {
                BuiltinId::Activated => BoolEdgeMode::Activated,
                BuiltinId::Deactivated => BoolEdgeMode::Deactivated,
                _ => unreachable!(),
            };
            IndicatorState::BoolEdge(BoolEdgeState::new(mode))
        });
        let result = match &mut state {
            IndicatorState::BoolEdge(state) => state.update(condition, pc)?,
            _ => unreachable!(),
        };
        self.indicator_state.insert(key, state);
        Ok(result)
    }

    fn call_state_machine(
        &mut self,
        callsite: u16,
        arity: usize,
        args: Vec<Value>,
        pc: usize,
    ) -> Result<Value, RuntimeError> {
        if arity != 2 {
            return Err(RuntimeError::ArityMismatch {
                builtin: "state",
                expected: 2,
                found: arity,
            });
        }
        let enter_slot = series_ref(args[0].clone(), pc)?;
        let exit_slot = series_ref(args[1].clone(), pc)?;
        self.consume_steps(2, pc)?;
        let enter = self
            .series_values
            .get(enter_slot)
            .ok_or(RuntimeError::InvalidSeriesSlot { slot: enter_slot })?;
        let exit = self
            .series_values
            .get(exit_slot)
            .ok_or(RuntimeError::InvalidSeriesSlot { slot: exit_slot })?;
        let key = (BuiltinId::State, callsite);
        let mut state = self
            .indicator_state
            .remove(&key)
            .unwrap_or(IndicatorState::StateMachine(PersistentState::new()));
        let result = match &mut state {
            IndicatorState::StateMachine(state) => state.update(enter, exit, pc)?,
            _ => unreachable!(),
        };
        self.indicator_state.insert(key, state);
        Ok(result)
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

    fn call_nz(
        &mut self,
        arity: usize,
        args: Vec<Value>,
        pc: usize,
    ) -> Result<Value, RuntimeError> {
        if arity != 2 {
            return Err(RuntimeError::ArityMismatch {
                builtin: "nz",
                expected: 2,
                found: arity,
            });
        }
        self.call_coalesce(2, args, pc)
    }

    fn call_na(
        &mut self,
        arity: usize,
        args: Vec<Value>,
        pc: usize,
    ) -> Result<Value, RuntimeError> {
        if arity != 1 {
            return Err(RuntimeError::ArityMismatch {
                builtin: "na",
                expected: 1,
                found: arity,
            });
        }
        let value = self.materialize_value(args.into_iter().next().unwrap_or(Value::NA), pc)?;
        Ok(Value::Bool(matches!(value, Value::NA)))
    }

    fn call_coalesce(
        &mut self,
        arity: usize,
        args: Vec<Value>,
        pc: usize,
    ) -> Result<Value, RuntimeError> {
        if arity != 2 {
            return Err(RuntimeError::ArityMismatch {
                builtin: "coalesce",
                expected: 2,
                found: arity,
            });
        }
        let left = self.materialize_value(args[0].clone(), pc)?;
        if !matches!(left, Value::NA) {
            return Ok(left);
        }
        self.materialize_value(args[1].clone(), pc)
    }

    fn call_cum(
        &mut self,
        callsite: u16,
        arity: usize,
        args: Vec<Value>,
        pc: usize,
    ) -> Result<Value, RuntimeError> {
        if arity != 1 {
            return Err(RuntimeError::ArityMismatch {
                builtin: "cum",
                expected: 1,
                found: arity,
            });
        }
        let key = (BuiltinId::Cum, callsite);
        let mut state = self
            .indicator_state
            .remove(&key)
            .unwrap_or(IndicatorState::Cum(CumState::new()));
        let input = self.materialize_value(args.into_iter().next().unwrap_or(Value::NA), pc)?;
        let result = match &mut state {
            IndicatorState::Cum(state) => state.update(input, pc)?,
            _ => unreachable!(),
        };
        self.indicator_state.insert(key, state);
        Ok(result)
    }

    fn call_highest_bars(
        &mut self,
        arity: usize,
        args: Vec<Value>,
        pc: usize,
    ) -> Result<Value, RuntimeError> {
        self.call_extrema_bar_offset(arity, args, pc, BuiltinId::HighestBars)
    }

    fn call_lowest_bars(
        &mut self,
        arity: usize,
        args: Vec<Value>,
        pc: usize,
    ) -> Result<Value, RuntimeError> {
        self.call_extrema_bar_offset(arity, args, pc, BuiltinId::LowestBars)
    }

    fn call_extrema_bar_offset(
        &mut self,
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
        self.consume_steps(window.max(1), pc)?;
        let buffer = self
            .series_values
            .get(series_slot)
            .ok_or(RuntimeError::InvalidSeriesSlot { slot: series_slot })?;
        match builtin {
            BuiltinId::HighestBars => calculate_highest_bars(buffer, window, pc),
            BuiltinId::LowestBars => calculate_lowest_bars(buffer, window, pc),
            _ => unreachable!(),
        }
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

    fn call_anchored_extrema(
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
        let anchor_slot = series_ref(args[0].clone(), pc)?;
        let source_slot = series_ref(args[1].clone(), pc)?;
        let anchor = self
            .series_values
            .get(anchor_slot)
            .ok_or(RuntimeError::InvalidSeriesSlot { slot: anchor_slot })?;
        let source = self
            .series_values
            .get(source_slot)
            .ok_or(RuntimeError::InvalidSeriesSlot { slot: source_slot })?;
        let key = (builtin, callsite);
        let mut state = self.indicator_state.remove(&key).unwrap_or_else(|| {
            let mode = match builtin {
                BuiltinId::HighestSince | BuiltinId::HighestBarsSince => {
                    AnchoredExtremaMode::Highest
                }
                BuiltinId::LowestSince | BuiltinId::LowestBarsSince => AnchoredExtremaMode::Lowest,
                _ => unreachable!(),
            };
            IndicatorState::AnchoredExtrema(AnchoredExtremaState::new(mode))
        });
        let result = match &mut state {
            IndicatorState::AnchoredExtrema(state) => match builtin {
                BuiltinId::HighestSince | BuiltinId::LowestSince => {
                    state.update_value(anchor, source, pc)?
                }
                BuiltinId::HighestBarsSince | BuiltinId::LowestBarsSince => {
                    state.update_offset(anchor, source, pc)?
                }
                _ => unreachable!(),
            },
            _ => unreachable!(),
        };
        self.indicator_state.insert(key, state);
        Ok(result)
    }

    fn call_valuewhen_since(
        &mut self,
        callsite: u16,
        arity: usize,
        args: Vec<Value>,
        pc: usize,
    ) -> Result<Value, RuntimeError> {
        if arity != 4 {
            return Err(RuntimeError::ArityMismatch {
                builtin: "valuewhen_since",
                expected: 4,
                found: arity,
            });
        }
        let anchor_slot = series_ref(args[0].clone(), pc)?;
        let condition_slot = series_ref(args[1].clone(), pc)?;
        let source_slot = series_ref(args[2].clone(), pc)?;
        let occurrence = expect_window(args[3].clone(), pc)?;
        let anchor = self
            .series_values
            .get(anchor_slot)
            .ok_or(RuntimeError::InvalidSeriesSlot { slot: anchor_slot })?;
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
        let key = (BuiltinId::ValueWhenSince, callsite);
        let mut state = self.indicator_state.remove(&key).unwrap_or_else(|| {
            IndicatorState::AnchoredValueWhen(AnchoredValueWhenState::new(occurrence))
        });
        let result = match &mut state {
            IndicatorState::AnchoredValueWhen(state) => {
                state.update(anchor, condition, source, pc)?
            }
            _ => unreachable!(),
        };
        self.indicator_state.insert(key, state);
        Ok(result)
    }

    fn call_count_since(
        &mut self,
        callsite: u16,
        arity: usize,
        args: Vec<Value>,
        pc: usize,
    ) -> Result<Value, RuntimeError> {
        if arity != 2 {
            return Err(RuntimeError::ArityMismatch {
                builtin: "count_since",
                expected: 2,
                found: arity,
            });
        }
        let anchor_slot = series_ref(args[0].clone(), pc)?;
        let condition_slot = series_ref(args[1].clone(), pc)?;
        let anchor = self
            .series_values
            .get(anchor_slot)
            .ok_or(RuntimeError::InvalidSeriesSlot { slot: anchor_slot })?;
        let condition =
            self.series_values
                .get(condition_slot)
                .ok_or(RuntimeError::InvalidSeriesSlot {
                    slot: condition_slot,
                })?;
        let key = (BuiltinId::CountSince, callsite);
        let mut state = self
            .indicator_state
            .remove(&key)
            .unwrap_or(IndicatorState::AnchoredCount(AnchoredCountState::new()));
        let result = match &mut state {
            IndicatorState::AnchoredCount(state) => state.update(anchor, condition, pc)?,
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

    fn call_ad(
        &mut self,
        callsite: u16,
        arity: usize,
        args: Vec<Value>,
        pc: usize,
    ) -> Result<Value, RuntimeError> {
        if arity != 4 {
            return Err(RuntimeError::ArityMismatch {
                builtin: "ad",
                expected: 4,
                found: arity,
            });
        }
        let high_slot = series_ref(args[0].clone(), pc)?;
        let low_slot = series_ref(args[1].clone(), pc)?;
        let close_slot = series_ref(args[2].clone(), pc)?;
        let volume_slot = series_ref(args[3].clone(), pc)?;
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
        let volume = self
            .series_values
            .get(volume_slot)
            .ok_or(RuntimeError::InvalidSeriesSlot { slot: volume_slot })?;
        let key = (BuiltinId::Ad, callsite);
        let mut state = self
            .indicator_state
            .remove(&key)
            .unwrap_or(IndicatorState::Ad(AdState::new()));
        let result = match &mut state {
            IndicatorState::Ad(state) => state.update(high, low, close, volume, pc)?,
            _ => unreachable!(),
        };
        self.indicator_state.insert(key, state);
        Ok(result)
    }

    fn call_adosc(
        &mut self,
        callsite: u16,
        arity: usize,
        args: Vec<Value>,
        pc: usize,
    ) -> Result<Value, RuntimeError> {
        if arity != 6 {
            return Err(RuntimeError::ArityMismatch {
                builtin: "adosc",
                expected: 6,
                found: arity,
            });
        }
        let high_slot = series_ref(args[0].clone(), pc)?;
        let low_slot = series_ref(args[1].clone(), pc)?;
        let close_slot = series_ref(args[2].clone(), pc)?;
        let volume_slot = series_ref(args[3].clone(), pc)?;
        let fast = expect_window(args[4].clone(), pc)?;
        let slow = expect_window(args[5].clone(), pc)?;
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
        let volume = self
            .series_values
            .get(volume_slot)
            .ok_or(RuntimeError::InvalidSeriesSlot { slot: volume_slot })?;
        let key = (BuiltinId::Adosc, callsite);
        let mut state = self
            .indicator_state
            .remove(&key)
            .unwrap_or(IndicatorState::AdOsc(AdOscState::new(fast, slow)));
        let result = match &mut state {
            IndicatorState::AdOsc(state) => state.update(high, low, close, volume, pc)?,
            _ => unreachable!(),
        };
        self.indicator_state.insert(key, state);
        Ok(result)
    }

    fn call_mfi(
        &mut self,
        arity: usize,
        args: Vec<Value>,
        pc: usize,
    ) -> Result<Value, RuntimeError> {
        if arity != 5 {
            return Err(RuntimeError::ArityMismatch {
                builtin: "mfi",
                expected: 5,
                found: arity,
            });
        }
        let high_slot = series_ref(args[0].clone(), pc)?;
        let low_slot = series_ref(args[1].clone(), pc)?;
        let close_slot = series_ref(args[2].clone(), pc)?;
        let volume_slot = series_ref(args[3].clone(), pc)?;
        let window = expect_window(args[4].clone(), pc)?;
        self.consume_steps(window + 1, pc)?;
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
        let volume = self
            .series_values
            .get(volume_slot)
            .ok_or(RuntimeError::InvalidSeriesSlot { slot: volume_slot })?;
        calculate_mfi(high, low, close, volume, window, pc)
    }

    fn call_imi(
        &mut self,
        arity: usize,
        args: Vec<Value>,
        pc: usize,
    ) -> Result<Value, RuntimeError> {
        if arity != 3 {
            return Err(RuntimeError::ArityMismatch {
                builtin: "imi",
                expected: 3,
                found: arity,
            });
        }
        let open_slot = series_ref(args[0].clone(), pc)?;
        let close_slot = series_ref(args[1].clone(), pc)?;
        let window = expect_window(args[2].clone(), pc)?;
        self.consume_steps(window.max(1), pc)?;
        let open = self
            .series_values
            .get(open_slot)
            .ok_or(RuntimeError::InvalidSeriesSlot { slot: open_slot })?;
        let close = self
            .series_values
            .get(close_slot)
            .ok_or(RuntimeError::InvalidSeriesSlot { slot: close_slot })?;
        calculate_imi(open, close, window, pc)
    }

    fn call_dm(
        &mut self,
        callsite: u16,
        arity: usize,
        args: Vec<Value>,
        pc: usize,
        builtin: BuiltinId,
    ) -> Result<Value, RuntimeError> {
        if arity != 3 {
            return Err(RuntimeError::ArityMismatch {
                builtin: builtin.as_str(),
                expected: 3,
                found: arity,
            });
        }
        let high_slot = series_ref(args[0].clone(), pc)?;
        let low_slot = series_ref(args[1].clone(), pc)?;
        let window = expect_window(args[2].clone(), pc)?;
        let kind = if matches!(builtin, BuiltinId::PlusDm) {
            DmKind::Plus
        } else {
            DmKind::Minus
        };
        let key = (builtin, callsite);
        let mut state = self
            .indicator_state
            .remove(&key)
            .unwrap_or(IndicatorState::Dm(DmState::new(window, kind)));
        let result = match &mut state {
            IndicatorState::Dm(state) => {
                let high = self
                    .series_values
                    .get(high_slot)
                    .ok_or(RuntimeError::InvalidSeriesSlot { slot: high_slot })?;
                let low = self
                    .series_values
                    .get(low_slot)
                    .ok_or(RuntimeError::InvalidSeriesSlot { slot: low_slot })?;
                state.update(high, low, pc)?
            }
            _ => unreachable!(),
        };
        self.indicator_state.insert(key, state);
        Ok(result)
    }

    fn call_directional(
        &mut self,
        callsite: u16,
        arity: usize,
        args: Vec<Value>,
        pc: usize,
        builtin: BuiltinId,
    ) -> Result<Value, RuntimeError> {
        if arity != 4 {
            return Err(RuntimeError::ArityMismatch {
                builtin: builtin.as_str(),
                expected: 4,
                found: arity,
            });
        }
        let high_slot = series_ref(args[0].clone(), pc)?;
        let low_slot = series_ref(args[1].clone(), pc)?;
        let close_slot = series_ref(args[2].clone(), pc)?;
        let window = expect_window(args[3].clone(), pc)?;
        let kind = match builtin {
            BuiltinId::Atr => DirectionalKind::Atr,
            BuiltinId::Natr => DirectionalKind::Natr,
            BuiltinId::PlusDi => DirectionalKind::PlusDi,
            BuiltinId::MinusDi => DirectionalKind::MinusDi,
            BuiltinId::Dx => DirectionalKind::Dx,
            BuiltinId::Adx => DirectionalKind::Adx,
            BuiltinId::Adxr => DirectionalKind::Adxr,
            _ => unreachable!(),
        };
        let key = (builtin, callsite);
        let mut state = self
            .indicator_state
            .remove(&key)
            .unwrap_or(IndicatorState::Directional(DirectionalState::new(
                window, kind,
            )));
        let result = match &mut state {
            IndicatorState::Directional(state) => {
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
                state.update(high, low, close, pc)?
            }
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
        (Value::TimeInForce(left), Value::TimeInForce(right)) => left == right,
        (Value::TriggerReference(left), Value::TriggerReference(right)) => left == right,
        (Value::PositionSide(left), Value::PositionSide(right)) => left == right,
        (Value::ExitKind(left), Value::ExitKind(right)) => left == right,
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
