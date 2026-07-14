//! Export an account's double-entry sub-ledger as CSV — the audit trail a
//! finance team imports into a spreadsheet or accounting system, every row
//! traceable to a consensus timestamp, with a per-asset running balance.
//! The rendering lives in `hiero_journal::report`; this is just a driver.
//!
//!   cargo run --example subledger_csv -- <record-dir> <account> > ledger.csv
//!   cargo run --example subledger_csv -- tests/fixtures/mainnet 0.0.800

use hiero_journal::{from_record_dir, report};

fn main() {
    let mut args = std::env::args().skip(1);
    let dir = args
        .next()
        .expect("usage: subledger_csv <record-dir> <account>");
    let account = args
        .next()
        .expect("usage: subledger_csv <record-dir> <account>");

    let journal = from_record_dir(&dir).expect("parse record dir");
    let breaks = journal.conservation_breaks();
    assert!(
        breaks.is_empty(),
        "refusing to export an unbalanced book: {breaks:?}"
    );

    print!("{}", report::subledger_csv(&journal, &account));
}
