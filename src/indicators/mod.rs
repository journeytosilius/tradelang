//! Indicator implementations and runtime state used by builtin execution.
//!
//! Indicator math lives here so the VM keeps direct opcode dispatch while the
//! per-indicator logic stays modular and independently testable.

pub(crate) mod ema;
pub(crate) mod rsi;
pub(crate) mod sma;

pub(crate) use ema::EmaState;
pub(crate) use rsi::RsiState;
pub(crate) use sma::SmaState;

#[derive(Clone, Debug)]
pub(crate) enum IndicatorState {
    Sma(SmaState),
    Ema(EmaState),
    Rsi(RsiState),
}
