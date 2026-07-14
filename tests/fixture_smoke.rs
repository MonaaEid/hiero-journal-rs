//! End-to-end smoke test over committed mainnet record fixtures: parse →
//! journal → prove conservation → produce a statement that ties out. These
//! fixtures are real signed mainnet files (copied from `hiero-streams`),
//! so a green run means the whole pipeline agrees with consensus output.

use hiero_journal::{from_record_dir, statements};

const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/mainnet");

#[test]
fn journal_is_non_empty_and_conserves() {
    let journal = from_record_dir(FIXTURES).expect("parse fixture dir");
    assert!(!journal.is_empty(), "fixtures should yield postings");

    let breaks = journal.conservation_breaks();
    assert!(
        breaks.is_empty(),
        "every transaction must net to zero per asset; breaks: {breaks:?}"
    );
}

#[test]
fn whole_book_trial_balance_is_zero() {
    let journal = from_record_dir(FIXTURES).expect("parse fixture dir");
    let tb = statements::trial_balance(&journal, None, None);
    assert!(
        tb.is_empty(),
        "double-entry book must sum to zero; got {tb:?}"
    );
}

#[test]
fn subledger_csv_has_header_and_one_row_per_posting() {
    use hiero_journal::report;

    let journal = from_record_dir(FIXTURES).expect("parse fixture dir");
    let account = "0.0.800"; // the staking-reward account — active in the fixtures
    let csv = report::subledger_csv(&journal, account);

    let mut lines = csv.lines();
    let header = lines.next().expect("header row");
    assert!(
        header.starts_with("consensus_timestamp,"),
        "stable header present"
    );
    assert!(
        header.ends_with(",running_balance"),
        "running balance column present"
    );

    let data_rows = lines.count();
    let postings = journal.for_account(account).count();
    assert_eq!(data_rows, postings, "exactly one CSV row per posting");
    assert!(postings > 0, "fixture account should have postings");
}

#[test]
fn cash_movement_ties_out_for_the_busiest_account() {
    let journal = from_record_dir(FIXTURES).expect("parse fixture dir");

    // Pick the account with the most postings so the report is non-trivial.
    let mut counts = std::collections::HashMap::<&str, usize>::new();
    for e in journal.entries() {
        *counts.entry(e.account.as_str()).or_default() += 1;
    }
    let account = counts
        .into_iter()
        .max_by_key(|(_, n)| *n)
        .map(|(a, _)| a.to_string())
        .expect("at least one account");

    let mv = statements::cash_movement(&journal, &account, None, None);
    assert!(
        mv.ties_out(),
        "opening + movements must equal closing for {account}"
    );
    assert!(
        !mv.lines.is_empty(),
        "busiest account should have movement lines"
    );
}
