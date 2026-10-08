//! The 0-G promotion probe, checked against the shipped `Napoleon.exe` (read-only).
//!
//! [`ntw_data::debugger`] already checks the probe *script* against the address table
//! without needing the install (see the unit tests in that module). This test adds the
//! other half: it reads the module's bytes and confirms that each breakpoint still lands
//! on the instruction it is documented to land on. That is what catches a **stale
//! address** — a game patch moving a function, or an address transcribed wrong — which is
//! otherwise invisible: `cdb` sets the breakpoint happily and the log just comes back
//! missing a line.
//!
//! ```text
//! cargo test -p ntw_data --test probe_install -- --ignored --nocapture
//! ```

use ntw_data::debugger::{IMAGE_BASE, PROBE_BREAKPOINTS, RETURN_PAIRS, TEXT_RANGE, return_pair_bytes};
use std::path::PathBuf;

fn exe() -> Option<PathBuf> {
    let dir = std::env::var("NTW_INSTALL_DIR").unwrap_or_else(|_| {
        r"C:\Program Files (x86)\Steam\steamapps\common\Napoleon Total War".to_string()
    });
    let p = PathBuf::from(dir).join("Napoleon.exe");
    p.is_file().then_some(p)
}

/// A read-only view of the module's PE image, so an RVA can be turned into a file offset.
///
/// This has to go through the section table: `.text` has RVA `0x1000` but its
/// `PointerToRawData` is not `0x1000`, and later sections are further off still, so
/// treating an RVA as a file offset reads the wrong bytes and reports a perfectly good
/// address as stale.
struct Pe<'a> {
    bytes: &'a [u8],
    sections: Vec<Section>,
    image_base: u32,
}

/// One section header: its name as stored (8 bytes, NUL-trimmed), RVA, virtual size, file
/// offset and size on file.
struct Section {
    name: String,
    rva: u32,
    vsize: u32,
    raw: u32,
    raw_size: u32,
}

impl<'a> Pe<'a> {
    fn parse(bytes: &'a [u8]) -> Self {
        assert_eq!(&bytes[0..2], b"MZ", "not a PE image");
        let pe = u32::from_le_bytes(bytes[0x3C..0x40].try_into().unwrap()) as usize;
        assert_eq!(&bytes[pe..pe + 4], b"PE\0\0", "bad PE signature");
        let coff = pe + 4;
        let n_sections = u16::from_le_bytes(bytes[coff + 2..coff + 4].try_into().unwrap()) as usize;
        let opt_size = u16::from_le_bytes(bytes[coff + 16..coff + 18].try_into().unwrap()) as usize;
        let opt = coff + 20;
        // ImageBase sits at a different offset in PE32 (0x1C) than PE32+ (0x18). Napoleon
        // is 32-bit, but reading the wrong one silently yields 0, which would then make
        // every RVA nonsense.
        let magic = u16::from_le_bytes(bytes[opt..opt + 2].try_into().unwrap());
        let image_base_at = match magic {
            0x010B => opt + 0x1C, // PE32
            0x020B => opt + 0x18, // PE32+
            other => panic!("unknown optional header magic 0x{other:04X}"),
        };
        let image_base = u32::from_le_bytes(bytes[image_base_at..image_base_at + 4].try_into().unwrap());
        let table = opt + opt_size;
        let u32_at = |at: usize| u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap());
        let sections = (0..n_sections)
            .map(|i| {
                let h = table + i * 40;
                let name = String::from_utf8_lossy(&bytes[h..h + 8]).trim_end_matches('\0').to_string();
                Section { name, vsize: u32_at(h + 8), rva: u32_at(h + 12), raw_size: u32_at(h + 16), raw: u32_at(h + 20) }
            })
            .collect();
        Self { bytes, sections, image_base }
    }

    fn section_at(&self, static_va: u32) -> Option<&Section> {
        let rva = static_va.checked_sub(self.image_base)?;
        self.sections.iter().find(|s| rva >= s.rva && rva < s.rva + s.vsize.max(1))
    }

    /// The byte at a static virtual address, or `None` when nothing on file backs it (outside
    /// every section, or in a section's zero-filled tail past its raw size).
    fn byte_at_va(&self, static_va: u32) -> Option<u8> {
        let s = self.section_at(static_va)?;
        let off = static_va - self.image_base - s.rva;
        if off >= s.raw_size {
            return None;
        }
        self.bytes.get((s.raw + off) as usize).copied()
    }

    /// The name of the section holding a static VA, read from the header itself.
    fn section_of(&self, static_va: u32) -> &str {
        self.section_at(static_va).map_or("unmapped", |s| s.name.as_str())
    }
}

