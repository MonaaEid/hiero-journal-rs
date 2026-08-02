//! # hiero-journal
//!
//! An **audit-grade double-entry journal** and the financial statements
//! that are views over it — cash flow, balance position, trial balance —
//! derived from Hedera consensus streams via
//! [`hiero-streams`](https://github.com/hiero-hackers/hiero-streams-rs).
//!
//! The design rests on three invariants:
//!
//! 1. **Exact integer arithmetic.** Every amount is a signed integer in
//!    the asset's smallest unit ([`asset::Asset`]). No float ever touches
//!    a balance.
//! 2. **Per-transaction conservation.** Postings for a transaction net to
//!    zero in every asset ([`journal::Journal::conservation_breaks`]) —
//!    and `hiero-streams` proves the underlying stream is gapless, so this
//!    is a structural guarantee, not just an arithmetic tripwire.
//! 3. **Checkpoint-anchored balances.** Balances fold the journal; at scale
//!    you persist periodic self-computed checkpoints and replay only the
//!    delta since — the stream carries all history, so no external snapshot
//!    is needed.
//!
//! ```no_run
//! use hiero_journal::statements;
//!
//! // Parse a directory of verified record files into the book.
//! let journal = hiero_journal::from_record_dir("downloaded/").unwrap();
//! assert!(journal.conservation_breaks().is_empty()); // proves it ties out
//!
//! let mv = statements::cash_movement(&journal, "0.0.98", None, None);
//! assert!(mv.ties_out());
//! ```
//!
//! Everything this crate produces is derivable from the ledger and traces
//! to consensus timestamps. Valuation, cost basis, and revenue-vs-capital
//! classification are policy over external inputs and belong in a separate
//! engine — deliberately not here.

pub mod asset;
pub mod balance;
pub mod journal;
pub mod money;
pub mod report;
pub mod statements;
pub mod token;

pub use asset::{Asset, TokenId};
pub use balance::{Balances, NftHoldings};
pub use journal::{ConservationBreak, EntryKind, Journal, LedgerEntry};
pub use money::Amount;
pub use report::AssuranceReport;
pub use token::Decimals;

use hiero_streams::{detect_format, parse_block, parse_record_file, Format};
use std::path::Path;
use std::{fmt, fs, io};

/// Block-proof verification policy for block-stream ingest.
///
/// A block carries its proof in-band, but a proof *present* is not a proof
/// *checked* — until verified, you are trusting whoever handed you the
/// block. Choose deliberately.
pub enum Verify<'a> {
    /// Verify every block's in-band proof (recomputed merkle root, hinTS
    /// threshold signature, Schnorr/WRAPS suffix) against the ledger-ID
    /// publication carried in the `genesis` block. Requires the
    /// `block-proofs` feature (on by default).
    Against {
        /// Raw bytes of the genesis block (`0.blk.gz`).
        genesis: &'a [u8],
    },
    /// Skip verification — trust the source. The result is *convenient*,
    /// not *provable*; do not market it as verified.
    Trust,
}

/// Error building a journal from stream files on disk.
#[derive(Debug)]
#[non_exhaustive]
pub enum SourceError {
    Io(io::Error),
    /// A stream file failed to parse in `hiero-streams`.
    Stream(String),
    /// Verification ran and the block's in-band proof did not check out —
    /// tampered, truncated, or from the wrong ledger. Fail loud.
    ProofInvalid(String),
    /// `Verify::Against` was requested but this build lacks `block-proofs`.
    VerifyUnavailable,
    /// A file's format is not the one this entry point handles.
    UnsupportedFormat(String),
}

impl fmt::Display for SourceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SourceError::Io(e) => write!(f, "io error: {e}"),
            SourceError::Stream(e) => write!(f, "stream parse error: {e}"),
            SourceError::ProofInvalid(m) => write!(f, "block proof INVALID: {m}"),
            SourceError::VerifyUnavailable => write!(
                f,
                "verification requested but this build lacks the `block-proofs` feature"
            ),
            SourceError::UnsupportedFormat(p) => {
                write!(f, "unexpected stream format in {p}")
            }
        }
    }
}

impl std::error::Error for SourceError {}

impl From<io::Error> for SourceError {
    fn from(e: io::Error) -> Self {
        SourceError::Io(e)
    }
}

