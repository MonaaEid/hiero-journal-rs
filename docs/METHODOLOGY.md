# Methodology (basis of preparation)

How every figure is derived, stated precisely enough for an auditor to
verify the logic independently. All outputs (the HTML assurance report and
the CSV sub-ledger) are presentations of the one derivation described here.

## 1. Basis

- **Source.** Cryptographically verified consensus stream only (see
  [ARCHITECTURE.md](ARCHITECTURE.md)). No unverified feed contributes.
- **Measurement.** Every amount is a signed integer in the asset's smallest
  unit — **tinybar** for HBAR (1 ℏ = 100,000,000 tinybar), base units for a
  token. No floating point is used at any stage; decimals affect *display*
  only, never a stored value.
- **Arithmetic width.** Amounts and all accumulations are 128-bit signed
  integers with **checked** addition — an overflow aborts rather than wraps.
- **Recognition.** Cash basis: a movement is recognised at its consensus
  timestamp. No accruals, no valuation, no revenue/capital classification.

## 2. From a transaction to postings

Each transaction is decomposed into double-entry postings (`LedgerEntry`).
The rules, in order:

### 2.1 HBAR legs → net per account
Sum the transaction's HBAR transfer legs per account (fee legs are already
included on-ledger). Accounts netting to zero produce no posting.

### 2.2 Fee decomposition
A network fee is separable only on the **payer's** line, where it equals the
transaction's `charged_tx_fee`. For the payer, the net leg is split:

```
fee_posting        = − charged_fee            (EntryKind::Fee, always a debit)
principal_posting  = net_leg + charged_fee    (Transfer or StakingReward)
```

Because `fee + principal = net_leg`, the split is **conservation-preserving**
— it re-labels the payer's movement, it does not change any total.

### 2.3 Staking rewards (heuristic)
A positive leg to an account in a transaction that also **debits the reward
account `0.0.800`** is classified `StakingReward` rather than `Transfer`.
This is a documented heuristic to separate reward income from ordinary
receipts; it affects classification only, never an amount.

### 2.4 Token legs
Token transfer legs are booked directly (tokens carry no HBAR fee). Amounts
are token base units.

### 2.5 Supply changes → synthetic contra account
Some transactions legitimately do **not** net to zero because supply is
created or destroyed. The residual is booked to a synthetic `supply:*`
account so the book still closes:

- **Token mint/burn/wipe.** Only under transaction types `tokenMint`,
  `tokenBurn`, `tokenWipe` is a non-zero token residual booked to
  `supply:<token-id>` (`Mint` if positive, `Burn` if negative). A residual
  under any other type is **not** absorbed — it surfaces as a conservation
  break (§3).
- **Genesis HBAR issuance.** The initial supply is created into the treasury
  with no debit. A non-zero HBAR residual is booked to `supply:HBAR` **only
  when the transaction is single-sided** (all legs the same direction — money
  with no source). This is safe because HBAR is never minted after genesis:
  every normal HBAR transaction has a debit, so a genuine stream gap (which
  carries mixed-sign legs) still surfaces as a break rather than being masked.

### 2.6 Failed transactions
Retained, not filtered. A failed transaction still charged its fee, and its
record already omits the reverted principal — so its legs as-parsed are
exactly what belongs in the books.

## 3. The conservation check

Postings are grouped by `(consensus_timestamp, asset)` and summed. A correct
book sums to **zero** in every group. Any non-zero residual is a
`ConservationBreak`, naming the transaction and the amount it is off by. The
tools **refuse to emit a statement when any break exists** — an unbalanced
book is never presented as if it balanced.

## 4. Balances and statements

- **`balance_at(account, day)`** — the signed sum of the account's postings
  whose day is `≤ day` (inclusive), per asset, dropping assets that net to
  zero. `day = None` folds the whole journal.
- **Holdings** — `balance_at` at a point in time (asset side only; no
  liabilities or equity, which are not on-ledger facts).
- **Cash movements** — postings in a window, grouped by `(EntryKind, asset)`
  into gross inflow (amount ≥ 0), gross outflow (|amount| for amount < 0),
  and net. Opening balance is everything strictly before the window; closing
  = opening + Σ(window movements). The report asserts `opening + Σ = closing`
  per asset (`ties_out`).
- **Trial balance** — the sum of every posting per asset over a window. A
  conserved book sums to zero in every asset; a non-empty result is an
  imbalance.

Period bounds use the `YYYY-MM-DD` day string (inclusive), which sorts
correctly without a date library.

## 5. Verification methodology (block streams)

"Verified" means, for each block, all of the following checked out before the
block's transactions were booked (verify-then-report — a failure books
nothing):

1. **Merkle root recomputed** from the raw block items and matched.
2. **hinTS threshold signature** over that root validated.
3. **Scheme suffix** validated — aggregate-Schnorr (genesis / pre-settled)
   or WRAPS (settled history).

Each block's `Attestation` records the evidence: block number, recomputed
root, scheme, hinTS result, signer count, and overall verdict. These populate
the assurance report's attestation table so a reviewer inspects *what was
checked*, not a boolean.

## 6. Rounding and precision

None. All arithmetic is exact integer arithmetic in the smallest unit.
Display scaling by a token's decimals (or HBAR's 8) is a formatting step
applied to the exact integer; it never rounds a stored balance.

## 7. Per-report presentation notes

- **Assurance report (HTML).** §5 attestation table, then a §4 reconciliation
  listing every account's position (sorted) with a total that must equal
  zero. The "Scope & basis" footer restates §1.
- **Sub-ledger (CSV, RFC 4180).** One row per posting in consensus order,
  with a per-asset running balance folded by checked addition. Columns:
  `consensus_timestamp, day, tx_type, result_code, entry_kind, asset, amount,
  running_balance`. Amounts are exact smallest-unit integers; the `asset`
  column names the unit.

## 8. Known limitations

See [TESTING.md](TESTING.md) for the full list. Material to this methodology:
NFT (non-fungible) transfers are not modelled; the staking-reward
classification (§2.3) is heuristic; valuation and accounting-policy
judgments are explicitly out of scope.

## 9. How to verify independently

- Re-run ingest and confirm `conservation_breaks` is empty.
- Trace any report figure to the CSV sub-ledger, then to its
  `consensus_timestamp` — the canonical on-ledger reference.
- Re-verify any block's proof from the stream file with `hiero-streams`.
