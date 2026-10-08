//! The addresses the 0-G promotion debugger probe breaks on, and a checker for the script
//! that uses them.
//!
//! # Why this exists
//!
//! [`analysis/fidelity/debugger/0g_promotion_probe.cdb.txt`] is handed to the user to run
//! under `cdb`, unattended, possibly long after it was written. A breakpoint on the wrong
//! address does not announce itself: `cdb` sets it happily, the game never hits it, and the
//! log simply comes back missing a line. That failure mode already cost a round — the first
//! version of the script broke on `0x008E27A0`, the `return -1` of the land cost slot,
//! instead of `0x008E279C`, the one that actually returns the price, so the single most
//! important line of the log could never appear.
//!
//! So the addresses live here as data, next to what each one is supposed to be, and the
//! script is checked against them:
//!
//! * every `bp Napoleon+0x...` in the script must correspond to an entry in
//!   [`PROBE_BREAKPOINTS`], and its offset must equal that entry's `static_va - IMAGE_BASE`;
//! * every entry must be used by exactly one `bp` (no orphans in either direction);
//! * every `bp` must be followed by its `.printf "ARMED <name>\n"` line (printed when the bp is
//!   SET -- a command string inside the bp only runs when it is hit);
//! * every entry must carry a `static_va` inside the exe's `.text` and the `first_opcode`
//!   recorded from the sandbox's disassembly (kept only in its ignored `target/tmp/gh/`);
//! * the script must be **read-only**: top-level lines are limited to `*` comments,
//!   `.logopen`, `.echo`, the `ARMED` prints, `bp`, `bl` and `g`, and a bp's command string to
//!   `.printf`, `du`, `.echo` and `gc`. A memory or register write, a `$$` comment (cdb ends it
//!   at the first `;`) or a `#` line (cdb's disassembly search) is reported.
//!
//! The opcode check is the part that actually catches a *stale* address: `Napoleon.exe` is
//! read (never written) when an install is present and the byte at each static VA is compared
//! against what the decompiler showed. That test needs the install and is `#[ignore]`d;
//! the structural checks need nothing and run everywhere.
//!
//! ```
//! use ntw_data::debugger::check_probe_script;
//! let problems = check_probe_script(include_str!(
//!     "../../../analysis/fidelity/debugger/0g_promotion_probe.cdb.txt"
//! ));
//! assert!(problems.is_empty(), "{problems:#?}");
//! ```

use std::collections::BTreeMap;

/// The exe's preferred image base. A probe writes module-relative offsets, so the
/// subtraction has to happen exactly once, here.
pub const IMAGE_BASE: u64 = 0x0040_0000;

/// One breakpoint in the 0-G promotion probe.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProbeBreakpoint {
    /// The label the script prints in its `ARMED` line, so a gap in the log names the
    /// breakpoint that is missing.
    pub name: &'static str,
    /// The static virtual address in `Napoleon.exe`, as the decompiler prints it.
    pub static_va: u32,
    /// What the byte at `static_va` is, from the kept disassembly. `None` when no opcode
    /// was recorded, which downgrades the install test to "address is in .text" only.
    pub first_opcode: Option<u8>,
    /// One line on what the stop is for, and what its registers hold.
    pub why: &'static str,
}

impl ProbeBreakpoint {
    /// The `Napoleon+0x...` offset the script must use.
    pub fn module_offset(&self) -> u32 {
        debug_assert!(self.static_va as u64 >= IMAGE_BASE);
        self.static_va - IMAGE_BASE as u32
    }

    /// The offset as `0x` + uppercase hex, exactly how it is written in the script.
    pub fn module_offset_hex(&self) -> String {
        format!("0x{:X}", self.module_offset())
    }
}

