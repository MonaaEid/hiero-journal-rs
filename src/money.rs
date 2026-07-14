//! Exact integer money.
//!
//! Amounts are [`Amount`] (`i128`) in an asset's smallest unit. A single
//! ledger leg always fits `i64` (that is what the parser emits), but this
//! crate *accumulates* — and cumulative gross flow on a hot account can
//! exceed `i64`'s ceiling of ~9.2×10¹⁸ tinybar (≈92 billion ℏ). `i128`
//! carries ~1.7×10³⁸ of headroom, so a real Hedera figure cannot overflow
//! it; we widen losslessly from the parser's `i64` at ingest.
//!
//! No float ever touches a value here — not in arithmetic, not in
//! rendering. That is what makes the output audit-grade.

/// A signed amount in an asset's smallest unit.
pub type Amount = i128;

/// Checked addition that **fails loud** instead of wrapping. A silent wrap
/// in a balance is a catastrophe; an overflow at `i128` is impossible with
/// real Hedera data, so hitting this panic means a bug or a crafted input —
/// exactly the cases where you want a stop, not a wrong number.
#[inline]
pub fn add(a: Amount, b: Amount) -> Amount {
    a.checked_add(b)
        .expect("money overflow: i128 accumulation exceeded — bug or crafted input")
}

/// Add `b` into the accumulator `a` in place (checked).
#[inline]
pub fn add_assign(a: &mut Amount, b: Amount) {
    *a = add(*a, b);
}

/// Format `amount` (smallest units) at `decimals` places, **exactly**, via
/// integer split — never division into a float. `suffix` is appended as-is
/// (e.g. `" ℏ"` or `""`).
pub fn render(amount: Amount, decimals: u8, suffix: &str) -> String {
    // 10^decimals must fit i128; guard absurd scales rather than overflow.
    let Some(scale) = 10u128.checked_pow(decimals as u32) else {
        return format!("{amount} (raw, {decimals} decimals){suffix}");
    };
    let sign = if amount < 0 { "-" } else { "" };
    let abs = amount.unsigned_abs();
    let whole = abs / scale;
    if decimals == 0 {
        return format!("{sign}{whole}{suffix}");
    }
    let frac = abs % scale;
    format!(
        "{sign}{whole}.{frac:0width$}{suffix}",
        width = decimals as usize
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_hbar_exactly() {
        assert_eq!(render(150_000_000, 8, " ℏ"), "1.50000000 ℏ");
        assert_eq!(render(-1, 8, " ℏ"), "-0.00000001 ℏ");
        assert_eq!(render(0, 8, " ℏ"), "0.00000000 ℏ");
    }

    #[test]
    fn renders_token_decimals_exactly() {
        // 1_500_000 base units at 6 decimals == 1.500000
        assert_eq!(render(1_500_000, 6, ""), "1.500000");
        assert_eq!(render(1, 0, ""), "1"); // 0-decimal token == integer
    }

    #[test]
    fn checked_add_survives_large_but_real_totals() {
        // 92 billion ℏ in tinybar overflows i64 but is fine at i128.
        let over_i64 = 92_000_000_000i128 * 100_000_000;
        assert_eq!(add(over_i64, over_i64), over_i64 * 2);
    }

    #[test]
    #[should_panic(expected = "money overflow")]
    fn checked_add_stops_at_the_ceiling() {
        add(i128::MAX, 1);
    }
}
