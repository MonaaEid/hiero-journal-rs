//! Generate a human-readable **assurance report** (HTML) from a verified
//! block stream — the artifact you hand a finance reviewer instead of a
//! terminal full of checkmarks. All the work lives in `hiero_journal::report`;
//! this is just a driver.
//!
//!   cargo run --example audit_report -- [out.html]
//!
//! The report is print-ready — open it and Print → Save as PDF. Requires the
//! (default) `block-proofs` feature.

use hiero_journal::{from_block_dir_attested, AssuranceReport};

const BLOCKS: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/blocks");

fn main() {
    let out = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "audit_report.html".to_string());
    let genesis = std::fs::read(format!("{BLOCKS}/0.blk.gz")).expect("read genesis");

    let (journal, attestations) =
        from_block_dir_attested(BLOCKS, &genesis).expect("verify + ingest block stream");

    let report = AssuranceReport::from_attested(&journal, &attestations);
    std::fs::write(&out, report.to_html()).expect("write report");

    println!(
        "wrote {out}  ({} blocks attested · {} postings · conservation {})",
        report.attestations.len(),
        report.postings,
        if report.conserved {
            "holds ✓"
        } else {
            "BROKEN ✗"
        },
    );
}
