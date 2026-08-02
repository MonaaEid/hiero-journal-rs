//! Presentation — turning the journal into the two deliverables:
//!
//! - **[`AssuranceReport`]** → HTML for a human reviewer. A plain data model
//!   (the facts, testable, decoupled from the feature-gated
//!   [`crate::Attestation`]) plus [`AssuranceReport::to_html`], whose
//!   stylesheet lives in a real `.css` file ([`include_str!`]) so it is
//!   edited as CSS, not a Rust string.
//! - **[`subledger_csv`]** → RFC 4180 CSV for a spreadsheet / accounting
//!   system: one row per posting, in consensus order, with a per-asset
//!   running balance and exact integer amounts.
//!
//! Only the convenience constructor that reads a verified block run
//! ([`AssuranceReport::from_attested`]) needs `block-proofs`.

use crate::asset::Asset;
use crate::money::{self, Amount};
use crate::Journal;
use std::collections::BTreeMap;

/// One row of the verification-attestation table. Plain data mirroring
/// [`crate::Attestation`], so rendering does not depend on `block-proofs`.
#[derive(Debug, Clone)]
pub struct AttestationRow {
    pub block: u64,
    pub root_hex: String,
    pub scheme: String,
    pub hints_ok: bool,
    /// (signers present, total nodes) on the Schnorr path.
    pub signers: Option<(u64, u64)>,
    pub wraps_ok: Option<bool>,
    pub valid: bool,
}

/// One reconciliation line: an account's position in a single asset.
#[derive(Debug, Clone)]
pub struct ReconLine {
    pub account: String,
    pub asset: Asset,
    pub amount: Amount,
}

/// A complete assurance report as data. Build it directly, or from a
/// verified block run via [`AssuranceReport::from_attested`], then render
/// with [`AssuranceReport::to_html`].
#[derive(Debug, Clone)]
pub struct AssuranceReport {
    pub attestations: Vec<AttestationRow>,
    pub reconciliation: Vec<ReconLine>,
    /// The asset the reconciliation totals (and `total`) are in.
    pub asset: Asset,
    pub total: Amount,
    pub postings: usize,
    pub conserved: bool,
}

impl AssuranceReport {
    /// Inclusive (lowest, highest) attested block number.
    pub fn block_range(&self) -> (u64, u64) {
        let lo = self.attestations.iter().map(|a| a.block).min().unwrap_or(0);
        let hi = self.attestations.iter().map(|a| a.block).max().unwrap_or(0);
        (lo, hi)
    }

    /// Render the report as a self-contained HTML document body (a `<title>`,
    /// the stylesheet, and the `<article>`) — publishable as-is.
    pub fn to_html(&self) -> String {
        let (lo, hi) = self.block_range();

        let att_rows: String = self.attestations.iter().map(att_row).collect();
        let recon_rows: String = self.reconciliation.iter().map(recon_row).collect();
        let total_row = format!(
            "<tr class=\"total\"><td>Total — ledger closes to</td>\
             <td class=\"num mono\">{}</td></tr>",
            self.asset.render(self.total)
        );
        let assert_class = if self.conserved { "ok" } else { "bad" };
        let assert_text = if self.conserved {
            "holds ✓"
        } else {
            "BROKEN ✗"
        };

        format!(
            "<title>Hedera Ledger — Assurance Report</title>\n\
             <style>{css}</style>\n\
             <article>\
               <header class=\"rep-head\">\
                 <p class=\"eyebrow\">Cryptographic assurance report</p>\
                 <h1>Hedera consensus ledger — verified extract</h1>\
                 <p class=\"lede\">Every figure below is derived from block-stream data whose \
                   in-band proof was re-verified against the ledger's genesis publication. \
                   Blocks {lo}–{hi}.</p>\
               </header>\
               <section>\
                 <h2>1 · Verification attestation</h2>\
                 <p class=\"note\">What “verified” means here: for each block, the merkle root was \
                   recomputed from the raw items and the hinTS threshold signature over that root, \
                   plus the scheme-specific suffix, checked out. A block that failed would not \
                   appear — it never enters the ledger.</p>\
                 <div class=\"scroll\"><table>\
                   <thead><tr><th>Block</th><th>Recomputed merkle root</th><th>Scheme</th>\
                     <th class=\"c\">hinTS</th><th class=\"c\">Signers</th><th class=\"c\">WRAPS</th>\
                     <th class=\"c\">Verdict</th></tr></thead>\
                   <tbody>{att_rows}</tbody>\
                 </table></div>\
               </section>\
               <section>\
                 <h2>2 · Reconciliation</h2>\
                 <p class=\"note\">Every account's position from the verified blocks. \
                   Double-entry holds: the positions sum to zero. <code>supply:HBAR</code> is the \
                   contra account for the genesis issuance of the initial supply (created with no \
                   debit), so the book still closes.</p>\
                 <div class=\"scroll\"><table class=\"recon\">\
                   <thead><tr><th>Account</th><th class=\"num\">Position</th></tr></thead>\
                   <tbody>{recon_rows}{total_row}</tbody>\
                 </table></div>\
                 <p class=\"assert {assert_class}\">{postings} postings booked · conservation \
                   {assert_text}</p>\
               </section>\
               <footer>\
                 <h2>Scope &amp; basis</h2>\
                 <ul>\
                   <li><strong>Basis.</strong> Audit-<em>ready</em> schedules traceable to consensus \
                     timestamps — not GAAP/IFRS financial statements. Accrual and valuation \
                     judgments are the preparer's. Full derivation methodology: \
                     <code>docs/METHODOLOGY.md</code>.</li>\
                   <li><strong>Provenance.</strong> Figures derive solely from cryptographically \
                     verified stream data — the only source. No unverified feed contributes to \
                     any figure.</li>\
                   <li><strong>Integrity.</strong> Amounts are exact integers in the asset's \
                     smallest unit; every accumulation is overflow-checked.</li>\
                 </ul>\
               </footer>\
             </article>",
            css = include_str!("assurance.css"),
            postings = self.postings,
        )
    }
}

