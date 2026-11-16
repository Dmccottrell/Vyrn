//! Helpers for the hot paths of the dispatch loop: copies and stores that
//! skip the general clone and destructor for scalars, and Python-style
//! integer division without the general arithmetic routine. Each returns
//! `None`, or falls back to the general code, for anything unusual, so the
//! fast and slow paths always agree.

use super::value::Value;

/// A copy of `value`, without a call for the scalars that dominate hot
/// loops.
#[inline(always)]
pub fn copy(value: &Value) -> Value {
    match value {
        Value::Int(i) => Value::Int(*i),
        Value::Float(f) => Value::Float(*f),
        Value::Bool(b) => Value::Bool(*b),
        Value::None => Value::None,
        other => other.clone(),
    }
}

/// Stores into a slot without a destructor call when the old value owns
/// nothing.
#[inline(always)]
pub fn store(slot: &mut Value, value: Value) {
    if slot.owns_heap() {
        *slot = value;
    } else {
        std::mem::forget(std::mem::replace(slot, value));
    }
}

/// Python's floor division, or `None` when the general routine must
/// decide (division by zero, overflow).
#[inline(always)]
pub fn floor_div(x: i64, y: i64) -> Option<i64> {
    let q = x.checked_div(y)?;
    Some(if x % y != 0 && ((x < 0) != (y < 0)) {
        q - 1
    } else {
        q
    })
}

/// Python's modulo (the sign follows the divisor), or `None` as above.
#[inline(always)]
pub fn modulo(x: i64, y: i64) -> Option<i64> {
    let r = x.checked_rem(y)?;
    Some(if r != 0 && ((r < 0) != (y < 0)) {
        r + y
    } else {
        r
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn floor_division_and_modulo_follow_python() {
        assert_eq!(floor_div(7, 2), Some(3));
        assert_eq!(floor_div(-7, 2), Some(-4));
        assert_eq!(modulo(-7, 3), Some(2));
        assert_eq!(modulo(7, -3), Some(-2));
        assert_eq!(floor_div(1, 0), None);
        assert_eq!(floor_div(i64::MIN, -1), None);
        assert_eq!(modulo(i64::MIN, -1), None);
    }
}
