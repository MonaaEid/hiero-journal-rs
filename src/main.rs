//! `hiero-journal` CLI — build a journal from a directory of record files
//! and print statements. Dependency-light arg parsing, matching the
//! `hiero-streams` house style (no clap).
//!
//! ```text
//! hiero-journal movements     --dir <dir> --account 0.0.123 [--from YYYY-MM-DD] [--to YYYY-MM-DD]
//! hiero-journal holdings      --dir <dir> --account 0.0.123 [--as-of YYYY-MM-DD]
//! hiero-journal trial-balance --dir <dir>
//! ```

use hiero_journal::{
    from_block_dir, from_record_dir, statements, Decimals, Journal, SourceError, Verify,
};
use std::collections::HashMap;
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(cmd) = args.first().cloned() else {
        return usage();
    };
    let opts = parse_opts(&args[1..]);

    let dir = match opts.get("dir") {
        Some(d) => d,
        None => {
            eprintln!("error: --dir <dir> is required\n");
            return usage();
        }
    };

    let journal = match load_journal(dir, &opts) {
        Ok(j) => j,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };

    // Optional token decimals for display: "0.0.123:6,0.0.456:2".
    let decimals = match opts.get("decimals") {
        Some(spec) => match Decimals::from_pairs(spec) {
            Ok(d) => d,
            Err(e) => {
                eprintln!("error: bad --decimals: {e}");
                return ExitCode::FAILURE;
            }
        },
        None => Decimals::new(),
    };

    // Conservation is the whole crate's premise — check it up front and
    // fail loudly rather than print figures that don't tie out.
    let breaks = journal.conservation_breaks();
    if !breaks.is_empty() {
        eprintln!("CONSERVATION BROKEN in {} transaction(s):", breaks.len());
        for b in breaks.iter().take(5) {
            eprintln!(
                "  {} {} residual {}",
                b.consensus_timestamp, b.asset, b.residual
            );
        }
        return ExitCode::FAILURE;
    }

    match cmd.as_str() {
        "movements" => movements(&journal, &opts, &decimals),
        "holdings" => holdings(&journal, &opts, &decimals),
        "trial-balance" => trial_balance(&journal, &opts),
        _ => usage(),
    }
}

fn movements(journal: &Journal, opts: &HashMap<String, String>, decimals: &Decimals) -> ExitCode {
    let Some(account) = opts.get("account") else {
        eprintln!("error: --account is required for movements");
        return ExitCode::FAILURE;
    };
    let mv = statements::cash_movement(
        journal,
        account,
        opts.get("from").map(String::as_str),
        opts.get("to").map(String::as_str),
    );

    let period = match (&mv.from_day, &mv.to_day) {
        (Some(f), Some(t)) => format!("{f} .. {t}"),
        (Some(f), None) => format!("{f} .."),
        (None, Some(t)) => format!(".. {t}"),
        (None, None) => "all time".to_string(),
    };
    println!("Cash movements — {account}  ({period})\n");

    for line in &mv.lines {
        println!(
            "    {:<16} in {:>20}   out {:>20}   net {:>20}",
            line.kind.as_str(),
            line.asset.render_with(line.inflow, decimals),
            line.asset.render_with(line.outflow, decimals),
            line.asset.render_with(line.net(), decimals),
        );
    }

    println!();
    print_position("Opening", &mv.opening, decimals);
    print_position("Closing", &mv.closing, decimals);
    println!("\n  ties out: {}", mv.ties_out());
    ExitCode::SUCCESS
}

fn holdings(journal: &Journal, opts: &HashMap<String, String>, decimals: &Decimals) -> ExitCode {
    let Some(account) = opts.get("account") else {
        eprintln!("error: --account is required for holdings");
        return ExitCode::FAILURE;
    };
    let pos = statements::holdings(journal, account, opts.get("as-of").map(String::as_str));
    let as_of = opts.get("as-of").map(String::as_str).unwrap_or("latest");
    println!("Holdings — {account}  (as of {as_of})\n");
    print_position("Holdings", &pos, decimals);
    ExitCode::SUCCESS
}

