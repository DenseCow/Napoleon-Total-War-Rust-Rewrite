use ntw_data::debugger::{PROBE_BREAKPOINTS, check_probe_script, parse_probe_script};

/// The probe script and the address table must not drift apart.
///
/// This is the check to run **before** handing the script to the user: it is what catches an
/// address that no longer means what the script's comment says. A `bp` on the wrong address
/// does not fail loudly -- `cdb` sets it, the game never hits it, and the log comes back
/// without the line, which reads exactly like "the user did not do the action".
const SCRIPT: &str = include_str!("../../../analysis/fidelity/debugger/0g_promotion_probe.cdb.txt");

#[test]
fn every_breakpoint_in_the_probe_script_is_in_the_address_table() {
    let problems = check_probe_script(SCRIPT);
    assert!(
        problems.is_empty(),
        "the probe script and crates/ntw_data/src/debugger.rs disagree:\n  {}",
        problems.join("\n  ")
    );
}

#[test]
fn the_probe_arms_every_breakpoint_it_claims_to() {
    // One `.printf "ARMED <name>\n"` per breakpoint, as a top-level line right after its `bp`
    // (so it prints when the bp is set, not when it is hit), and the closing line names the
    // real count, or the reader cannot tell a full log from a partial one.
    let armed = SCRIPT.lines().map(str::trim).filter(|l| l.starts_with(".printf \"ARMED ")).count();
    assert_eq!(
        armed,
        PROBE_BREAKPOINTS.len(),
        "the script's ARMED lines ({armed}) and the address table ({}) disagree",
        PROBE_BREAKPOINTS.len()
    );
    let parsed = parse_probe_script(SCRIPT);
    assert!(parsed.iter().all(|p| p.label.is_some()), "a bp is not followed by its ARMED line");
    assert!(
        SCRIPT.contains(&format!("all {} breakpoints set", PROBE_BREAKPOINTS.len())),
        "the closing line does not name the real breakpoint count"
    );
}

/// Every breakpoint must log a line a reader can act on, and must not stop the game. This is
/// the second half of "a probe that fails silently is worse than no probe".
#[test]
fn every_breakpoint_logs_a_tag_and_continues() {
    let mut problems = Vec::new();
    for p in parse_probe_script(SCRIPT) {
        let first = p.commands.first().map(String::as_str).unwrap_or("");
        // `.printf \"TAG ...`: an upper-case tag first, so each hit line is greppable.
        let tag_ok = first
            .strip_prefix(".printf \\\"")
            .is_some_and(|s| s.chars().next().is_some_and(|c| c.is_ascii_uppercase()));
        if !tag_ok {
            problems.push(format!("line {}: the bp does not start by printing a tag: {first}", p.line));
        }
        if !p.continues {
            problems.push(format!("line {}: the bp does not continue, it would stop the game", p.line));
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

/// The probe is read-only against the game: every bp command is printing, dumping memory as
/// text or continuing, and the whole-script check (which also limits the top-level lines)
/// passes.
#[test]
fn the_probe_only_reads() {
    for p in parse_probe_script(SCRIPT) {
        for c in &p.commands {
            let head = c.split_whitespace().next().unwrap_or("");
            assert!([".printf", "du", ".echo", "gc"].contains(&head), "line {}: {c}", p.line);
        }
    }
    assert!(check_probe_script(SCRIPT).is_empty());
}

/// The price is the whole point of the probe, so its return must be instrumented *and* the
/// neighbouring `-1` return must be too. They are four bytes apart; breaking on the wrong
/// one is silent, and it is the mistake this file exists because of.
#[test]
fn the_price_breakpoint_and_its_minus_one_neighbour_are_both_present() {
    assert!(SCRIPT.contains("Napoleon+0x4E279C"), "the land price return is not instrumented");
    assert!(
        SCRIPT.contains("Napoleon+0x4E27A0"),
        "the -1 return is not instrumented, so 'no record' would look like 'no output'"
    );
    // Same shape for the naval pair, at 0x38 and 0x3C.
    for (value, none) in [("0x4E27BC", "0x4E27C0"), ("0x4E2A6C", "0x4E2A70")] {
        assert!(SCRIPT.contains(&format!("Napoleon+{value}")), "naval value return {value} missing");
        assert!(SCRIPT.contains(&format!("Napoleon+{none}")), "naval -1 return {none} missing");
    }
}

/// The run sheet has to stand on its own: a person who has never read 0-G's notes has to be
/// able to run the probe and come back with an answer. These are the three things they cannot
/// work out from the log alone.
#[test]
fn the_run_sheet_explains_how_to_run_it_and_what_a_missed_breakpoint_means() {
    let sheet = include_str!("../../../analysis/fidelity/debugger/0G_PROMOTION_PROBE_2026-10-05.md");
    for needle in [
        "cdb -pn Napoleon.exe -pd -cf",
        "target\\tmp\\gh\\promotion_probe.log",
        "promote_general_in_field",
        "never hits",
    ] {
        assert!(sheet.contains(needle), "the run sheet does not mention {needle:?}");
    }
    // And the script's own header carries the command too, since the sheet may be lost.
    assert!(SCRIPT.contains("cdb -pn Napoleon.exe -pd -cf"));
}