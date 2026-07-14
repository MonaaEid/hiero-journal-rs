//! The unit an amount is denominated in. Every amount in the journal is
//! a signed integer in this asset's *smallest* unit — tinybar for HBAR,
//! the token's base unit for a token. Floating point never touches a
//! balance; that discipline is what makes the output audit-grade.

use crate::money::{self, Amount};
use crate::token::Decimals;
use std::fmt;

/// HBAR's fixed decimal scale: 1 ℏ = 10^8 tinybar.
pub const HBAR_DECIMALS: u8 = 8;

/// An on-ledger asset: native HBAR or a fungible token by entity id.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Asset {
    /// Native HBAR. Amounts are tinybar (1 ℏ = 100_000_000 tinybar).
    Hbar,
    /// A token, identified "0.0.123". Amounts are the token's base unit;
    /// decimals live off-ledger, so this crate never rescales them.
    Token(String),
}

impl Asset {
    /// A stable label for grouping/printing: `"HBAR"` or the token id.
    pub fn label(&self) -> &str {
        match self {
            Asset::Hbar => "HBAR",
            Asset::Token(id) => id,
        }
    }

    /// Format an amount in this asset's smallest unit for humans. HBAR is
    /// rendered in ℏ (exact, via integer split — no float); tokens stay in
    /// base units, since their decimal scale is not on the transfer legs.
    /// Use [`Asset::render_with`] to scale tokens when the decimals are
    /// known.
    pub fn render(&self, amount: Amount) -> String {
        match self {
            Asset::Hbar => money::render(amount, HBAR_DECIMALS, " ℏ"),
            Asset::Token(id) => format!("{amount} {id}"),
        }
    }

    /// Like [`Asset::render`], but scales a token by its decimals when the
    /// registry knows them (e.g. `1_500_000` at 6 decimals → `1.500000`).
    /// Falls back to base units for unknown tokens. HBAR is unaffected.
    pub fn render_with(&self, amount: Amount, decimals: &Decimals) -> String {
        match self {
            Asset::Hbar => money::render(amount, HBAR_DECIMALS, " ℏ"),
            Asset::Token(id) => match decimals.get(id) {
                Some(d) => money::render(amount, d, &format!(" {id}")),
                None => format!("{amount} {id}"),
            },
        }
    }
}

impl fmt::Display for Asset {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.label())
    }
}
