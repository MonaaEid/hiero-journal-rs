//! Balances — a multi-asset position, and the `balance_at` primitive that
//! every statement composes from.
//!
//! This crate computes balances by folding the journal it holds. For
//! full-history `balance_at` at scale you persist periodic self-computed
//! checkpoints and replay only the delta since the latest; the fold here is
//! the same arithmetic over whatever window the journal covers. The stream
//! carries all history from genesis, so no external snapshot is needed.

use crate::asset::Asset;
use crate::journal::Journal;
use crate::money::{self, Amount};
use std::collections::{BTreeMap, BTreeSet};

/// A position across assets, each in its smallest unit (`i128`). Ordered by
/// asset so renderings and diffs are deterministic.
pub type Balances = BTreeMap<Asset, Amount>;

/// NFT holdings grouped by token id, with the owned serial numbers for each
/// token. This is the serial-aware storage view layered over the journal.
pub type NftHoldings = BTreeMap<crate::asset::TokenId, BTreeSet<u64>>;

/// Sum every posting for `account` whose day is `<= as_of_day` (inclusive,
/// "YYYY-MM-DD"). `None` folds the entire journal. Assets that net to zero
/// are dropped so the position lists only non-empty holdings.
pub fn balance_at(journal: &Journal, account: &str, as_of_day: Option<&str>) -> Balances {
    let mut bal: Balances = BTreeMap::new();
    for e in journal.for_account(account) {
        if let Some(day) = as_of_day {
            if e.day.as_str() > day {
                continue;
            }
        }
        money::add_assign(bal.entry(e.asset.clone()).or_default(), e.amount);
    }
    bal.retain(|_, v| *v != 0);
    bal
}

/// Add two positions (used to roll an opening balance forward by a period
/// of net movement).
pub fn add(mut base: Balances, delta: &Balances) -> Balances {
    for (asset, amount) in delta {
        money::add_assign(base.entry(asset.clone()).or_default(), *amount);
    }
    base.retain(|_, v| *v != 0);
    base
}

/// Collect the NFT serials owned by `account` as of `as_of_day`.
///
/// Each NFT serial is tracked individually in the journal, so the result is
/// grouped by token id and contains the exact owned serial numbers.
pub fn nft_holdings_at(journal: &Journal, account: &str, as_of_day: Option<&str>) -> NftHoldings {
    let mut holdings: NftHoldings = BTreeMap::new();
    for e in journal.for_account(account) {
        if let Some(day) = as_of_day {
            if e.day.as_str() > day {
                continue;
            }
        }
        let Asset::Nft {
            token_id,
            serial_number,
        } = e.asset
        else {
            continue;
        };
        let serials = holdings.entry(token_id).or_default();
        if e.amount > 0 {
            serials.insert(serial_number);
        } else if e.amount < 0 {
            serials.remove(&serial_number);
        }
    }
    holdings.retain(|_, serials| !serials.is_empty());
    holdings
}
