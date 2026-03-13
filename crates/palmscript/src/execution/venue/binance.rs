use crate::interval::DeclaredMarketSource;

use super::super::ExecutionError;

pub(crate) fn validate(source: &DeclaredMarketSource) -> Result<(), ExecutionError> {
    if source.symbol.is_empty() {
        return Err(ExecutionError::InvalidConfig {
            message: "binance paper execution requires a non-empty symbol".to_string(),
        });
    }
    Ok(())
}
