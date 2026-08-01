//! Reports — views over the journal. Everything here is derivable *from
//! the ledger itself* and ties back to consensus timestamps; nothing here
//! makes an accounting-policy judgment. That line is the whole point:
//!
//! - **Cash movements** — inflows/outflows/net by period. Crypto is
//!   cash-basis, so raw movement is a provable fact. We group by
//!   [`EntryKind`] (the only classification the ledger justifies), *not*
//!   by GAAP sections, which would be opinion.
//! - **Holdings** — asset position at any past instant. A fact.
//! - **Trial balance** — the whole book sums to zero per asset. A proof.
//!
//! Valuation (FX into a reporting currency), cost basis, and revenue-vs-
//! capital classification are policy layered on external inputs, and belong
//! in a separate, clearly-labeled engine — not in this provable core.

use crate::asset::Asset;
use crate::balance::{self, Balances};
use crate::journal::{EntryKind, Journal};
use crate::money::{self, Amount};
use std::collections::BTreeMap;

/// One aggregated movement line: a (kind, asset) bucket with gross inflow,
/// gross outflow, and net, all in the asset's smallest unit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MovementLine {
    pub kind: EntryKind,
    pub asset: Asset,
    pub inflow: Amount,
    pub outflow: Amount, // stored positive; the gross outflow magnitude
}

impl MovementLine {
    pub fn net(&self) -> Amount {
        self.inflow - self.outflow
    }
}

/// A cash-movement report for one account over a day range, with
/// opening/closing holdings that tie out by construction.
#[derive(Debug, Clone)]
pub struct CashMovement {
    pub account: String,
    pub from_day: Option<String>,
    pub to_day: Option<String>,
    pub opening: Balances,
    pub closing: Balances,
    pub lines: Vec<MovementLine>,
}

impl CashMovement {
    /// Net movement per asset across all lines — equals `closing -
    /// opening` for every asset by construction.
    pub fn net_by_asset(&self) -> Balances {
        let mut m: Balances = BTreeMap::new();
        for l in &self.lines {
            money::add_assign(m.entry(l.asset).or_default(), l.net());
        }
        m.retain(|_, v| *v != 0);
        m
    }

    /// True when opening + Σmovements == closing for every asset. The
    /// statement-level echo of journal conservation.
    pub fn ties_out(&self) -> bool {
        balance::add(self.opening.clone(), &self.net_by_asset()) == self.closing
    }
}

/// Build a cash-movement report for `account` over `[from_day, to_day]`
/// (inclusive, `None` = unbounded), grouped by the provable [`EntryKind`].
pub fn cash_movement(
    journal: &Journal,
    account: &str,
    from_day: Option<&str>,
    to_day: Option<&str>,
) -> CashMovement {
    let opening = opening_balance(journal, account, from_day);

    let mut buckets: BTreeMap<(EntryKind, Asset), (Amount, Amount)> = BTreeMap::new();
    for e in journal.for_account(account) {
        if !in_window(&e.day, from_day, to_day) {
            continue;
        }
        let slot = buckets.entry((e.kind, e.asset)).or_insert((0, 0));
        if e.amount >= 0 {
            money::add_assign(&mut slot.0, e.amount);
        } else {
            money::add_assign(&mut slot.1, -e.amount);
        }
    }

    let lines = buckets
        .into_iter()
        .map(|((kind, asset), (inflow, outflow))| MovementLine {
            kind,
            asset,
            inflow,
            outflow,
        })
        .collect();

    let closing = balance::add(
        opening.clone(),
        &period_movement(journal, account, from_day, to_day),
    );

    CashMovement {
        account: account.to_string(),
        from_day: from_day.map(str::to_string),
        to_day: to_day.map(str::to_string),
        opening,
        closing,
        lines,
    }
}

/// Holdings for `account` as of `as_of_day` (inclusive; `None` = latest).
/// The asset position — a fact. No liabilities or equity: those are not
/// on-ledger, and this crate does not fabricate them.
pub fn holdings(journal: &Journal, account: &str, as_of_day: Option<&str>) -> Balances {
    balance::balance_at(journal, account, as_of_day)
}

/// Trial balance: sum every posting per asset over the window. A correct
/// double-entry book sums to zero in every asset — the whole-book proof,
/// independent of any single account. An empty result means balanced.
pub fn trial_balance(journal: &Journal, from_day: Option<&str>, to_day: Option<&str>) -> Balances {
    let mut m: Balances = BTreeMap::new();
    for e in journal.entries() {
        if !in_window(&e.day, from_day, to_day) {
            continue;
        }
        money::add_assign(m.entry(e.asset).or_default(), e.amount);
    }
    m.retain(|_, v| *v != 0);
    m
}

fn in_window(day: &str, from: Option<&str>, to: Option<&str>) -> bool {
    if let Some(f) = from {
        if day < f {
            return false;
        }
    }
    if let Some(t) = to {
        if day > t {
            return false;
        }
    }
    true
}

/// Balance of everything strictly before `from_day` (empty for an
/// unbounded window — there is no "before").
fn opening_balance(journal: &Journal, account: &str, from_day: Option<&str>) -> Balances {
    let Some(from) = from_day else {
        return Balances::new();
    };
    let mut bal: Balances = BTreeMap::new();
    for e in journal.for_account(account) {
        if e.day.as_str() >= from {
            continue;
        }
        money::add_assign(bal.entry(e.asset).or_default(), e.amount);
    }
    bal.retain(|_, v| *v != 0);
    bal
}

/// Net movement within the window (used to derive closing from opening).
fn period_movement(
    journal: &Journal,
    account: &str,
    from: Option<&str>,
    to: Option<&str>,
) -> Balances {
    let mut m: Balances = BTreeMap::new();
    for e in journal.for_account(account) {
        if !in_window(&e.day, from, to) {
            continue;
        }
        money::add_assign(m.entry(e.asset).or_default(), e.amount);
    }
    m.retain(|_, v| *v != 0);
    m
}
