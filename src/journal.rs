//! The journal — the foundational book of record. Every financial
//! statement in this crate ([`crate::statements`]) is just a *view* over
//! these entries, so the hard correctness work lives here, once.
//!
//! ## From ledger transaction to double-entry
//!
//! A Hedera transaction records a `transferList` where every leg — fee
//! legs included, as on-ledger — nets to zero. We keep that invariant
//! (see [`Journal::conservation_breaks`]) but decompose each account's
//! net movement into accounting-meaningful [`EntryKind`]s:
//!
//! - **Fee.** The fee is only separable on the *payer's* line, and it is
//!   exactly `charged_fee_tinybar`. We split the payer's net leg into a
//!   `Fee` debit and a principal remainder. Because `fee + principal ==
//!   original net`, the decomposition preserves conservation exactly.
//! - **StakingReward.** A credit to an account in a transaction that also
//!   debits the reward account `0.0.800`. This is a documented heuristic,
//!   good enough to separate reward income from ordinary receipts; finer
//!   classification is a downstream policy concern, out of scope here.
//! - **Transfer.** Everything else — ordinary principal movement.
//!
//! Failed transactions are kept, not filtered: a failed transaction still
//! charged its fee, and its record already omits the reverted principal —
//! so the legs as-parsed are exactly what belongs in the books.

use crate::asset::Asset;
use crate::money::{self, Amount};
use hiero_streams::{ParsedTransaction, TransferLeg};
use std::collections::BTreeMap;

/// The reward account: HBAR staking rewards are paid from here.
const REWARD_ACCOUNT: &str = "0.0.800";

/// Prefix for the synthetic contra accounts that absorb supply changes
/// (token mint/burn, genesis HBAR issuance) so the book still conserves.
/// These are not real ledger accounts — the colon distinguishes them, since
/// real entity ids (`0.0.x`) never contain one.
const SUPPLY_PREFIX: &str = "supply:";

/// The synthetic supply (contra) account for an asset, e.g. `supply:HBAR`
/// or `supply:0.0.123`.
fn supply_account(asset: &Asset) -> String {
    format!("{SUPPLY_PREFIX}{}", asset.label())
}

/// What an entry represents, once decomposed from the raw transfer legs.
/// Every variant here is derivable *from the ledger itself* — this is the
/// only classification the crate makes, precisely because it's provable.
/// Opinion-based categorization (revenue vs. capital, GAAP sections) is a
/// downstream policy concern and deliberately lives outside this crate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub enum EntryKind {
    /// Ordinary principal movement.
    Transfer,
    /// A network fee charged to the payer (always a debit).
    Fee,
    /// A staking-reward credit (heuristic: paired with a `0.0.800` debit).
    StakingReward,
    /// Token supply created (mint) — the contra-side of a treasury credit.
    Mint,
    /// Token supply destroyed (burn/wipe) — the contra-side of a debit.
    Burn,
}

impl EntryKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            EntryKind::Transfer => "Transfer",
            EntryKind::Fee => "Network fee",
            EntryKind::StakingReward => "Staking reward",
            EntryKind::Mint => "Mint",
            EntryKind::Burn => "Burn",
        }
    }
}

/// One posting: a single account's movement in a single asset, in that
/// asset's smallest unit, at a consensus timestamp.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LedgerEntry {
    /// Consensus timestamp, "seconds.nanos" — the ledger's authoritative
    /// clock, and the anchor an auditor traces every figure back to.
    pub consensus_timestamp: String,
    /// UTC day "YYYY-MM-DD" (lexicographically sortable — used for
    /// period bounds without pulling in a date dependency).
    pub day: String,
    /// TransactionBody oneof case, e.g. "cryptoTransfer".
    pub tx_type: String,
    /// ResponseCodeEnum numeric result (22 = SUCCESS). Kept for
    /// classification; entries are booked regardless of success.
    pub result_code: i32,
    /// The account this posting belongs to, "0.0.123".
    pub account: String,
    /// Fee payer of the originating transaction.
    pub payer: String,
    pub asset: Asset,
    /// Signed smallest-unit amount (negative = debit / outflow). `i128`
    /// for headroom when this crate accumulates; widened losslessly from
    /// the parser's per-leg `i64` at ingest.
    pub amount: Amount,
    pub kind: EntryKind,
}

/// A complete, ordered book of [`LedgerEntry`]s. Build it from parsed
/// stream transactions, then query it for balances and statements.
#[derive(Debug, Default, Clone)]
pub struct Journal {
    entries: Vec<LedgerEntry>,
}

impl Journal {
    pub fn new() -> Self {
        Journal::default()
    }