/// Every breakpoint in `analysis/fidelity/debugger/0g_promotion_probe.cdb.txt`.
///
/// The `static_va`s are the ones the decompiler prints; the opcodes are the first byte of
/// each instruction in the kept listings (`target/tmp/gh/`, ignored) re-read on demand.
pub const PROBE_BREAKPOINTS: &[ProbeBreakpoint] = &[
    ProbeBreakpoint {
        name: "cost44-enter",
        static_va: 0x008E_2770,
        first_opcode: Some(0x56), // PUSH ESI
        why: "land unit class vtable slot +0x44; on entry ECX is the class, +0x40 its record",
    },
    ProbeBreakpoint {
        name: "price-land",
        static_va: 0x008E_279C,
        first_opcode: Some(0xC3), // RET, after MOV EAX,[EAX+0x38]: the price
        why: "the land price in EAX -- the value return, NOT the -1 return at 0x008E27A0",
    },
    ProbeBreakpoint {
        name: "cost44-norecord",
        static_va: 0x008E_27A0,
        first_opcode: Some(0xC3), // RET, after OR EAX,0xFFFFFFFF
        why: "land cost slot found no record: EAX = -1",
    },
    ProbeBreakpoint {
        name: "cost38-enter",
        static_va: 0x008E_27B0,
        first_opcode: Some(0xE8), // CALL 0x008E29A0
        why: "naval slot +0x38",
    },
    ProbeBreakpoint {
        name: "cost38-value",
        static_va: 0x008E_27BC,
        first_opcode: Some(0xC3), // RET, after MOV EAX,[EAX+0x38]
        why: "naval slot +0x38 value return",
    },
    ProbeBreakpoint {
        name: "cost38-norecord",
        static_va: 0x008E_27C0,
        first_opcode: Some(0xC3), // RET, after OR EAX,0xFFFFFFFF
        why: "naval slot +0x38 with no record: EAX = -1",
    },
    ProbeBreakpoint {
        name: "cost3c-enter",
        static_va: 0x008E_2A60,
        first_opcode: Some(0xE8), // CALL 0x008E29A0
        why: "naval slot +0x3C",
    },
    ProbeBreakpoint {
        name: "cost3c-value",
        static_va: 0x008E_2A6C,
        first_opcode: Some(0xC3), // RET, after MOV EAX,[EAX+0x3C]
        why: "naval slot +0x3C value return",
    },
    ProbeBreakpoint {
        name: "cost3c-norecord",
        static_va: 0x008E_2A70,
        first_opcode: Some(0xC3), // RET, after OR EAX,0xFFFFFFFF
        why: "naval slot +0x3C with no record: EAX = -1",
    },
    ProbeBreakpoint {
        name: "lookup-found",
        static_va: 0x008E_297F,
        first_opcode: Some(0x5F), // POP EDI, on the path that returns the hash value
        why: "0x008E27D0 normal exit; arg1 (the agent type) is at esp+0x2C, not esp+4",
    },
    ProbeBreakpoint {
        name: "lookup-missing",
        static_va: 0x008E_2989,
        first_opcode: Some(0x5F), // POP EDI, on the path that zeroes EAX
        why: "0x008E27D0 found nothing",
    },
    ProbeBreakpoint {
        name: "hash-entry",
        static_va: 0x00F9_C2A0,
        first_opcode: Some(0x83), // SUB ESP,0x8
        why: "the string hash; ECX is the map (+0x3C bucket count, +0x40 bucket array)",
    },
    ProbeBreakpoint {
        name: "hash-node",
        static_va: 0x00F9_C358,
        first_opcode: Some(0x8B), // MOV EAX,[ESI+0x14]
        why: "the found node: ESI+8 is the key, ESI+0x14 the value the price is read off",
    },
    ProbeBreakpoint {
        name: "treasury",
        static_va: 0x00BA_F500,
        first_opcode: Some(0x83), // SUB ESP,0x18
        why: "FactionEconomics(amount, reason): the treasury move",
    },
    ProbeBreakpoint {
        name: "pool-take",
        static_va: 0x008F_35E0,
        first_opcode: Some(0x8B), // MOV EDX,[ESP+0x4]
        why: "the non-treasury counter both promotions and both hires touch (this+0x6C)",
    },
    ProbeBreakpoint {
        name: "exec-land",
        static_va: 0x008E_1C20,
        first_opcode: Some(0x83), // SUB ESP,0x50
        why: "the land promotion executor",
    },
    ProbeBreakpoint {
        name: "exec-naval",
        static_va: 0x008E_2260,
        first_opcode: Some(0x83), // SUB ESP,0x44
        why: "the naval promotion executor",
    },
    ProbeBreakpoint {
        name: "gate",
        static_va: 0x009E_0AF0,
        first_opcode: Some(0x83), // SUB ESP,0x8
        why: "the promote gate, the unit class's vtable slot +0x40",
    },
    ProbeBreakpoint {
        name: "promote",
        static_va: 0x00A1_A2E0,
        first_opcode: Some(0x56), // PUSH ESI
        why: "sets the character's agent record at +0x1AC and fires CharacterPromoted",
    },
    ProbeBreakpoint {
        name: "hire-general",
        static_va: 0x00A1_64C0,
        first_opcode: Some(0x83), // SUB ESP,0x24
        why: "hire a General from the pool (a different cost source, logged for comparison)",
    },
    ProbeBreakpoint {
        name: "hire-admiral",
        static_va: 0x00A1_6110,
        first_opcode: Some(0x83), // SUB ESP,0x2C
        why: "hire an admiral from the pool (a different cost source, logged for comparison)",
    },
];

