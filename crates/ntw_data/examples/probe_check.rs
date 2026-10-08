//! Runs the 0-G promotion probe's own self-check, so a stale breakpoint is caught before
//! the user spends a sitting under `cdb`.
//!
//! ```text
//! cargo run -p ntw_data --example probe_check            # check the script
//! cargo run -p ntw_data --example probe_check -- --list  # print the address table
//! ```
//!
//! Read-only: it touches only the tracked script and this repo. Exit code 1 means at least
//! one breakpoint in the script disagrees with [`ntw_data::debugger::PROBE_BREAKPOINTS`].

use ntw_data::debugger::{PROBE_BREAKPOINTS, check_probe_script, parse_probe_script};

const SCRIPT: &str = include_str!("../../../analysis/fidelity/debugger/0g_promotion_probe.cdb.txt");

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();

    if args.iter().any(|a| a == "--list") {
        println!("{:<16} {:>10} {:>10}  why", "label", "static VA", "offset");
        for b in PROBE_BREAKPOINTS {
            println!(
                "{:<16} 0x{:08X} {:>10}  {}",
                b.name,
                b.static_va,
                b.module_offset_hex(),
                b.why
            );
        }
        println!("\n{} breakpoints", PROBE_BREAKPOINTS.len());
        return;
    }

    let parsed = parse_probe_script(SCRIPT);
    println!("checked {} bp lines against {} table entries", parsed.len(), PROBE_BREAKPOINTS.len());

    let problems = check_probe_script(SCRIPT);
    if problems.is_empty() {
        println!("OK: every breakpoint in the script matches the address table.");
        return;
    }
    eprintln!("FAIL: {} problem(s):", problems.len());
    for p in &problems {
        eprintln!("  - {p}");
    }
    std::process::exit(1);
}