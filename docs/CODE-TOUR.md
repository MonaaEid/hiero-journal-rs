# Code tour

A guided reading order for the crate. If you read the files in this order,
each one only depends on concepts from the ones before it.

## Start here: the value types

1. **`src/money.rs`** — `Amount` (`i128`) and `money::add` (checked). The
   whole exactness story in ~80 lines. Read the module doc first.
2. **`src/asset.rs`** — `Asset` (HBAR or a token id) and how amounts render.
3. **`src/token.rs`** — `Decimals`, a token → decimal-places registry used
   only for *display*. Note it never divides a stored balance.

## The core: the journal

4. **`src/journal.rs`** — the heart. Read in this order:
   - `LedgerEntry` — one posting (account, asset, signed amount, kind).
   - `EntryKind` — the *only* classification the crate makes, because each
     variant is derivable from the ledger.
   - `Journal::ingest` — how one parsed transaction becomes postings. This is
     where the two subtle rules live: **fee decomposition** (split the payer's
     leg into a `Fee` and principal) and **supply contra** (token mint/burn
     and genesis HBAR issuance book to a synthetic `supply:*` account so the
     book conserves).
   - `Journal::conservation_breaks` — the tripwire that makes the whole thing
     trustworthy: any transaction not netting to zero is returned.

## Deriving statements

5. **`src/balance.rs`** — `Balances` and `balance_at`, the balance-at-a-past-
   -instant primitive everything else composes from.
6. **`src/statements.rs`** — `cash_movement`, `holdings`, `trial_balance`.
   All are views over the journal; none makes a policy judgment.

## Ingest, verification, and the CLI

7. **`src/lib.rs`** — how bytes on disk become a journal:
   - `from_record_dir` (v6 records) and `from_block_dir` (block streams).
   - `Verify` — the block-proof policy (`Against { genesis }` vs `Trust`).
   - `Attestation` + `from_block_dir_attested` — verify-then-report, returning
     the evidence of what was checked.
   - `SourceError` — the error vocabulary (`ProofInvalid`, `Stream`, …).
8. **`src/main.rs`** — the CLI; auto-detects record vs block dirs, enforces
   verification for blocks.

## Presentation

9. **`src/report/mod.rs`** — `AssuranceReport` (data model + `to_html`) and
   `subledger_csv`. The renderer is feature-free; only `from_attested` needs
   `block-proofs`.
10. **`src/report/assurance.css`** — the report stylesheet, edited as CSS and
    pulled in with `include_str!`.

## Tests and examples

- **`tests/fixture_smoke.rs`** — record-era pipeline over real mainnet files.
- **`tests/block_ingest.rs`** — block verify-then-report, including tamper
  rejection at the container *and* proof layers.
- **`examples/audit_report.rs`** / **`examples/subledger_csv.rs`** — thin
  drivers over the library. Examples are drivers, never homes for logic.

## The one-paragraph mental model

Bytes are parsed and verified by `hiero-streams`, folded into an exact,
conserved `Journal` by `journal.rs`, projected into provable statements by
`statements.rs`, and rendered for humans or spreadsheets by `report.rs`.
`lib.rs` wires ingest and verification; `money.rs` guarantees the arithmetic
is exact. See [ARCHITECTURE.md](ARCHITECTURE.md) for the why.