/// `.text` of `Napoleon.exe` (`0x00401000 .. 0x013069FF`, read out of the loaded program),
/// used to sanity-check that an address is code at all rather than in `.rdata` or `.data`.
pub const TEXT_RANGE: (u32, u32) = (0x0040_1000, 0x0130_6A00);

/// The three cost slots' value / `-1` return pairs: `(value-return VA, the field the value is
/// loaded from)`. The table above records `RET` at the value return and `RET` four bytes later
/// after `OR EAX,0xFFFFFFFF`, with `MOV EAX,[EAX+field]` just before the first `RET`. Those
/// mnemonics fix the bytes (`8B 40 disp8`, `C3`, `83 C8 FF`, `C3`: the only encodings that fit
/// the four-byte gap), so [`return_pair_bytes`] can check eight bytes per pair instead of one.
pub const RETURN_PAIRS: &[(u32, u8)] = &[(0x008E_279C, 0x38), (0x008E_27BC, 0x38), (0x008E_2A6C, 0x3C)];

/// The eight bytes expected at `value_return - 3 ..= value_return + 4` for a [`RETURN_PAIRS`] entry:
/// `MOV EAX,[EAX+field]; RET; OR EAX,-1; RET`. Derived from the recorded mnemonics (above), not
/// read from a kept dump, so a mismatch on the install means "re-read the disassembly", not
/// necessarily "the address moved".
pub fn return_pair_bytes(field: u8) -> [u8; 8] {
    [0x8B, 0x40, field, 0xC3, 0x83, 0xC8, 0xFF, 0xC3]
}

/// The only commands a breakpoint's command string may run: print, dump memory as text, and
/// continue. All read-only.
const BP_COMMANDS: &[&str] = &[".printf", "du", ".echo", "gc"];

/// One `bp Napoleon+0x...` line found in the script.
#[derive(Debug, PartialEq, Eq)]
pub struct ParsedBreakpoint {
    /// 1-based line number in the script.
    pub line: usize,
    /// The name the `.printf "ARMED <name>\n"` line right after the `bp` prints when it is set.
    pub label: Option<String>,
    /// The offset written after `Napoleon+`.
    pub offset: u32,
    /// The commands of the bp's quoted command string, split on `;` outside inner strings.
    pub commands: Vec<String>,
    /// Whether the last command is `gc` (every breakpoint must log and continue).
    pub continues: bool,
}

/// Split a bp command string on `;`, ignoring semicolons inside its escaped `\"...\"` strings.
fn split_commands(body: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut in_str = false;
    let mut chars = body.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\' && chars.peek() == Some(&'"') {
            in_str = !in_str;
            cur.push(c);
            cur.push(chars.next().unwrap_or('"'));
            continue;
        }
        if c == ';' && !in_str {
            out.push(cur.trim().to_string());
            cur.clear();
            continue;
        }
        cur.push(c);
    }
    if !cur.trim().is_empty() {
        out.push(cur.trim().to_string());
    }
    out
}

/// The name an `ARMED` line prints, if `line` is one: `.printf "ARMED <name>\n"`.
fn armed_name(line: &str) -> Option<String> {
    let rest = line.trim().strip_prefix(".printf \"ARMED ")?;
    let name: String = rest.chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '-').collect();
    (!name.is_empty()).then_some(name)
}

