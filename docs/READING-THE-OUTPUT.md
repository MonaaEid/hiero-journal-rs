# Reading the output

What the figures actually mean, in plain language. Two outputs: the CSV
sub-ledger and the HTML assurance report.

## The one-line summary

The tool turns a blockchain's raw transaction stream into trustworthy
accounting statements — and proves the numbers came from the real network,
untampered. Two halves: **prove the data is genuine** (verification) and
**turn it into accounting** (balances that add up).

## A CSV row, decoded

```
1783123259.905485000,2026-07-04,consensusSubmitMessage,22,Transfer,HBAR,-4027588200,-4027588200
```

| Column | Value | Meaning |
|---|---|---|
| `consensus_timestamp` | `1783123259.905485000` | The exact instant the network agreed this happened (seconds.nanoseconds since 1970). The tamper-proof "when" every figure traces back to. |
| `day` | `2026-07-04` | That instant as a calendar day, for grouping into periods. |
| `tx_type` | `consensusSubmitMessage` | The kind of transaction (here, posting a message to a topic). |
| `result_code` | `22` | Outcome. On Hedera, **22 = SUCCESS**. |
| `entry_kind` | `Transfer` | Our classification: ordinary movement (vs Fee, Staking reward, Mint, Burn). |
| `asset` | `HBAR` | The currency (HBAR, or a token id). |
| `amount` | `-4027588200` | Change to *this* account, in **tinybar** (1 ℏ = 100,000,000 tinybar). Negative = money left. So **−40.27588200 ℏ**. |
| `running_balance` | `-4027588200` | This account's cumulative balance after this row. |

**In one sentence:** *at this exact moment on 4 July 2026 this account
successfully paid out 40.28 ℏ; its running total is now −40.28 ℏ.*

Amounts are exact integers in the smallest unit — the `asset` column names
the unit. To show a token as `1.50` rather than `1500000`, supply its decimal
scale (see `Decimals` / the CLI `--decimals` flag). HBAR is always 8 places.

## The assurance report, decoded

**Section 1 — Verification attestation** answers *"is this data real?"*
Transactions arrive from the network in signed blocks. The tool recomputed
the block's fingerprint (its "merkle root") and checked the network's
signature over it. `2 / 3 signers` means 2 of 3 nodes signed — enough.
**VERIFIED** means the data is genuinely what the network produced, not
edited afterward.

**Section 2 — Reconciliation** answers *"does the accounting add up?"* Every
account's balance is listed and they sum to exactly **0.00000000 ℏ**. In
double-entry accounting, correct books cancel to zero — every unit came from
somewhere and went somewhere. `supply:HBAR` is the "where money was
originally created" account (the genesis issuance of the initial supply).
**conservation holds** means no unit appeared or vanished.

## How you know it's right (not just that it says so)

- **It refuses to lie.** If even one tinybar doesn't add up, the tool won't
  print a statement — it names the exact transaction that broke.
- **Tampering is rejected.** Alter a byte in a block and verification fails
  with `block proof INVALID`; there is a test proving it.
- **No rounding, ever.** Amounts are exact integers, so the books can't drift.

## The version you'd say to someone

> It reads Hedera's cryptographically-signed transaction stream, verifies the
> data is authentic, and produces double-entry accounting statements that
> provably balance — the trustworthy data layer under financial reporting.
