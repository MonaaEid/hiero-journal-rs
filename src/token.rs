//! Token decimal scales — a display concern, kept out of the arithmetic.
//!
//! HTS token amounts on the ledger are integers in the token's *base unit*;
//! the decimal scale lives in the token's metadata (set at `TokenCreate`),
//! **not** in the transfer legs. So decimals are an input the caller
//! supplies — from parsing the token's `TokenCreate` transaction in the
//! stream, where the scale is set on-chain.
//!
//! Crucially, this crate never divides a stored balance by `10^decimals`:
//! that would introduce a fraction and break the exact-integer invariant.
//! Decimals only affect *rendering* ([`crate::asset::Asset::render_with`]).

use std::collections::BTreeMap;

/// A registry of token id → decimal places. HBAR is not here (its scale of
/// 8 is fixed and known); this is only for HTS tokens.
#[derive(Debug, Default, Clone)]
pub struct Decimals(BTreeMap<String, u8>);

impl Decimals {
    pub fn new() -> Self {
        Decimals::default()
    }

    /// Record a token's decimal scale.
    pub fn set(&mut self, token: impl Into<String>, decimals: u8) {
        self.0.insert(token.into(), decimals);
    }

    /// The decimal scale for a token, if known.
    pub fn get(&self, token: &str) -> Option<u8> {
        self.0.get(token).copied()
    }

    /// Parse a `"0.0.123:6,0.0.456:2"` list (for the CLI). Whitespace
    /// around entries is tolerated; a malformed pair is an error.
    pub fn from_pairs(spec: &str) -> Result<Self, String> {
        let mut d = Decimals::new();
        for pair in spec.split(',').map(str::trim).filter(|s| !s.is_empty()) {
            let (token, dec) = pair
                .split_once(':')
                .ok_or_else(|| format!("expected 'token:decimals', got '{pair}'"))?;
            let dec: u8 = dec
                .trim()
                .parse()
                .map_err(|_| format!("bad decimals in '{pair}'"))?;
            d.set(token.trim(), dec);
        }
        Ok(d)
    }
}