#[test]
#[ignore = "needs the player's install"]
fn every_probe_breakpoint_still_sits_on_its_instruction() {
    let Some(path) = exe() else {
        panic!(
            "Napoleon.exe not found; set NTW_INSTALL_DIR to the install folder \
             (the Steam default was tried)"
        );
    };
    let bytes = std::fs::read(&path).expect("read Napoleon.exe (read-only)");
    let pe = Pe::parse(&bytes);
    let (lo, hi) = TEXT_RANGE;
    println!("{} bytes from {}", bytes.len(), path.display());
    println!(
        "image base 0x{:08X}, {} sections: {:?}",
        pe.image_base,
        pe.sections.len(),
        pe.sections
            .iter()
            .map(|s| format!("{} rva=0x{:X} size=0x{:X} raw=0x{:X}", s.name, s.rva, s.vsize, s.raw))
            .collect::<Vec<_>>()
    );
    let mut checked = 0usize;
    let mut without_opcode = Vec::new();
    for bp in PROBE_BREAKPOINTS {
        assert!(
            (lo..hi).contains(&bp.static_va),
            "{}: 0x{:08X} is outside .text",
            bp.name,
            bp.static_va
        );
        assert_eq!(
            pe.section_of(bp.static_va),
            ".text",
            "{}: 0x{:08X} is not in .text",
            bp.name,
            bp.static_va
        );
        let Some(want) = bp.first_opcode else {
            without_opcode.push(bp.name);
            continue;
        };
        let got = pe.byte_at_va(bp.static_va).unwrap_or_else(|| {
            panic!("{}: nothing mapped at 0x{:08X}", bp.name, bp.static_va)
        });
        assert_eq!(
            got, want,
            "{}: byte at 0x{:08X} is 0x{got:02X}, expected 0x{want:02X} -- STALE, \
             re-read the decompile before trusting the probe",
            bp.name, bp.static_va
        );
        checked += 1;
    }
    println!("{checked} of {} breakpoints opcode-checked", PROBE_BREAKPOINTS.len());
    if !without_opcode.is_empty() {
        println!("no opcode recorded (address-in-.text check only): {without_opcode:?}");
    }
}

/// One byte per breakpoint is a weak check for the six `RET`s (`0xC3` is common). The three
/// value / `-1` return pairs are pinned by eight bytes each, derived from the recorded
/// mnemonics ([`return_pair_bytes`]): `MOV EAX,[EAX+field]; RET; OR EAX,-1; RET`. A failure
/// here means the recorded disassembly was wrong (or the exe differs), so re-read it before
/// trusting the PRICE line.
#[test]
#[ignore = "needs the player's install"]
fn the_cost_slots_value_and_minus_one_returns_match_the_recorded_instructions() {
    let Some(path) = exe() else {
        panic!("Napoleon.exe not found; set NTW_INSTALL_DIR to the install folder");
    };
    let bytes = std::fs::read(&path).expect("read Napoleon.exe (read-only)");
    let pe = Pe::parse(&bytes);
    for &(va, field) in RETURN_PAIRS {
        let got: Vec<Option<u8>> = (va - 3..=va + 4).map(|a| pe.byte_at_va(a)).collect();
        let want: Vec<Option<u8>> = return_pair_bytes(field).iter().map(|b| Some(*b)).collect();
        assert_eq!(got, want, "0x{:08X}..: the value / -1 return pair is not where the notes put it", va - 3);
    }
}

#[test]
#[ignore = "needs the player's install"]
fn the_module_offset_is_recomputable_from_the_file() {
    // `IMAGE_BASE` is what makes `Napoleon+0x...` work under ASLR, so it must be the
    // module's real preferred base.
    let Some(path) = exe() else {
        panic!("Napoleon.exe not found; set NTW_INSTALL_DIR to the install folder");
    };
    let bytes = std::fs::read(&path).expect("read Napoleon.exe (read-only)");
    let image_base = Pe::parse(&bytes).image_base;
    assert_eq!(
        image_base as u64, IMAGE_BASE,
        "the exe's preferred image base moved; every probe offset is image_base - relative \
         and must be re-derived"
    );
}