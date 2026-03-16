//! Ledger-scoped types shared by the compiler, runtime, and backtester.
//!
//! These types model strategy-visible per-execution ledger fields exposed by
//! backtests and portfolio simulations.

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum LedgerField {
    BaseFree,
    QuoteFree,
    BaseTotal,
    QuoteTotal,
    MarkValueQuote,
}

impl LedgerField {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::BaseFree => "base_free",
            Self::QuoteFree => "quote_free",
            Self::BaseTotal => "base_total",
            Self::QuoteTotal => "quote_total",
            Self::MarkValueQuote => "mark_value_quote",
        }
    }

    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "base_free" => Some(Self::BaseFree),
            "quote_free" => Some(Self::QuoteFree),
            "base_total" => Some(Self::BaseTotal),
            "quote_total" => Some(Self::QuoteTotal),
            "mark_value_quote" => Some(Self::MarkValueQuote),
            _ => None,
        }
    }
}
