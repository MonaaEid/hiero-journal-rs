//! The unit an amount is denominated in. Every amount in the journal is
//! a signed integer in this asset's *smallest* unit — tinybar for HBAR,
//! the token's base unit for a fungible token, or 1 for a single NFT
//! serial. Floating point never touches a balance; that discipline is
//! what makes the output audit-grade.

use crate::money::{self, Amount};
use crate::token::Decimals;
use std::fmt;

/// A Hedera token id in `shard.realm.num` form.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TokenId {
    pub shard_num: i64,
    pub realm_num: i64,
    pub token_num: i64,
}

impl TokenId {
    pub fn from_parts(shard_num: i64, realm_num: i64, token_num: i64) -> Self {
        TokenId {
            shard_num,
            realm_num,
            token_num,
        }
    }

    /// Parse `"shard.realm.num"`. Entity id components are non-negative;
    /// anything else (missing parts, trailing junk, signs) is `None`.
    pub fn parse(s: &str) -> Option<Self> {
        let mut parts = s.split('.');
        // Digits only — `i64::parse` would also accept a leading sign.
        let mut component = || {
            let p = parts.next()?;
            if p.is_empty() || !p.bytes().all(|b| b.is_ascii_digit()) {
                return None;
            }
            p.parse::<i64>().ok()
        };
        let shard_num = component()?;
        let realm_num = component()?;
        let token_num = component()?;
        if parts.next().is_some() {
            return None;
        }
        Some(TokenId::from_parts(shard_num, realm_num, token_num))
    }
}

impl fmt::Display for TokenId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}.{}.{}",
            self.shard_num, self.realm_num, self.token_num
        )
    }
}

impl From<hiero_streams::TokenId> for TokenId {
    fn from(token_id: hiero_streams::TokenId) -> Self {
        TokenId {
            shard_num: token_id.shard_num,
            realm_num: token_id.realm_num,
            token_num: token_id.token_num,
        }
    }
}

/// HBAR's fixed decimal scale: 1 ℏ = 10^8 tinybar.
pub const HBAR_DECIMALS: u8 = 8;

/// An on-ledger asset: native HBAR, a fungible token, or a single NFT.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Asset {
    /// Native HBAR. Amounts are tinybar (1 ℏ = 100_000_000 tinybar).
    Hbar,
    /// A fungible token, identified by token id. Amounts are the token's
    /// base unit; decimals live off-ledger, so this crate never rescales them.
    FungibleToken { token_id: TokenId },
    /// A non-fungible token serial, identified by token id + serial.
    /// Amounts are whole ownership units — every posting is ±1. The serial
    /// is `i64` exactly as on the wire (valid serials are >= 1).
    Nft {
        token_id: TokenId,
        serial_number: i64,
    },
}

impl Asset {
    /// A stable label for grouping/printing: `"HBAR"` or the token id.
    pub fn label(&self) -> String {
        match self {
            Asset::Hbar => "HBAR".to_string(),
            Asset::FungibleToken { token_id } | Asset::Nft { token_id, .. } => token_id.to_string(),
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
            Asset::FungibleToken { token_id } => format!("{amount} {token_id}"),
            Asset::Nft {
                token_id,
                serial_number,
            } => format!("{amount} {token_id}#{serial_number}"),
        }
    }

    /// Like [`Asset::render`], but scales a token by its decimals when the
    /// registry knows them (e.g. `1_500_000` at 6 decimals → `1.500000`).
    /// Falls back to base units for unknown tokens. HBAR is unaffected.
    /// NFTs are indivisible, so their whole-unit counts never rescale.
    pub fn render_with(&self, amount: Amount, decimals: &Decimals) -> String {
        match self {
            Asset::Hbar => money::render(amount, HBAR_DECIMALS, " ℏ"),
            Asset::FungibleToken { token_id } => match decimals.get(&token_id.to_string()) {
                Some(d) => money::render(amount, d, &format!(" {token_id}")),
                None => format!("{amount} {token_id}"),
            },
            Asset::Nft { .. } => self.render(amount),
        }
    }
}

impl fmt::Display for Asset {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Asset::Hbar => f.write_str("HBAR"),
            Asset::FungibleToken { token_id } => fmt::Display::fmt(token_id, f),
            Asset::Nft {
                token_id,
                serial_number,
            } => write!(f, "{token_id}#{serial_number}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_id_parses_well_formed_ids() {
        assert_eq!(
            TokenId::parse("0.0.123"),
            Some(TokenId::from_parts(0, 0, 123))
        );
        assert_eq!(TokenId::parse("1.2.3"), Some(TokenId::from_parts(1, 2, 3)));
    }

    #[test]
    fn token_id_rejects_malformed_ids() {
        for bad in [
            "", "0.0", "0.0.1.2", "0.0.x", "0.0.-1", "-1.0.5", "0. 0.1", "0.0.+1",
        ] {
            assert_eq!(TokenId::parse(bad), None, "{bad:?} must not parse");
        }
    }

    #[test]
    fn token_id_display_round_trips() {
        let id = TokenId::from_parts(0, 0, 4242);
        assert_eq!(TokenId::parse(&id.to_string()), Some(id));
    }

    #[test]
    fn nft_asset_renders_with_serial() {
        let nft = Asset::Nft {
            token_id: TokenId::from_parts(0, 0, 7),
            serial_number: 12,
        };
        assert_eq!(nft.to_string(), "0.0.7#12");
        assert_eq!(nft.render(1), "1 0.0.7#12");
        assert_eq!(nft.label(), "0.0.7");
    }

    #[test]
    fn nft_render_with_never_rescales() {
        let nft = Asset::Nft {
            token_id: TokenId::from_parts(0, 0, 7),
            serial_number: 12,
        };
        // Even if a decimals registry claims a scale for the token id,
        // an NFT count is indivisible and stays whole.
        let mut decimals = Decimals::new();
        decimals.set("0.0.7", 6);
        assert_eq!(nft.render_with(1, &decimals), "1 0.0.7#12");
    }
}
