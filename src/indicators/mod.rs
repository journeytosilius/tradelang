//! Indicator implementations and runtime state used by builtin execution.
//!
//! Indicator math lives here so the VM keeps direct opcode dispatch while the
//! per-indicator logic stays modular and independently testable.

pub(crate) mod ema;
pub(crate) mod event;
pub(crate) mod extrema;
pub(crate) mod macd;
pub(crate) mod math;
pub(crate) mod rsi;
pub(crate) mod sma;
pub(crate) mod volatility;
pub(crate) mod volume;
pub(crate) mod wma;

pub(crate) use ema::EmaState;
pub(crate) use event::{BarsSinceState, ValueWhenState};
pub(crate) use extrema::{
    calculate_max_index, calculate_min_index, calculate_min_max, calculate_min_max_index,
    FallingState, HighestState, LowestState, RisingState,
};
pub(crate) use macd::MacdState;
pub(crate) use math::{
    apply_unary as apply_unary_math, calculate_avgdev, calculate_sum, UnaryMathTransform,
};
pub(crate) use rsi::RsiState;
pub(crate) use sma::SmaState;
pub(crate) use volatility::calculate_trange;
pub(crate) use volume::ObvState;
pub(crate) use wma::calculate as calculate_wma;

#[derive(Clone, Debug)]
pub(crate) enum IndicatorState {
    Sma(SmaState),
    Ema(EmaState),
    Rsi(RsiState),
    Highest(HighestState),
    Lowest(LowestState),
    Rising(RisingState),
    Falling(FallingState),
    BarsSince(BarsSinceState),
    ValueWhen(ValueWhenState),
    Macd(MacdState),
    Obv(ObvState),
}
