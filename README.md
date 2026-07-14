# hiero-journal (Rust)

A prototype of an **audit-grade double-entry journal** and the financial statements that
are views over it — cash flow, balance position, trial balance — derived
from Hiero consensus streams.

It reads its data from
[`hiero-streams`](https://github.com/hiero-hackers/hiero-streams-rs), which
parses **and cryptographically verifies** the signed record/block stream
the network itself publishes. So every figure this crate prints traces back
to consensus-signed output, not to a block explorer's decode you have to
take on trust.

## Documentation

- [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) — the design, the three invariants, the provable/policy boundary
- [docs/METHODOLOGY.md](docs/METHODOLOGY.md) — basis of preparation: exactly how every figure is derived (for auditors)
- [docs/CODE-TOUR.md](docs/CODE-TOUR.md) — a guided reading order through the modules
- [docs/READING-THE-OUTPUT.md](docs/READING-THE-OUTPUT.md) — what every figure in the CSV and report means
- [docs/TESTING.md](docs/TESTING.md) — what the fixtures do and don't cover, honestly

## Why this layer

The ledger records *transactions*. Accounting needs a *double-entry
sub-ledger with provable balances*. The gap between those two is this crate.
It rests on three invariants:

1. **Exact integer arithmetic.** Every amount is a signed integer in the
   asset's smallest unit (tinybar for HBAR, base units for tokens). No float
   ever touches a balance. Amounts are `i128` and every accumulation is
   *checked* — a single leg fits `i64`, but cumulative gross flow on a hot
   account can exceed `i64`'s ~92-billion-ℏ ceiling, and a silent wrap in a
   balance is a catastrophe, so overflow panics rather than wraps.
2. **Per-transaction conservation.** Postings for a transaction net to zero
   in every asset (`Journal::conservation_breaks`). Because `hiero-streams`
   proves the underlying stream is gapless and un-reordered, this is a
   *structural* guarantee, not just an arithmetic tripwire.
3. **Checkpoint-anchored balances.** Balances fold the journal; at full-
   history scale you persist periodic self-computed checkpoints and replay
   only the delta since the latest — no external snapshot needed, since the
   stream carries all history from genesis.

## What it gets right (the non-obvious parts)

- **Fees are separated from principal.** A network fee is only separable on
  the payer's line, where it is exactly `charged_fee_tinybar`. The payer's
  net leg is split into a `Fee` debit and a principal remainder — and because
  `fee + principal == original net`, the split preserves conservation.
- **Mint/burn conserve too.** Token supply changes make the transfer list
  *not* net to zero. The residual is booked to a synthetic per-token
  `supply:<token>` account — but only under `tokenMint`/`tokenBurn`/
  `tokenWipe`, so a real stream gap can't hide as a fake mint.
- **Failed transactions are kept.** A failed transaction still charged its
  fee, and its record already omits the reverted principal — so the legs
  as-parsed are exactly what belongs in the books.

## Reports — provable only, by design

The scope is deliberately narrow: everything here is derivable *from the
ledger* and traces to consensus timestamps. Nothing makes an accounting-
policy judgment.

| Report | What it is | Nature |
|---|---|---|
| **Cash movements** (`statements::cash_movement`) | Inflows/outflows/net by period, grouped by provable `EntryKind`, with opening→closing tie-out | Fact |
| **Holdings** (`statements::holdings`) | Asset position at any past instant | Fact |
| **Trial balance** (`statements::trial_balance`) | Sums to zero in every asset — whole-book consistency | Proof |

The only classification the crate makes is `EntryKind` (Transfer, Network
fee, Staking reward, Mint, Burn) — because each is derivable from the ledger.

**Explicitly out of scope** (it's opinion over external inputs, not a
provable fact, so it belongs in a separate engine):

- GAAP sections (Operating / Investing / Financing)
- revenue-vs-capital classification / income statement
- valuation / FX into a reporting currency
- cost basis (FIFO / LIFO / average) and realized/unrealized gains

Keeping these *out* is what lets every number this crate prints trace back
to a consensus signature. A later `hiero-journal-value` engine can layer
them on via `PriceSource` / `CostBasis` traits, with its outputs honestly
labeled policy-dependent rather than provable.

## CLI

```sh
hiero-journal movements     --dir <dir> --account 0.0.123 [--from YYYY-MM-DD] [--to YYYY-MM-DD]
hiero-journal holdings      --dir <dir> --account 0.0.123 [--as-of YYYY-MM-DD]
hiero-journal trial-balance --dir <dir>
```

`<dir>` may hold **record files** (`.rcd`/`.rcd.gz`, v6) **or block-stream
files** (`.blk`/`.blk.gz`, HIP-1056) — the format is auto-detected. The CLI
refuses to print figures if conservation is broken.

### Verify-then-report (block streams)

A block carries its proof in-band, but *present* ≠ *checked* — until
verified, you are trusting whoever handed you the block. So block dirs are
**verified against the genesis block by default**; a block whose proof fails
never reaches the journal:

```sh
hiero-journal trial-balance --dir blocks/ --genesis blocks/0.blk.gz
#   block stream detected — verifying in-band proofs against blocks/0.blk.gz
#   balanced: every asset sums to zero ✓

hiero-journal trial-balance --dir blocks/ --trust   # skip verification (NOT provable)
```

Verification (recomputed merkle root, hinTS threshold signature,
Schnorr/WRAPS suffix) lives in `hiero-streams` behind the `block-proofs`
feature, which this crate turns **on by default**. Build
`--no-default-features` for a lean record-only (v6) build with no crypto
stack. In the library:

```rust
use hiero_journal::{from_block_dir, Verify};

let genesis = std::fs::read("blocks/0.blk.gz")?;
let journal = from_block_dir("blocks/", &Verify::Against { genesis: &genesis })?;
// every block verified, or the call errors — nothing unverified is booked
```

### Token decimals

HTS amounts are stored (and summed) in a token's **base unit** — the decimal
scale is token metadata, not on the transfer legs — so tokens print in base
units unless you supply the scale:

```sh
hiero-journal holdings --dir <dir> --account 0.0.123 --decimals 0.0.731861:6
#   -1181217986 0.0.731861   →   -1181.217986 0.0.731861
```

Decimals are a **display** concern only: the stored balance never changes,
it is just formatted at the given scale (exactly, by integer split). Feed
the scales from a future `TokenCreate` ingest (the decimal scale is set
on-chain when the token is created). HBAR is always 8.

## Library

```rust
use hiero_journal::{from_record_dir, statements};

let journal = from_record_dir("downloaded/")?;
assert!(journal.conservation_breaks().is_empty());       // proves it ties out

let mv = statements::cash_movement(&journal, "0.0.98", Some("2026-04-01"), Some("2026-06-30"));
assert!(mv.ties_out());                                   // opening + Σ == closing
```

## Examples

Two, one per output surface — a human report and a machine export:

```sh
# Human: a print-ready HTML assurance report (attestation + reconciliation)
cargo run --example audit_report                 # writes ./audit_report.html
cargo run --example audit_report -- report.html  # or a path you choose

# Machine: per-account double-entry sub-ledger as CSV, with running balance
cargo run --example subledger_csv -- tests/fixtures/mainnet 0.0.800 > ledger.csv
```

### PDF

The report is print-ready (it carries `@media print` styles), so any browser
turns it into a filing-quality PDF — `open audit_report.html`, then Print →
Save as PDF. Headless, for automation:

```sh
"/Applications/Google Chrome.app/Contents/MacOS/Google Chrome" \
  --headless --no-pdf-header-footer \
  --print-to-pdf=audit_report.pdf "file://$PWD/audit_report.html"
```

No PDF engine is baked into the crate — it emits HTML and lets the browser's
print pipeline produce the PDF, which keeps the dependency surface small.

## Where it sits

```
consensus node → record/block stream (signed) → hiero-streams (verify) → hiero-journal → statements
```

The verified stream is the **sole** source of truth. Being the only source —
not one feed among several — is what keeps every figure provable.

**Non-goal:** mirror-node ingest is deliberately out of scope. Mirror data is
operator-attested, not cryptographically verified, so feeding it in would
dilute the provable claim.

## Scope / honesty boundary

This produces audit-**ready** schedules with a provable trail back to
consensus timestamps — **not** GAAP/IFRS financial statements. Accrual and
valuation judgments are the finance team's to make. Positioning: the
source-of-truth data layer under your statements, not an accountant in a box.

## Status

Early but real. Both eras are wired end-to-end: record (v6) files, and
block streams (HIP-1056) **with in-band proof verification on by default**.
Genesis HBAR issuance and token mint/burn are handled as supply contra-
entries, so the book conserves across both eras. Scope is intentionally held
to the provable core — cash movements, holdings, trial balance. Valuation
and cost basis are a separate future engine, not feature creep into this
crate.

Next: a block-node gRPC subscription adapter (live ingest — reassemble the
block-item stream into blocks, then the same verify-then-report path).

## License

Apache-2.0