fn att_row(a: &AttestationRow) -> String {
    let signers = a
        .signers
        .map(|(s, t)| format!("{s} / {t}"))
        .unwrap_or_else(|| "—".into());
    let wraps = match a.wraps_ok {
        Some(true) => "pass",
        Some(false) => "FAIL",
        None => "—",
    };
    let (verdict, verdict_class) = if a.valid {
        ("verified", "ok")
    } else {
        ("FAILED", "bad")
    };
    format!(
        "<tr><td class=\"num\">{block}</td>\
         <td class=\"mono root\" title=\"{root_full}\">{root_short}</td>\
         <td>{scheme}</td>\
         <td class=\"c\">{hints}</td>\
         <td class=\"c mono\">{signers}</td>\
         <td class=\"c mono\">{wraps}</td>\
         <td class=\"c\"><span class=\"chip {verdict_class}\">{verdict}</span></td></tr>",
        block = a.block,
        root_full = esc(&a.root_hex),
        root_short = short_hash(&a.root_hex),
        scheme = esc(&a.scheme),
        hints = if a.hints_ok { "pass" } else { "FAIL" },
    )
}

fn recon_row(l: &ReconLine) -> String {
    format!(
        "<tr><td class=\"mono\">{}</td><td class=\"num mono\">{}</td></tr>",
        esc(&l.account),
        l.asset.render(l.amount),
    )
}

/// Minimal HTML-text escape for interpolated values (defence in depth —
/// account ids and hashes are already safe, but rendering never trusts that).
fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn short_hash(hex: &str) -> String {
    if hex.len() <= 28 {
        hex.to_string()
    } else {
        format!("{}…{}", &hex[..16], &hex[hex.len() - 8..])
    }
}

// ── CSV sub-ledger export ──────────────────────────────────────────────

/// The sub-ledger column header. Stable — downstream imports key on it.
const CSV_HEADER: &str =
    "consensus_timestamp,day,tx_type,result_code,entry_kind,asset,amount,running_balance";

/// Render `account`'s postings as an RFC 4180 CSV sub-ledger: one row per
/// posting, in consensus order, with a per-asset running balance. Amounts
/// are exact integers in the asset's smallest unit (the `asset` column names
/// the unit); the running balance is folded with checked arithmetic. Every
/// field is escaped, so a future field containing a comma cannot corrupt the
/// file.
pub fn subledger_csv(journal: &Journal, account: &str) -> String {
    let mut out = String::from(CSV_HEADER);
    out.push('\n');

    let mut running: BTreeMap<&Asset, Amount> = BTreeMap::new();
    for e in journal.for_account(account) {
        let bal = running.entry(&e.asset).or_default();
        *bal = money::add(*bal, e.amount);
        let fields = [
            e.consensus_timestamp.clone(),
            e.day.clone(),
            e.tx_type.clone(),
            e.result_code.to_string(),
            e.kind.as_str().to_string(),
            e.asset.label(),
            e.amount.to_string(),
            bal.to_string(),
        ];
        for (i, f) in fields.iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            out.push_str(&csv_field(f));
        }
        out.push('\n');
    }
    out
}