/// Read every `bp Napoleon+0x...` line out of a cdb script. Lines starting with `*` are
/// comments (cdb ignores the rest of such a line, semicolons included).
pub fn parse_probe_script(script: &str) -> Vec<ParsedBreakpoint> {
    let lines: Vec<&str> = script.lines().collect();
    let mut out = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        let code = line.trim();
        let Some(rest) = code.strip_prefix("bp ") else { continue };
        let Some(addr) = rest.trim_start().strip_prefix("Napoleon+") else { continue };
        let end = addr.find(|c: char| !c.is_ascii_hexdigit() && c != 'x' && c != 'X').unwrap_or(addr.len());
        let digits = &addr[..end];
        let Some(offset) =
            digits.strip_prefix("0x").or_else(|| digits.strip_prefix("0X")).and_then(|h| u32::from_str_radix(h, 16).ok())
        else {
            continue;
        };
        let body = match (addr.find('"'), addr.rfind('"')) {
            (Some(a), Some(b)) if b > a => &addr[a + 1..b],
            _ => "",
        };
        let commands = split_commands(body);
        let continues = commands.last().is_some_and(|c| c == "gc");
        let label = lines
            .iter()
            .skip(i + 1)
            .map(|l| l.trim())
            .find(|l| !l.is_empty())
            .and_then(armed_name);
        out.push(ParsedBreakpoint { line: i + 1, label, offset, commands, continues });
    }
    out
}

/// Whether a top-level script line is one the probe may contain: a comment, a blank line, the
/// log, an echo, an `ARMED` print, a breakpoint, the breakpoint list, or the final `g`. Anything
/// else (a memory edit such as `ed` / `eb`, a register write, `.writemem`, a `$$` comment that a
/// `;` would end, a `#` disassembly search) is refused.
fn allowed_top_level(code: &str) -> bool {
    code.is_empty()
        || code.starts_with('*')
        || code.starts_with(".logopen ")
        || code == ".echo"
        || code.starts_with(".echo ")
        || armed_name(code).is_some()
        || code.starts_with("bp ")
        || code == "bl"
        || code == "g"
}

