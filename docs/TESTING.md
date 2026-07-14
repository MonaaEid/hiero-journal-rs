# Testing & data coverage

An honest account of what the tests prove and what they don't — so nobody
mistakes "verified on representative data" for "proven complete."

## The fixtures

All fixtures are **real, signed mainnet data** copied from `hiero-streams`,
so a green run means the pipeline agrees with genuine consensus output.

| Fixtures | Files | Yields | Exercises |
|---|---|---|---|
| `tests/fixtures/mainnet/` | 3 × `.rcd.gz` (v6 records) | ~93 postings | Record-era parse → journal → statements |
| `tests/fixtures/blocks/` | `0.blk.gz` (genesis) + `467.blk.gz` | 6 postings | Block-era verify-then-report |

## What the tests prove

- **The end-to-end pipeline works on real data** — parse, verify, ingest,
  reconcile, render — for both stream eras.
- **Conservation holds** across everything ingested, including the tricky
  cases: token mint, staking rewards, and the genesis 50-billion-ℏ issuance.
- **Both proof schemes verify.** The two blocks are deliberately chosen to
  cover *both* paths: genesis aggregate-Schnorr and settled-history WRAPS.
- **Tampering is rejected** at two layers — a corrupted gzip container
  (parse failure) and altered content under a valid gzip (proof failure) —
  and nothing unverified reaches the journal.
- **Exact arithmetic** — `money::add` overflow, CSV escaping, and the report
  data model are unit-tested directly.

Run everything with `cargo test` (needs the default `block-proofs` feature
for the block tests).

## What the tests do NOT cover — known gaps

Be upfront about these when presenting the crate:

- **Transaction-type variety is thin.** The fixtures contain
  `cryptoTransfer`, `consensusSubmitMessage`, `contractCall`, `tokenMint`,
  `cryptoCreateAccount`, and staking activity. Many types are absent
  (airdrops, scheduled transactions, token associate/freeze, etc.).
- **No NFT support.** The journal handles HBAR and *fungible* token transfers
  only. Non-fungible (serial-numbered) transfers are not modelled.
- **Not a scale or performance test.** ~100 postings, not millions. Full-
  history behaviour (snapshot-anchored replay) is designed for but unproven
  at volume here.
- **No live ingest.** A block-node gRPC subscription adapter (live blocks)
  is not built or tested; ingest is from stream files on disk.
- **Single network shard.** The block fixtures happen to be shard `11.12.x`;
  mainnet `0.0.x` is covered by the record fixtures.

## Strengthening coverage (future work)

In rough priority order: add fixtures covering more transaction types and a
**failed** transaction that still charges a fee; add NFT-transfer handling
plus a fixture; property-test the conservation invariant over generated
transaction sets; add a larger record window to exercise multi-day
`balance_at`.