/// RFC 4180 field: quote when the value contains a comma, quote, CR, or LF,
/// doubling any interior quotes.
fn csv_field(s: &str) -> String {
    if s.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

// ── Convenience constructor from a verified block run (needs the crypto) ──

#[cfg(feature = "block-proofs")]
impl AssuranceReport {
    /// Assemble a report from a verified block run: the per-block
    /// attestations, plus the HBAR reconciliation of the resulting journal.
    pub fn from_attested(journal: &crate::Journal, attestations: &[crate::Attestation]) -> Self {
        let asset = Asset::Hbar;
        let reconciliation = reconcile(journal, &asset);
        let total = reconciliation
            .iter()
            .fold(0, |acc, l| crate::money::add(acc, l.amount));
        AssuranceReport {
            attestations: attestations.iter().map(AttestationRow::from).collect(),
            reconciliation,
            asset,
            total,
            postings: journal.len(),
            conserved: journal.conservation_breaks().is_empty(),
        }
    }
}

#[cfg(feature = "block-proofs")]
impl From<&crate::Attestation> for AttestationRow {
    fn from(a: &crate::Attestation) -> Self {
        AttestationRow {
            block: a.block_number,
            root_hex: a.block_root_hex.clone(),
            scheme: a.proof_scheme.clone(),
            hints_ok: a.hints_threshold_ok,
            signers: a.signers,
            wraps_ok: a.wraps_ok,
            valid: a.valid,
        }
    }
}

/// Every account's non-zero position in `asset`, sorted descending — the
/// reconciliation body, which sums to zero for a conserved book.
#[cfg(feature = "block-proofs")]
fn reconcile(journal: &crate::Journal, asset: &Asset) -> Vec<ReconLine> {
    use std::collections::BTreeSet;
    let accounts: BTreeSet<&str> = journal
        .entries()
        .iter()
        .map(|e| e.account.as_str())
        .collect();
    let mut lines: Vec<ReconLine> = accounts
        .iter()
        .filter_map(|a| {
            crate::statements::holdings(journal, a, None)
                .get(asset)
                .filter(|v| **v != 0)
                .map(|v| ReconLine {
                    account: a.to_string(),
                    asset: *asset,
                    amount: *v,
                })
        })
        .collect();
    lines.sort_by_key(|l| std::cmp::Reverse(l.amount));
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> AssuranceReport {
        AssuranceReport {
            attestations: vec![
                AttestationRow {
                    block: 0,
                    root_hex: "3de47629fe289fc7c4c6757b78c90d5a".into(),
                    scheme: "aggregate-Schnorr".into(),
                    hints_ok: true,
                    signers: Some((2, 3)),
                    wraps_ok: None,
                    valid: true,
                },
                AttestationRow {
                    block: 467,
                    root_hex: "a7986473fa0a42a55a74f04eca352ec7".into(),
                    scheme: "WRAPS".into(),
                    hints_ok: true,
                    signers: None,
                    wraps_ok: Some(true),
                    valid: true,
                },
            ],
            reconciliation: vec![
                ReconLine {
                    account: "0.0.2".into(),
                    asset: Asset::Hbar,
                    amount: 100,
                },
                ReconLine {
                    account: "supply:HBAR".into(),
                    asset: Asset::Hbar,
                    amount: -100,
                },
            ],
            asset: Asset::Hbar,
            total: 0,
            postings: 6,
            conserved: true,
        }
    }

    #[test]
    fn block_range_spans_min_and_max() {
        assert_eq!(sample().block_range(), (0, 467));
    }

    #[test]
    fn html_contains_the_key_facts() {
        let html = sample().to_html();
        assert!(html.contains("Assurance Report"), "title present");
        assert!(
            html.contains("aggregate-Schnorr") && html.contains("WRAPS"),
            "schemes rendered"
        );
        assert!(html.contains("2 / 3"), "signer count rendered");
        assert!(html.contains(">verified<"), "verdict chip rendered");
        assert!(html.contains("supply:HBAR"), "contra account listed");
        assert!(html.contains("0.00000000 ℏ"), "total ties out to zero");
        assert!(
            html.contains("conservation holds"),
            "integrity line present"
        );
    }

    #[test]
    fn escaping_neutralizes_markup() {
        assert_eq!(esc("a<b>&c"), "a&lt;b&gt;&amp;c");
    }

    #[test]
    fn csv_fields_are_rfc4180_safe() {
        assert_eq!(csv_field("cryptoTransfer"), "cryptoTransfer"); // plain: untouched
        assert_eq!(csv_field("a,b"), "\"a,b\""); // comma → quoted
        assert_eq!(csv_field("she said \"hi\""), "\"she said \"\"hi\"\"\""); // quote doubled
        assert_eq!(csv_field("line1\nline2"), "\"line1\nline2\""); // newline → quoted
    }
}
