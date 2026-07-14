# Architecture

`hiero-journal` turns Hedera's cryptographically-signed transaction stream
into **trustworthy accounting statements** — and keeps the line between what
is *provable from the ledger* and what is *accounting policy* sharp.

## The problem it solves

A ledger records *transactions*. Finance needs a *double-entry sub-ledger
with provable balances*: what did each account hold, when, and does it all
add up. The gap between those two is this crate.

Two independent guarantees have to hold at once:

1. **The data is authentic** — it is what the network signed, untampered and
   gapless. (Verification.)
2. **The derived numbers are consistent** — every unit is conserved and the
   book closes to zero. (Conservation.)

Neither requires inventing cryptography or re-deriving accounting rules; the
crate composes an existing verifier with a small, exact accounting core.

## Where it sits

```
consensus node
   │  (signs)
   ▼
record / block stream            hiero-streams          hiero-journal
     .rcd.gz / .blk.gz  ─────▶   parse + verify   ─────▶  journal ─▶ statements
                                                             │           report
                                                             ▼
                                                     (self checkpoints)
```

- **Sole source of truth:** the verified stream (via `hiero-streams`). The
  proof travels *in-band* with block-era data, so authenticity is intrinsic —
  and being the *only* source, not one feed among several, is what keeps
  every figure provable.

`hiero-journal` never performs network I/O in its library core; it consumes
`hiero-streams`' parsed transactions and file bytes.

### Non-goals

- **Mirror-node ingest is deliberately out of scope.** A mirror node's REST
  data is operator-attested, not cryptographically verified, so feeding it in
  would dilute the "provable" claim the whole crate rests on. The stream is
  the only source. (A separate tool could layer unverified mirror data on top,
  clearly labelled as such — but that is not this crate.)
- **No valuation or accounting policy** (see the boundary below).

## The three invariants

Everything rests on these; break one and the "audit-grade" claim collapses.

1. **Exact integer arithmetic.** Every amount is a signed integer in the
   asset's smallest unit (tinybar for HBAR). Amounts are `i128` and every
   accumulation is *checked* — a single leg fits `i64`, but cumulative flow
   exceeds it, and a silent wrap in a balance is a catastrophe. No float ever
   touches a value. (`money.rs`)
2. **Per-transaction conservation.** Postings for a transaction net to zero
   in every asset (`Journal::conservation_breaks`). Because the stream is
   proven gapless, this is *structural*, not just an arithmetic tripwire.
   Supply changes (token mint/burn, genesis HBAR issuance) are booked to a
   synthetic `supply:*` contra account so the book still closes. (`journal.rs`)
3. **Checkpoint-anchored balances.** Balances fold the journal. At full scale
   you replay from genesis once and persist periodic *self-computed*
   checkpoints, then replay only the delta since the latest — no external
   snapshot needed, because the stream carries all history. (`balance.rs`)

## The provable / policy boundary

The scope is deliberately narrow: the crate emits only what the ledger itself
justifies. Anything requiring an external input or an opinion is *out*, by
design, so every figure traces to a consensus signature.

| In scope (provable) | Out of scope (policy — a separate future engine) |
|---|---|
| Cash movements, holdings, trial balance | GAAP sections (Operating/Investing/Financing) |
| `EntryKind` (Transfer, Fee, Staking reward, Mint, Burn) | Revenue-vs-capital / income statement |
| Conservation, verification attestation | Valuation / FX into a reporting currency |
| | Cost basis (FIFO/LIFO/avg), realized/unrealized gains |

The out-of-scope items need price oracles and policy choices; they belong in a
separate, clearly-labelled valuation engine (`PriceSource`/`CostBasis` traits),
whose outputs are *defensible under a stated policy*, not *provable*.

## Data flow through the modules

```
bytes ──▶ lib.rs (ingest, verify)
             │
             ├─ from_record_dir / from_block_dir[_attested]
             │       │
             │       ▼
             │   journal.rs  ──▶ LedgerEntry (exact, conserved)
             │       │
             ├───────┼─ balance.rs   (balance_at)
             │       ▼
             │   statements.rs (cash_movement, holdings, trial_balance)
             │       │
             └───────┴─▶ report.rs (AssuranceReport → HTML, subledger_csv)
```

| Module | Responsibility |
|---|---|
| `money.rs` | Exact `i128` arithmetic (checked add) and formatting. The exactness discipline. |
| `asset.rs` | The `Asset` value type (HBAR / token) and rendering. |
| `token.rs` | `Decimals` registry — display scaling for tokens (never divides a balance). |
| `journal.rs` | Core: `LedgerEntry`, ingest, fee/supply decomposition, conservation. |
| `balance.rs` | `Balances` and the `balance_at` primitive. |
| `statements.rs` | Provable reports: cash movements, holdings, trial balance. |
| `report.rs` | Presentation: HTML assurance report + RFC 4180 CSV export. |
| `lib.rs` | Source/ingest, `Verify` policy, `Attestation`, error types. |
| `main.rs` | The CLI. |

## Verify-then-report

A block carries its proof in-band, but *present* ≠ *checked*. Block ingest
verifies each block against the genesis bootstrap **before** its transactions
reach the journal:

```
extract proof material ─▶ resolve bootstrap ─▶ verify_block_proof
   │                                                │
   │  fail ──▶ SourceError::ProofInvalid (nothing booked)
   ▼
parse_block ─▶ journal.ingest
```

Verification (recomputed merkle root, hinTS threshold signature,
Schnorr/WRAPS suffix) lives in `hiero-streams` behind `block-proofs`, which
this crate enables **by default**. `from_block_dir_attested` also returns an
`Attestation` per block — the evidence of *what* was checked (root, scheme,
signers), so a reviewer inspects that rather than trusting a boolean.

## Scope boundary

This produces audit-**ready** schedules with a provable trail to consensus
timestamps — **not** GAAP/IFRS financial statements. Accrual and valuation
judgments are the preparer's. Positioning: the source-of-truth data layer
under financial reporting, not an accountant in a box.

See also [METHODOLOGY.md](METHODOLOGY.md) (basis of preparation — exactly how
each figure is derived), [CODE-TOUR.md](CODE-TOUR.md) (where to start reading),
[READING-THE-OUTPUT.md](READING-THE-OUTPUT.md) (what the figures mean), and
[TESTING.md](TESTING.md) (what the fixtures do and don't cover).