/// Check a cdb script against [`PROBE_BREAKPOINTS`], and that it is read-only. Returns one
/// message per problem found; an empty vector means the script is consistent with the table.
pub fn check_probe_script(script: &str) -> Vec<String> {
    let mut problems = Vec::new();

    for (i, line) in script.lines().enumerate() {
        let code = line.trim();
        if !allowed_top_level(code) {
            let why = if code.starts_with("$$") {
                "a `$$` comment ends at the first `;`, so the rest would run; use `*`"
            } else if code.starts_with('#') {
                "`#` is cdb's disassembly search command, not a comment; use `*`"
            } else {
                "not a command this read-only probe may run"
            };
            problems.push(format!("line {}: {why}: {code}", i + 1));
        }
    }

    let parsed = parse_probe_script(script);
    if parsed.is_empty() {
        problems.push("no `bp Napoleon+0x...` lines found at all -- is this the right file?".into());
        return problems;
    }

    let by_offset: BTreeMap<u32, usize> = parsed.iter().map(|p| (p.offset, p.line)).collect();
    if by_offset.len() != parsed.len() {
        let mut seen = Vec::new();
        for p in &parsed {
            if seen.contains(&p.offset) {
                problems.push(format!("line {}: offset 0x{:X} is set by more than one bp", p.line, p.offset));
            }
            seen.push(p.offset);
        }
    }

    let mut used = vec![false; PROBE_BREAKPOINTS.len()];
    for p in &parsed {
        for c in &p.commands {
            let head = c.split_whitespace().next().unwrap_or("");
            if !BP_COMMANDS.contains(&head) {
                problems.push(format!(
                    "line {}: bp command {c:?} is not one of {BP_COMMANDS:?} -- the probe must only read and log",
                    p.line
                ));
            }
        }
        let Some(idx) = PROBE_BREAKPOINTS.iter().position(|b| b.module_offset() == p.offset) else {
            problems.push(format!(
                "line {}: offset 0x{:X} ({}) is not in the address table -- a stale or invented address",
                p.line,
                p.offset,
                p.label.as_deref().unwrap_or("?")
            ));
            continue;
        };
        used[idx] = true;
        let bp = PROBE_BREAKPOINTS[idx];
        match &p.label {
            None => problems.push(format!(
                "line {}: {} (0x{:X}) is not followed by its `.printf \"ARMED {}\\n\"` line",
                p.line, bp.name, p.offset, bp.name
            )),
            Some(label) if label != bp.name => problems.push(format!(
                "line {}: offset 0x{:X} should be followed by ARMED {:?} but prints {label:?}",
                p.line, p.offset, bp.name
            )),
            Some(_) => {}
        }
        if !p.continues {
            problems.push(format!(
                "line {}: {} (0x{:X}) does not end in `gc`, so it would stop the game",
                p.line, bp.name, p.offset
            ));
        }
        let (lo, hi) = TEXT_RANGE;
        if bp.static_va < lo || bp.static_va >= hi {
            problems.push(format!(
                "{}: static VA 0x{:08X} is outside Napoleon.exe's .text ({:08X}..{:08X})",
                bp.name, bp.static_va, lo, hi
            ));
        }
    }
    for (idx, bp) in PROBE_BREAKPOINTS.iter().enumerate() {
        if !used[idx] {
            problems.push(format!(
                "{}: 0x{:08X} ({}) is in the address table but no bp uses it -- \
                 the table and the script have drifted apart",
                bp.name,
                bp.static_va,
                bp.module_offset_hex()
            ));
        }
    }
    problems
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The script the checker exists for, read from the repo.
    const SCRIPT: &str = include_str!("../../../analysis/fidelity/debugger/0g_promotion_probe.cdb.txt");

    #[test]
    fn the_promotion_probe_matches_its_address_table() {
        let problems = check_probe_script(SCRIPT);
        assert!(problems.is_empty(), "probe script problems:\n{}", problems.join("\n"));
        assert_eq!(parse_probe_script(SCRIPT).len(), PROBE_BREAKPOINTS.len());
    }

    #[test]
    fn every_breakpoint_has_a_unique_name_and_offset() {
        let mut names: Vec<&str> = PROBE_BREAKPOINTS.iter().map(|b| b.name).collect();
        names.sort_unstable();
        let before = names.len();
        names.dedup();
        assert_eq!(before, names.len(), "two breakpoints share an ARMED label");

        let mut offs: Vec<u32> = PROBE_BREAKPOINTS.iter().map(|b| b.module_offset()).collect();
        offs.sort_unstable();
        let before = offs.len();
        offs.dedup();
        assert_eq!(before, offs.len(), "two breakpoints share an address");
    }

    #[test]
    fn the_module_offset_is_the_static_va_minus_the_image_base() {
        // The mistake this guards: writing the static VA into the module offset, or
        // subtracting the base twice. The land price is the one that matters.
        let price = PROBE_BREAKPOINTS.iter().find(|b| b.name == "price-land").unwrap();
        assert_eq!(price.static_va, 0x008E_279C);
        assert_eq!(price.module_offset(), 0x004E_279C);
        assert!(SCRIPT.contains("Napoleon+0x4E279C"), "script lost the price breakpoint");
    }

    /// The exact bug that cost a round: the price return and the -1 return are four bytes
    /// apart, so a probe on the wrong one never fires.
    #[test]
    fn the_price_return_and_the_minus_one_return_are_both_instrumented() {
        let price = *PROBE_BREAKPOINTS.iter().find(|b| b.name == "price-land").unwrap();
        let none = *PROBE_BREAKPOINTS.iter().find(|b| b.name == "cost44-norecord").unwrap();
        assert_eq!(none.static_va, price.static_va + 4);
        assert!(SCRIPT.contains("Napoleon+0x4E27A0"), "the -1 return must stay instrumented");
        // Every return pair is in the table, with RET at both ends.
        for &(va, _) in RETURN_PAIRS {
            for at in [va, va + 4] {
                let bp = PROBE_BREAKPOINTS.iter().find(|b| b.static_va == at).expect("a return in the table");
                assert_eq!(bp.first_opcode, Some(0xC3), "{}", bp.name);
            }
        }
    }

    #[test]
    fn a_stale_address_is_reported() {
        let broken = SCRIPT.replace("Napoleon+0x4E279C", "Napoleon+0x4E27A1");
        let problems = check_probe_script(&broken);
        assert!(
            problems.iter().any(|p| p.contains("0x4E27A1") && p.contains("price-land")),
            "a wrong offset was not reported; problems were {problems:#?}"
        );
        assert!(
            problems.iter().any(|p| p.contains("price-land") && p.contains("drifted")),
            "the now-unused table entry was not reported"
        );
    }

    #[test]
    fn a_missing_breakpoint_is_reported() {
        let broken: String =
            SCRIPT.lines().filter(|l| !l.contains("Napoleon+0x7AF500")).map(|l| format!("{l}\n")).collect();
        let problems = check_probe_script(&broken);
        assert!(
            problems.iter().any(|p| p.contains("treasury") && p.contains("drifted")),
            "a dropped breakpoint was not reported; problems were {problems:#?}"
        );
    }

    #[test]
    fn a_breakpoint_that_would_stop_the_game_is_reported() {
        let broken = SCRIPT.replace("; gc\"", "\"");
        let problems = check_probe_script(&broken);
        let stops = problems.iter().filter(|p| p.contains("does not end in `gc`")).count();
        assert_eq!(stops, PROBE_BREAKPOINTS.len(), "{problems:#?}");
    }

    /// The ARMED line is printed when the bp is SET (it is a top-level command after the `bp`),
    /// not when it is hit. One missing after its bp is reported.
    #[test]
    fn a_breakpoint_without_its_armed_line_is_reported() {
        // By line, not by "...\n": a Windows checkout has CRLF line ends.
        let broken: String =
            SCRIPT.lines().filter(|l| l.trim() != ".printf \"ARMED treasury\\n\"").map(|l| format!("{l}\n")).collect();
        assert_eq!(broken.lines().count() + 1, SCRIPT.lines().count(), "the test did not remove exactly one line");
        let problems = check_probe_script(&broken);
        assert!(
            problems.iter().any(|p| p.contains("treasury") && p.contains("not followed by")),
            "{problems:#?}"
        );
    }

    #[test]
    fn commented_out_breakpoints_do_not_count_as_set() {
        // `*` comments the whole line in cdb, semicolons included.
        let with_dead = format!("{SCRIPT}* bp Napoleon+0x4E27A0 \"gc\"\n");
        let problems = check_probe_script(&with_dead);
        assert!(problems.is_empty(), "a commented-out bp changed the verdict: {problems:#?}");
    }

    /// `$$` is a comment only up to the next `;` in cdb, and `#` is a command; the first
    /// version of this script used both for prose, so cdb would have run the prose.
    #[test]
    fn dollar_and_hash_comments_are_refused() {
        for bad in ["$$ note ; bp Napoleon+0x4E27A0 \"gc\"", "## a note that cdb would run as a search"] {
            let problems = check_probe_script(&format!("{SCRIPT}{bad}\n"));
            assert!(problems.iter().any(|p| p.contains("use `*`")), "{bad:?} was accepted: {problems:#?}");
        }
    }

    /// The probe must never write to the game: a memory edit at the top level or inside a bp's
    /// command string is refused.
    #[test]
    fn a_memory_or_register_write_is_refused() {
        for bad in ["ed Napoleon+0x4E279C 0", "eb 0x008E279C c3", ".writemem out.bin 0 L10", "r eax=0"] {
            let problems = check_probe_script(&format!("{SCRIPT}{bad}\n"));
            assert!(problems.iter().any(|p| p.contains("read-only")), "{bad:?} was accepted: {problems:#?}");
        }
        let edited = SCRIPT.replace(
            "\".printf \\\"PRICE land=%d rec=%x\\n\\\", @eax, @ecx; gc\"",
            "\".printf \\\"PRICE land=%d rec=%x\\n\\\", @eax, @ecx; r eax=0; gc\"",
        );
        assert_ne!(edited, SCRIPT, "the test did not edit the price bp");
        let problems = check_probe_script(&edited);
        assert!(problems.iter().any(|p| p.contains("r eax=0") && p.contains("only read")), "{problems:#?}");
    }

    #[test]
    fn a_semicolon_inside_a_printed_string_does_not_split_the_command() {
        assert_eq!(split_commands(r#".printf \"a ; b\n\"; gc"#), vec![r#".printf \"a ; b\n\""#.to_string(), "gc".into()]);
    }

    #[test]
    fn two_breakpoints_on_one_address_are_reported() {
        let dup = format!("{SCRIPT}\nbp Napoleon+0x4E279C \".printf \\\"X\\n\\\"; gc\"\n.printf \"ARMED price-land\\n\"\n");
        let problems = check_probe_script(&dup);
        assert!(
            problems.iter().any(|p| p.contains("more than one bp") && p.contains("0x4E279C")),
            "a duplicated address was not reported; problems were {problems:#?}"
        );
    }

    #[test]
    fn the_return_pair_bytes_put_ret_four_bytes_apart() {
        let b = return_pair_bytes(0x38);
        assert_eq!((b[3], b[7]), (0xC3, 0xC3));
        assert_eq!(&b[..3], &[0x8B, 0x40, 0x38]);
    }
}