fn trial_balance(journal: &Journal, opts: &HashMap<String, String>) -> ExitCode {
    let tb = statements::trial_balance(
        journal,
        opts.get("from").map(String::as_str),
        opts.get("to").map(String::as_str),
    );
    println!("Trial balance ({} entries in book)\n", journal.len());
    if tb.is_empty() {
        println!("  balanced: every asset sums to zero ✓");
    } else {
        println!("  UNBALANCED:");
        for (asset, residual) in &tb {
            println!("    {} residual {}", asset, asset.render(*residual));
        }
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

fn print_position(label: &str, pos: &hiero_journal::Balances, decimals: &Decimals) {
    if pos.is_empty() {
        println!("  {label}: (empty)");
        return;
    }
    println!("  {label}:");
    for (asset, amount) in pos {
        println!("    {:>24}", asset.render_with(*amount, decimals));
    }
}

/// Load a journal from `dir`, auto-detecting record (v6) vs block streams.
/// Record dirs load directly. Block dirs are verified against `--genesis`
/// (verify-then-report) unless `--trust` is passed to skip it.
fn load_journal(dir: &str, opts: &HashMap<String, String>) -> Result<Journal, String> {
    match from_record_dir(dir) {
        Ok(j) => Ok(j),
        // First file wasn't a record file → treat the dir as block streams.
        Err(SourceError::UnsupportedFormat(_)) => {
            let genesis_bytes;
            let verify = if let Some(g) = opts.get("genesis") {
                genesis_bytes =
                    std::fs::read(g).map_err(|e| format!("reading genesis '{g}': {e}"))?;
                eprintln!("block stream detected — verifying in-band proofs against {g}");
                Verify::Against {
                    genesis: &genesis_bytes,
                }
            } else if opts.contains_key("trust") {
                eprintln!("block stream detected — WARNING: --trust set, proofs NOT verified");
                Verify::Trust
            } else {
                return Err(
                    "block files detected — pass --genesis <genesis.blk.gz> to verify \
                            their proofs, or --trust to skip verification (not provable)"
                        .to_string(),
                );
            };
            from_block_dir(dir, &verify).map_err(|e| e.to_string())
        }
        Err(e) => Err(e.to_string()),
    }
}

/// Minimal `--key value` / `--key=value` parser.
fn parse_opts(args: &[String]) -> HashMap<String, String> {
    let mut map = HashMap::new();
    let mut i = 0;
    while i < args.len() {
        let a = &args[i];
        if let Some(key) = a.strip_prefix("--") {
            if let Some((k, v)) = key.split_once('=') {
                map.insert(k.to_string(), v.to_string());
                i += 1;
            } else if i + 1 < args.len() && !args[i + 1].starts_with("--") {
                map.insert(key.to_string(), args[i + 1].clone());
                i += 2;
            } else {
                map.insert(key.to_string(), String::new());
                i += 1;
            }
        } else {
            i += 1;
        }
    }
    map
}

fn usage() -> ExitCode {
    eprintln!(
        "hiero-journal — provable reports from verified Hedera record streams\n\n\
         USAGE:\n  \
         hiero-journal movements     --dir <dir> --account 0.0.123 [--from YYYY-MM-DD] [--to YYYY-MM-DD] [--decimals 0.0.T:6]\n  \
         hiero-journal holdings      --dir <dir> --account 0.0.123 [--as-of YYYY-MM-DD] [--decimals 0.0.T:6]\n  \
         hiero-journal trial-balance --dir <dir>\n\n\
         --decimals scales token amounts for display (comma-separated token:decimals); HBAR is always 8.\n  \
         <dir> may hold record (v6) OR block-stream files. Block dirs verify against --genesis <0.blk.gz>\n  \
         (or --trust to skip, which forfeits the 'provable' claim)."
    );
    ExitCode::FAILURE
}