/// Ingest one record-file's worth of bytes (`.rcd`/`.rcd.gz` as read from
/// disk — `hiero-streams` handles the gzip) into `journal`.
pub fn ingest_record_bytes(
    journal: &mut Journal,
    bytes: &[u8],
    label: &str,
) -> Result<(), SourceError> {
    match detect_format(bytes) {
        Ok(Format::RecordFileV6) => {
            let parsed =
                parse_record_file(bytes).map_err(|e| SourceError::Stream(format!("{e:?}")))?;
            for tx in parsed.transactions {
                journal.ingest(tx);
            }
            Ok(())
        }
        Ok(_) => Err(SourceError::UnsupportedFormat(label.to_string())),
        Err(e) => Err(SourceError::Stream(format!("{e:?}"))),
    }
}

/// Build a journal from every record file in a directory, ingested in
/// filename order (record filenames are consensus-time-sortable). Files
/// whose extension is not `.gz`/`.rcd` are skipped.
pub fn from_record_dir<P: AsRef<Path>>(dir: P) -> Result<Journal, SourceError> {
    let mut paths: Vec<_> = fs::read_dir(&dir)?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| {
            matches!(
                p.extension().and_then(|e| e.to_str()),
                Some("gz") | Some("rcd")
            )
        })
        .collect();
    paths.sort();

    let mut journal = Journal::new();
    for path in paths {
        let bytes = fs::read(&path)?;
        let label = path.display().to_string();
        ingest_record_bytes(&mut journal, &bytes, &label)?;
    }
    Ok(journal)
}

/// Verify one block's in-band proof against the genesis bootstrap, then
/// ingest its transactions. This is the verify-*then*-report step: a block
/// that fails verification never reaches the journal.
///
/// With `Verify::Trust`, or a non-`Against` policy, the proof is not checked
/// — see [`Verify`]. Requires the `block-proofs` feature for `Against`.
pub fn ingest_block_bytes(
    journal: &mut Journal,
    bytes: &[u8],
    verify: &Verify<'_>,
    label: &str,
) -> Result<(), SourceError> {
    match detect_format(bytes) {
        Ok(Format::BlockStream) => {}
        Ok(_) => return Err(SourceError::UnsupportedFormat(label.to_string())),
        Err(e) => return Err(SourceError::Stream(format!("{e:?}"))),
    }

    verify_block(bytes, verify, label)?;

    let parsed = parse_block(bytes).map_err(|e| SourceError::Stream(format!("{e:?}")))?;
    for tx in parsed.transactions {
        journal.ingest(tx);
    }
    Ok(())
}

/// Build a journal from every block file (`.blk`/`.blk.gz`) in a directory,
/// ingested in filename order, each verified per `verify` before it counts.
/// With `Verify::Against`, verification is enforced for every block — the
/// whole run fails if any block's proof does not check out.
pub fn from_block_dir<P: AsRef<Path>>(dir: P, verify: &Verify<'_>) -> Result<Journal, SourceError> {
    let mut paths: Vec<_> = fs::read_dir(&dir)?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| {
            matches!(
                p.extension().and_then(|e| e.to_str()),
                Some("gz") | Some("blk")
            )
        })
        .collect();
    // Numeric-aware order: block filenames are block numbers, so a plain
    // lexicographic sort would place "10" before "2". Sort by the leading
    // integer in the stem when present, falling back to the name.
    paths.sort_by_key(|p| block_key(p));

    let mut journal = Journal::new();
    for path in paths {
        let bytes = fs::read(&path)?;
        let label = path.display().to_string();
        ingest_block_bytes(&mut journal, &bytes, verify, &label)?;
    }
    Ok(journal)
}

/// Sort key for a block file: (leading block number, full name). Files
/// without a numeric stem sort after numbered ones, by name.
fn block_key(p: &Path) -> (u64, String) {
    let name = p.file_name().and_then(|n| n.to_str()).unwrap_or_default();
    let num: u64 = name
        .split(|c: char| !c.is_ascii_digit())
        .find(|s| !s.is_empty())
        .and_then(|s| s.parse().ok())
        .unwrap_or(u64::MAX);
    (num, name.to_string())
}

#[cfg(feature = "block-proofs")]
fn verify_block(bytes: &[u8], verify: &Verify<'_>, label: &str) -> Result<(), SourceError> {
    use hiero_streams::{extract_proof_material, resolve_bootstrap, verify_block_proof};

    let Verify::Against { genesis } = verify else {
        return Ok(()); // Trust: caller opted out of verification.
    };
    let material =
        extract_proof_material(bytes).map_err(|e| SourceError::Stream(format!("{e:?}")))?;
    let bootstrap = resolve_bootstrap(&material, Some(genesis), "pass the genesis block")
        .map_err(|e| SourceError::Stream(format!("{e:?}")))?;
    let verification = verify_block_proof(&material, &bootstrap)
        .map_err(|e| SourceError::Stream(format!("{e:?}")))?;
    if !verification.valid() {
        return Err(SourceError::ProofInvalid(format!(
            "block {} at {label}",
            material.block_number
        )));
    }
    Ok(())
}

