//! Stateless math helpers used by low-state TA-Lib builtins.

use crate::diagnostic::RuntimeError;
use crate::types::Value;
use crate::vm::SeriesBuffer;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum UnaryMathTransform {
    Acos,
    Asin,
    Atan,
    Ceil,
    Cos,
    Cosh,
    Exp,
    Floor,
    Ln,
    Log10,
    Sin,
    Sinh,
    Sqrt,
    Tan,
    Tanh,
}

pub(crate) fn apply_unary(
    value: Value,
    transform: UnaryMathTransform,
    pc: usize,
) -> Result<Value, RuntimeError> {
    match value {
        Value::NA => Ok(Value::NA),
        Value::F64(value) => {
            let output = match transform {
                UnaryMathTransform::Acos => value.acos(),
                UnaryMathTransform::Asin => value.asin(),
                UnaryMathTransform::Atan => value.atan(),
                UnaryMathTransform::Ceil => value.ceil(),
                UnaryMathTransform::Cos => value.cos(),
                UnaryMathTransform::Cosh => value.cosh(),
                UnaryMathTransform::Exp => value.exp(),
                UnaryMathTransform::Floor => value.floor(),
                UnaryMathTransform::Ln => value.ln(),
                UnaryMathTransform::Log10 => value.log10(),
                UnaryMathTransform::Sin => value.sin(),
                UnaryMathTransform::Sinh => value.sinh(),
                UnaryMathTransform::Sqrt => value.sqrt(),
                UnaryMathTransform::Tan => value.tan(),
                UnaryMathTransform::Tanh => value.tanh(),
            };
            Ok(Value::F64(output))
        }
        other => Err(RuntimeError::TypeMismatch {
            pc,
            expected: "f64",
            found: other.type_name(),
        }),
    }
}

pub(crate) fn calculate_sum(
    buffer: &SeriesBuffer,
    window: usize,
    pc: usize,
) -> Result<Value, RuntimeError> {
    if buffer.len() < window {
        return Ok(Value::NA);
    }

    let mut sum = 0.0;
    for value in buffer.iter_recent(window) {
        match value {
            Value::F64(value) => sum += value,
            Value::NA => return Ok(Value::NA),
            other => {
                return Err(RuntimeError::TypeMismatch {
                    pc,
                    expected: "f64",
                    found: other.type_name(),
                });
            }
        }
    }

    Ok(Value::F64(sum))
}

pub(crate) fn calculate_avgdev(
    buffer: &SeriesBuffer,
    window: usize,
    pc: usize,
) -> Result<Value, RuntimeError> {
    if buffer.len() < window {
        return Ok(Value::NA);
    }

    let mut sum = 0.0;
    for value in buffer.iter_recent(window) {
        match value {
            Value::F64(value) => sum += value,
            Value::NA => return Ok(Value::NA),
            other => {
                return Err(RuntimeError::TypeMismatch {
                    pc,
                    expected: "f64",
                    found: other.type_name(),
                });
            }
        }
    }

    let average = sum / window as f64;
    let mut total_deviation = 0.0;
    for value in buffer.iter_recent(window) {
        match value {
            Value::F64(value) => total_deviation += (value - average).abs(),
            Value::NA => return Ok(Value::NA),
            other => {
                return Err(RuntimeError::TypeMismatch {
                    pc,
                    expected: "f64",
                    found: other.type_name(),
                });
            }
        }
    }

    Ok(Value::F64(total_deviation / window as f64))
}

#[cfg(test)]
mod tests {
    use super::{apply_unary, calculate_avgdev, calculate_sum, UnaryMathTransform};
    use crate::types::Value;
    use crate::vm::SeriesBuffer;

    #[test]
    fn unary_math_transform_preserves_na() {
        assert_eq!(
            apply_unary(Value::NA, UnaryMathTransform::Sin, 0).unwrap(),
            Value::NA
        );
    }

    #[test]
    fn unary_math_transform_computes_numeric_output() {
        let value = apply_unary(Value::F64(0.0), UnaryMathTransform::Cos, 0).unwrap();
        assert_eq!(value, Value::F64(1.0));
    }

    #[test]
    fn sum_uses_trailing_window() {
        let mut buffer = SeriesBuffer::new(8);
        for value in [1.0, 2.0, 3.0, 4.0] {
            buffer.push(Value::F64(value));
        }

        assert_eq!(calculate_sum(&buffer, 3, 0).unwrap(), Value::F64(9.0));
    }

    #[test]
    fn avgdev_matches_average_absolute_deviation() {
        let mut buffer = SeriesBuffer::new(8);
        for value in [1.0, 2.0, 3.0] {
            buffer.push(Value::F64(value));
        }

        assert_eq!(
            calculate_avgdev(&buffer, 3, 0).unwrap(),
            Value::F64(2.0 / 3.0)
        );
    }
}
