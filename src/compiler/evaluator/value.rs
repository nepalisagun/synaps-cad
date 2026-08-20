use csgrs::Real;
use std::cmp::Ordering;

/// Compares two exact reals through Hyperlimit's workspace-wide predicate
/// cascade, including its policy-authorized terminal evaluation.
pub(crate) fn compare_reals(left: &Real, right: &Real) -> Option<Ordering> {
    hyperlimit::compare_reals(left, right, crate::compiler::PREDICATE_POLICY).value()
}

pub(crate) fn reals_equal(left: &Real, right: &Real) -> Option<bool> {
    compare_reals(left, right).map(|ordering| ordering == Ordering::Equal)
}

/// Runtime value produced by the `OpenSCAD` expression evaluator.
#[derive(Debug, Clone)]
pub enum Value {
    /// Exact rational, symbolic, or computable real number.
    Number(Real),
    /// Boolean value.
    Bool(bool),
    /// `OpenSCAD` vector or list.
    List(Vec<Self>),
    /// UTF-8 string.
    String(String),
    /// Inclusive `(start, end, step)` exact-real range.
    Range(Real, Real, Real),
    /// Undefined value.
    Undef,
}

impl Value {
    /// Returns the exact-real numeric value.
    #[must_use]
    pub fn as_real(&self) -> Option<Real> {
        match self {
            Self::Number(n) => Some(n.clone()),
            _ => None,
        }
    }

    /// Returns a lossy primitive approximation at an explicit interoperability boundary.
    #[must_use]
    pub fn to_f64_lossy(&self) -> Option<f64> {
        match self {
            Self::Number(n) => n.to_f64_lossy(),
            _ => None,
        }
    }

    /// Returns a nonnegative exact integer as `usize` for indexing/count APIs.
    #[must_use]
    pub fn to_usize_exact(&self) -> Option<usize> {
        let integer = self.as_real()?.exact_rational()?.to_big_integer()?;
        usize::try_from(integer).ok()
    }

    /// Returns a nonnegative exact integer as `u64` for deterministic seeds.
    #[must_use]
    pub fn to_u64_exact(&self) -> Option<u64> {
        let integer = self.as_real()?.exact_rational()?.to_big_integer()?;
        u64::try_from(integer).ok()
    }

    /// Returns a nonnegative exact integer as `u32` for character conversion.
    #[must_use]
    pub fn to_u32_exact(&self) -> Option<u32> {
        let integer = self.as_real()?.exact_rational()?.to_big_integer()?;
        u32::try_from(integer).ok()
    }

    /// Applies `OpenSCAD` truthiness rules when the numeric zero predicate can
    /// be decided under the centralized certainty policy.
    #[must_use]
    pub fn try_as_bool(&self) -> Option<bool> {
        match self {
            Self::Bool(b) => Some(*b),
            Self::Number(n) => reals_equal(n, &Real::zero()).map(|is_zero| !is_zero),
            Self::String(s) => Some(!s.is_empty()),
            Self::List(l) => Some(!l.is_empty()),
            Self::Undef => Some(false),
            Self::Range(..) => Some(true),
        }
    }

    /// Borrows the elements when this is a list.
    #[must_use]
    pub fn as_list(&self) -> Option<&[Self]> {
        match self {
            Self::List(l) => Some(l),
            _ => None,
        }
    }

    /// Converts a wholly numeric list to exact values.
    ///
    /// A malformed element invalidates the vector rather than being discarded
    /// and shifting every later coordinate to a different position.
    #[must_use]
    pub fn to_real_list(&self) -> Option<Vec<Real>> {
        self.as_list()
            .and_then(|l| l.iter().map(Self::as_real).collect())
    }

    /// Expands ranges and lists into values suitable for `for` iteration.
    ///
    /// # Errors
    ///
    /// Returns an error if an exact range direction or bound predicate remains
    /// undecided after the centralized certainty cascade.
    pub fn to_iterable(&self) -> Result<Vec<Self>, String> {
        match self {
            Self::Range(from, to, step) => {
                let mut vals = Vec::new();
                let mut value = from.clone();
                let step_order = compare_reals(step, &Real::zero())
                    .ok_or_else(|| "range step sign is undecided".to_owned())?;
                if step_order == Ordering::Greater {
                    loop {
                        let bound = compare_reals(&value, to)
                            .ok_or_else(|| "ascending range bound is undecided".to_owned())?;
                        if bound == Ordering::Greater {
                            break;
                        }
                        vals.push(Self::Number(value.clone()));
                        #[cfg(feature = "fuzz-bounded-campaign")]
                        if vals.len() >= 256 {
                            break;
                        }
                        value += step;
                    }
                } else if step_order == Ordering::Less {
                    loop {
                        let bound = compare_reals(&value, to)
                            .ok_or_else(|| "descending range bound is undecided".to_owned())?;
                        if bound == Ordering::Less {
                            break;
                        }
                        vals.push(Self::Number(value.clone()));
                        #[cfg(feature = "fuzz-bounded-campaign")]
                        if vals.len() >= 256 {
                            break;
                        }
                        value += step;
                    }
                }
                Ok(vals)
            }
            Self::List(l) => Ok(l.clone()),
            _ => Ok(vec![self.clone()]),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numeric_comparisons_use_the_centralized_predicate_policy() {
        let below_pi = Real::from(103_993_u32) / Real::from(33_102_u32);
        let below_pi = below_pi.expect("the denominator is nonzero");

        assert_eq!(
            compare_reals(&Real::pi(), &below_pi),
            Some(Ordering::Greater)
        );
        assert_eq!(reals_equal(&Real::pi(), &below_pi), Some(false));
    }

    #[test]
    fn malformed_numeric_lists_are_not_compacted() {
        let list = Value::List(vec![
            Value::Number(Real::one()),
            Value::String("not a coordinate".into()),
            Value::Number(Real::from(3_u8)),
        ]);

        assert!(list.to_real_list().is_none());
    }
}