#[cfg(not(feature = "block-proofs"))]
fn verify_block(_bytes: &[u8], verify: &Verify<'_>, _label: &str) -> Result<(), SourceError> {
    match verify {
        Verify::Against { .. } => Err(SourceError::VerifyUnavailable),
        Verify::Trust => Ok(()),
    }
}

/// A per-block record of what verification actually checked — the audit
/// trail behind the word "verified". An auditor inspects *this* (which
/// block, which recomputed root, which signature scheme, how many signers)
/// rather than trusting a green checkmark.
#[cfg(feature = "block-proofs")]
#[derive(Debug, Clone)]
pub struct Attestation {
    pub block_number: u64,
    /// Recomputed block merkle root (hex) — the value the signature covers.
    pub block_root_hex: String,
    /// Proof scheme: "aggregate-Schnorr" (genesis / pre-settled) or "WRAPS".
    pub proof_scheme: String,
    /// The hinTS threshold signature over the recomputed root checked out.
    pub hints_threshold_ok: bool,
    /// (signers present, total nodes) on the Schnorr path.
    pub signers: Option<(u64, u64)>,
    /// WRAPS suffix checks passed (settled history).
    pub wraps_ok: Option<bool>,
    /// Every applicable check passed.
    pub valid: bool,
}

/// Verify every block in a directory against `genesis` and ingest it,
/// returning both the journal **and the attestation for each block** — the
/// evidence of what was checked. A block that fails verification aborts the
/// whole run (nothing unverified is booked).
#[cfg(feature = "block-proofs")]
pub fn from_block_dir_attested<P: AsRef<Path>>(
    dir: P,
    genesis: &[u8],
) -> Result<(Journal, Vec<Attestation>), SourceError> {
    use hiero_streams::{extract_proof_material, resolve_bootstrap, verify_block_proof, ProofPath};

    let mut paths: Vec<_> = fs::read_dir(&dir)?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| {
            matches!(
                p.extension().and_then(|e| e.to_str()),
                Some("gz") | Some("blk")
            )
        })
        .collect();
    paths.sort_by_key(|p| block_key(p));

    let mut journal = Journal::new();
    let mut attestations = Vec::new();
    for path in paths {
        let bytes = fs::read(&path)?;
        let label = path.display().to_string();

        let material =
            extract_proof_material(&bytes).map_err(|e| SourceError::Stream(format!("{e:?}")))?;
        let bootstrap = resolve_bootstrap(&material, Some(genesis), "pass the genesis block")
            .map_err(|e| SourceError::Stream(format!("{e:?}")))?;
        let v = verify_block_proof(&material, &bootstrap)
            .map_err(|e| SourceError::Stream(format!("{e:?}")))?;

        let att = Attestation {
            block_number: material.block_number,
            block_root_hex: to_hex(&material.block_root),
            proof_scheme: match material.layout.path {
                ProofPath::AggregateSchnorr => "aggregate-Schnorr".into(),
                ProofPath::WrapsCompressedProof => "WRAPS".into(),
                _ => "unknown".into(),
            },
            hints_threshold_ok: v.hints.all_passed(),
            signers: v
                .schnorr
                .as_ref()
                .map(|s| (s.signer_count as u64, s.total_nodes as u64)),
            wraps_ok: v.wraps.as_ref().map(|w| w.all_passed()),
            valid: v.valid(),
        };
        if !att.valid {
            return Err(SourceError::ProofInvalid(format!(
                "block {} at {label}",
                att.block_number
            )));
        }
        attestations.push(att);

        let parsed = parse_block(&bytes).map_err(|e| SourceError::Stream(format!("{e:?}")))?;
        for tx in parsed.transactions {
            journal.ingest(tx);
        }
    }
    Ok((journal, attestations))
}

#[cfg(feature = "block-proofs")]
fn to_hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    bytes
        .iter()
        .fold(String::with_capacity(bytes.len() * 2), |mut s, b| {
            let _ = write!(s, "{b:02x}");
            s
        })
}
