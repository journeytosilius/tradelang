//! Indicator implementations and runtime state used by builtin execution.
//!
//! Indicator math lives here so the VM keeps direct opcode dispatch while the
//! per-indicator logic stays modular and independently testable.

pub(crate) mod advanced_ma;
pub(crate) mod cmo;
pub(crate) mod directional;
pub(crate) mod ema;
pub(crate) mod event;
pub(crate) mod extrema;
pub(crate) mod macd;
pub(crate) mod math;
pub(crate) mod momentum;
pub(crate) mod oscillator;
pub(crate) mod rsi;
pub(crate) mod sma;
pub(crate) mod statistics;
pub(crate) mod volatility;
pub(crate) mod volume;
pub(crate) mod wma;

pub(crate) use advanced_ma::{BbandsState, MovingAverageState, T3State, TrixState};
pub(crate) use cmo::CmoState;
pub(crate) use directional::{DirectionalKind, DirectionalState, DmKind, DmState};
pub(crate) use ema::EmaState;
pub(crate) use event::{BarsSinceState, CumState, ValueWhenState};
pub(crate) use extrema::{
    calculate_aroon, calculate_aroonosc, calculate_highest_bars, calculate_lowest_bars,
    calculate_max_index, calculate_min_index, calculate_min_max, calculate_min_max_index,
    calculate_willr, FallingState, HighestState, LowestState, RisingState,
};
pub(crate) use macd::MacdState;
pub(crate) use math::{
    apply_unary as apply_unary_math, calculate_avgdev, calculate_sum, UnaryMathTransform,
};
pub(crate) use momentum::{calculate_bop, calculate_cci, calculate_imi, calculate_mfi};
pub(crate) use oscillator::{OscillatorKind, PriceOscillatorState};
pub(crate) use rsi::RsiState;
pub(crate) use sma::SmaState;
pub(crate) use statistics::{
    calculate_beta, calculate_correl, calculate_linear_regression, calculate_stddev, calculate_var,
    RegressionOutput,
};
pub(crate) use volatility::calculate_trange;
pub(crate) use volume::{AdOscState, AdState, ObvState};
pub(crate) use wma::calculate as calculate_wma;

#[derive(Clone, Debug)]
pub(crate) enum IndicatorState {
    Sma(SmaState),
    Ema(EmaState),
    Cmo(CmoState),
    Rsi(RsiState),
    Highest(HighestState),
    Lowest(LowestState),
    Rising(RisingState),
    Falling(FallingState),
    BarsSince(BarsSinceState),
    ValueWhen(ValueWhenState),
    Cum(CumState),
    Macd(MacdState),
    PriceOscillator(Box<PriceOscillatorState>),
    Obv(ObvState),
    Ad(AdState),
    AdOsc(AdOscState),
    Trix(TrixState),
    T3(Box<T3State>),
    Bbands(Box<BbandsState>),
    Dm(DmState),
    Directional(DirectionalState),
    MovingAverage(Box<MovingAverageState>),
}
