//! Block-stream ingest with in-band proof verification, over committed
//! HIP-1056 fixtures (genesis `0.blk.gz` + settled `467.blk.gz`, copied
//! from `hiero-streams`). These run only with the `block-proofs` feature,
//! which is on by default — the verify-then-report path end to end.
#![cfg(feature = "block-proofs")]

use hiero_journal::{from_block_dir, ingest_block_bytes, Journal, SourceError, Verify};

const BLOCKS: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/blocks");

fn genesis() -> Vec<u8> {
    std::fs::read(format!("{BLOCKS}/0.blk.gz")).expect("read genesis")
}

#[test]
fn verifies_and_ingests_a_block_dir() {
    let verify = Verify::Against {
        genesis: &genesis(),
    };
    let journal = from_block_dir(BLOCKS, &verify).expect("verify + ingest block dir");

    // Verification passed for every block or the call would have errored.
    // Whatever transactions those blocks carried must still conserve.
    assert!(
        journal.conservation_breaks().is_empty(),
        "ingested blocks must conserve per asset"
    );
}

#[test]
fn a_tampered_block_is_rejected() {
    let genesis = genesis();
    let mut settled = std::fs::read(format!("{BLOCKS}/467.blk.gz")).expect("read block 467");

    // Corrupt a byte in the middle of the gzip payload. It must fail to
    // verify (or fail to parse) — either way it never reaches the journal.
    let mid = settled.len() / 2;
    settled[mid] ^= 0xFF;

    let verify = Verify::Against { genesis: &genesis };
    let mut journal = Journal::new();
    let result = ingest_block_bytes(&mut journal, &settled, &verify, "tampered");

    assert!(
        result.is_err(),
        "a tampered block must be rejected, not booked"
    );
    assert!(
        journal.is_empty(),
        "nothing from a rejected block may enter the journal"
    );
    // A clean proof failure surfaces as ProofInvalid; a mangled container
    // surfaces as a parse error. Both are acceptable rejections.
    assert!(
        matches!(
            result,
            Err(SourceError::ProofInvalid(_) | SourceError::Stream(_))
        ),
        "expected a proof/parse rejection, got {result:?}"
    );
}

#[test]
fn content_tamper_fails_the_proof_layer() {
    use flate2::{read::GzDecoder, write::GzEncoder, Compression};
    use std::io::{Read, Write};

    let genesis = genesis();
    let block = std::fs::read(format!("{BLOCKS}/467.blk.gz")).expect("read block 467");

    // Decompress, flip one bit deep in the block content, re-gzip. The gzip
    // is valid and the block decodes, so this is NOT caught at parse — it
    // must fail because the recomputed merkle root no longer matches the
    // signed proof. This is the check that proves verification works.
    let mut raw = Vec::new();
    GzDecoder::new(&block[..])
        .read_to_end(&mut raw)
        .expect("inflate");
    let mid = raw.len() / 2;
    raw[mid] ^= 0x01;
    let mut e = GzEncoder::new(Vec::new(), Compression::default());
    e.write_all(&raw).unwrap();
    let tampered = e.finish().unwrap();

    let verify = Verify::Against { genesis: &genesis };
    let mut journal = Journal::new();
    let result = ingest_block_bytes(&mut journal, &tampered, &verify, "content-tamper");

    assert!(
        matches!(result, Err(SourceError::ProofInvalid(_))),
        "content tamper must fail at the proof layer, got {result:?}"
    );
    assert!(journal.is_empty());
}

#[test]
fn trust_mode_skips_verification() {
    // Trust ingests without a genesis / without checking proofs — the
    // escape hatch, honestly labeled. The genuine genesis still parses.
    let mut journal = Journal::new();
    ingest_block_bytes(&mut journal, &genesis(), &Verify::Trust, "genesis")
        .expect("trust-mode ingest parses without verifying");
}