    pub fn entries(&self) -> &[LedgerEntry] {
        &self.entries
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Build a journal from parsed stream transactions. The whole ledger
    /// is ingested (not filtered to owned accounts) so conservation stays
    /// checkable; statements filter to an account at query time.
    pub fn from_transactions<I>(txs: I) -> Self
    where
        I: IntoIterator<Item = ParsedTransaction>,
    {
        let mut j = Journal::new();
        for tx in txs {
            j.ingest(tx);
        }
        j
    }

    /// Fold one transaction into the book, applying the fee/reward
    /// decomposition described in the module docs.
    pub fn ingest(&mut self, tx: ParsedTransaction) {
        // Net HBAR movement per account (fee legs are already in here).
        // Widen each leg to i128 at the boundary; accumulate checked.
        let mut net: BTreeMap<&str, Amount> = BTreeMap::new();
        for TransferLeg { account, amount } in &tx.transfers {
            money::add_assign(
                net.entry(account.as_str()).or_default(),
                i128::from(*amount),
            );
        }
        let reward_paid = tx
            .transfers
            .iter()
            .any(|l| l.account == REWARD_ACCOUNT && l.amount < 0);
        let fee = i128::from(tx.charged_fee_tinybar);

        for (account, amount) in net {
            if amount == 0 {
                continue; // pure pass-through for this account
            }
            let is_payer_fee = account == tx.payer && fee > 0;
            if is_payer_fee {
                self.push(&tx, account, Asset::Hbar, -fee, EntryKind::Fee);
                let principal = money::add(amount, fee); // add the fee back out
                if principal != 0 {
                    let kind = reward_kind(principal, reward_paid);
                    self.push(&tx, account, Asset::Hbar, principal, kind);
                }
            } else {
                let kind = reward_kind(amount, reward_paid);
                self.push(&tx, account, Asset::Hbar, amount, kind);
            }
        }

        // Genesis HBAR issuance: the initial supply is created into the
        // treasury with no debit, so the legs don't net to zero — the HBAR
        // analog of a token mint. Book the residual to a synthetic
        // supply:HBAR account so the book conserves. Gate: the transaction
        // must be *single-sided* (money with no source). This is safe
        // because HBAR is never minted after genesis — every normal HBAR
        // transaction has a debit — so a genuine stream gap (which carries
        // mixed-sign legs) still surfaces as a conservation break rather
        // than being masked as issuance.
        let hbar_residual = tx
            .transfers
            .iter()
            .fold(0i128, |acc, l| money::add(acc, i128::from(l.amount)));
        if hbar_residual != 0 {
            let single_sided = tx.transfers.iter().all(|l| l.amount >= 0)
                || tx.transfers.iter().all(|l| l.amount <= 0);
            if single_sided {
                let kind = if hbar_residual > 0 {
                    EntryKind::Mint
                } else {
                    EntryKind::Burn
                };
                self.push(
                    &tx,
                    &supply_account(&Asset::Hbar),
                    Asset::Hbar,
                    -hbar_residual,
                    kind,
                );
            }
        }

        // Token legs carry no fee (fees are HBAR) — book them straight,
        // tracking each token's residual so we can balance mint/burn.
        let mut token_residual: BTreeMap<&str, Amount> = BTreeMap::new();
        for leg in &tx.token_transfers {
            if leg.amount == 0 {
                continue;
            }
            let amount = i128::from(leg.amount);
            money::add_assign(
                token_residual.entry(leg.token.as_str()).or_default(),
                amount,
            );
            self.push(
                &tx,
                &leg.account,
                Asset::Token(leg.token.clone()),
                amount,
                EntryKind::Transfer,
            );
        }

        // A token's legs net to zero *unless* supply changed. Only a
        // mint/burn/wipe transaction may create that residual; we book the
        // contra-entry to a synthetic per-token supply account so the book
        // conserves. A residual under any other type is left alone — it
        // surfaces as a conservation break, which is what we want.
        if supply_changing(&tx.tx_type) {
            for (token, residual) in token_residual {
                if residual == 0 {
                    continue;
                }
                let kind = if residual > 0 {
                    EntryKind::Mint
                } else {
                    EntryKind::Burn
                };
                let asset = Asset::Token(token.to_string());
                self.push(&tx, &supply_account(&asset), asset.clone(), -residual, kind);
            }
        }
    }

    fn push(
        &mut self,
        tx: &ParsedTransaction,
        account: &str,
        asset: Asset,
        amount: Amount,
        kind: EntryKind,
    ) {
        self.entries.push(LedgerEntry {
            consensus_timestamp: tx.consensus_timestamp.clone(),
            day: tx.day.clone(),
            tx_type: tx.tx_type.clone(),
            result_code: tx.result_code,
            account: account.to_string(),
            payer: tx.payer.clone(),
            asset,
            amount,
            kind,
        });
    }

    /// Every entry for one account, in ingest (consensus) order.
    pub fn for_account<'a>(&'a self, account: &'a str) -> impl Iterator<Item = &'a LedgerEntry> {
        self.entries.iter().filter(move |e| e.account == account)
    }

    /// The conservation tripwire: group all entries by (timestamp, asset)
    /// and return any group whose signed amounts do **not** sum to zero.
    /// An empty result proves every unit is conserved — no stream gap and
    /// no ingest bug slipped a figure past the double-entry invariant.
    pub fn conservation_breaks(&self) -> Vec<ConservationBreak> {
        let mut sums: BTreeMap<(&str, &Asset), Amount> = BTreeMap::new();
        for e in &self.entries {
            money::add_assign(
                sums.entry((e.consensus_timestamp.as_str(), &e.asset))
                    .or_default(),
                e.amount,
            );
        }
        sums.into_iter()
            .filter(|(_, s)| *s != 0)
            .map(|((ts, asset), residual)| ConservationBreak {
                consensus_timestamp: ts.to_string(),
                asset: asset.clone(),
                residual,
            })
            .collect()
    }
}

/// A transaction whose postings failed to net to zero in some asset — a
/// missing leg (stream gap) or an ingest bug. Should never occur.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConservationBreak {
    pub consensus_timestamp: String,
    pub asset: Asset,
    /// The non-zero residual (what conservation is off by), smallest unit.
    pub residual: Amount,
}

/// Transaction types whose token transfer list may legitimately not net to
/// zero because they change a token's total supply.
fn supply_changing(tx_type: &str) -> bool {
    matches!(tx_type, "tokenMint" | "tokenBurn" | "tokenWipe")
}

fn reward_kind(amount: Amount, reward_paid: bool) -> EntryKind {
    if amount > 0 && reward_paid {
        EntryKind::StakingReward
    } else {
        EntryKind::Transfer
    }
}
