//! Research helper: reads a Windows minidump of the original game's crash (read-only) and prints
//! the faulting thread's registers and the likely call chain: every stack dword that points just
//! after a CALL instruction in Napoleon.exe's code (checked against the exe's bytes).
//!   dump_stack DUMP [max]
use std::collections::BTreeMap;

const EXE: &str = r"C:\Program Files (x86)\Steam\steamapps\common\Napoleon Total War\Napoleon.exe";

fn u32_at(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes(b[o..o + 4].try_into().unwrap())
}
fn u64_at(b: &[u8], o: usize) -> u64 {
    u64::from_le_bytes(b[o..o + 8].try_into().unwrap())
}

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let max: usize = a.get(1).and_then(|s| s.parse().ok()).unwrap_or(40);
    let d = std::fs::read(&a[0]).unwrap();
    assert_eq!(&d[0..4], b"MDMP");
    let n = u32_at(&d, 8) as usize;
    let dir = u32_at(&d, 12) as usize;
    let mut streams: BTreeMap<u32, (usize, usize)> = BTreeMap::new();
    for i in 0..n {
        let o = dir + i * 12;
        streams.insert(u32_at(&d, o), (u32_at(&d, o + 8) as usize, u32_at(&d, o + 4) as usize));
    }
    // Module list (4): the exe's base.
    let mut base = 0x400000u64;
    if let Some(&(rva, _)) = streams.get(&4) {
        let count = u32_at(&d, rva) as usize;
        for i in 0..count {
            let m = rva + 4 + i * 108;
            let name_rva = u32_at(&d, m + 20) as usize;
            let len = u32_at(&d, name_rva) as usize;
            let name: String = String::from_utf16_lossy(&d[name_rva + 4..name_rva + 4 + len].chunks(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect::<Vec<_>>());
            if name.to_lowercase().ends_with("napoleon.exe") {
                base = u64_at(&d, m);
                println!("module {name} base {base:#x} size {:#x}", u32_at(&d, m + 8));
            }
        }
    }
    // Exception stream (6): thread id, code, address, context.
    let (erva, _) = streams[&6];
    let tid = u32_at(&d, erva);
    let code = u32_at(&d, erva + 8);
    let addr = u64_at(&d, erva + 8 + 16);
    let ctx_rva = u32_at(&d, erva + 8 + 152 + 4) as usize;
    let r = |off: usize| u32_at(&d, ctx_rva + off);
    let (eip, esp, ebp) = (r(0xB8), r(0xC4), r(0xB4));
    println!("thread {tid} code {code:#x} address {addr:#x} (rel {:#x})", addr - base);
    println!("eax {:#x} ebx {:#x} ecx {:#x} edx {:#x} esi {:#x} edi {:#x} ebp {ebp:#x} esp {esp:#x} eip {eip:#x}", r(0xB0), r(0xA4), r(0xAC), r(0xA8), r(0xA0), r(0x9C));
    // Thread list (3): the thread's stack memory.
    let (trva, _) = streams[&3];
    let tcount = u32_at(&d, trva) as usize;
    let mut stack: Option<(u64, &[u8])> = None;
    for i in 0..tcount {
        let t = trva + 4 + i * 48;
        if u32_at(&d, t) == tid {
            let start = u64_at(&d, t + 24);
            let size = u32_at(&d, t + 32) as usize;
            let rva = u32_at(&d, t + 36) as usize;
            stack = Some((start, &d[rva..rva + size]));
        }
    }
    let (start, mem) = stack.expect("thread stack");
    if std::env::var("PEEK").is_ok() {
        let r = ranges(&d);
        println!("{} memory ranges, {} bytes", r.len(), r.iter().map(|x| x.2).sum::<usize>());
        for k in 0..40u64 {
            let va = u64::from(esp) + k * 4;
            println!("  [esp+{:#04x}] {:?}", k * 4, read32(&d, &r, va).map(|v| format!("{v:#010x}")));
        }
        if let Ok(list) = std::env::var("PEEK") {
            for p in list.split(',').filter(|s| !s.is_empty()) {
                let va = u64::from_str_radix(p.trim_start_matches("0x"), 16).unwrap();
                let words: Vec<String> = (0..16u64).map(|k| read32(&d, &r, va + k * 4).map_or("?".into(), |v| format!("{v:08x}"))).collect();
                println!("  {va:#x}: {}", words.join(" "));
            }
        }
    }
    find(&d, &ranges(&d));
    let exe = std::fs::read(EXE).unwrap();
    // .text: VA 0x1000, raw 0x400, size 0xf05a00 (SAVE_COMPAT notes; section table checked).
    let file_of = |va: u64| -> Option<usize> {
        let rel = va.checked_sub(base)?;
        (0x1000..0x1000 + 0xf05a00).contains(&rel).then(|| (rel - 0x1000 + 0x400) as usize)
    };
    let after_call = |ret: u64| -> bool {
        let Some(f) = file_of(ret) else { return false };
        if f < 7 {
            return false;
        }
        exe[f - 5] == 0xE8 || exe[f - 2] == 0xFF && (exe[f - 1] & 0x38) == 0x10 || exe[f - 3] == 0xFF && (exe[f - 2] & 0x38) == 0x10 || exe[f - 6] == 0xFF && (exe[f - 5] & 0x38) == 0x10
    };
    let mut shown = 0;
    let first = (u64::from(esp) - start) as usize;
    for off in (first..mem.len().saturating_sub(4)).step_by(4) {
        let v = u64::from(u32_at(mem, off));
        if after_call(v) {
            let f = file_of(v).unwrap();
            let target = if exe[f - 5] == 0xE8 { Some((v as i64 + i64::from(i32::from_le_bytes(exe[f - 4..f].try_into().unwrap()))) as u64) } else { None };
            println!("  [esp+{:#06x}] ret {v:#010x} (call to {})", off - first, target.map_or("indirect".to_string(), |t| format!("{t:#010x}")));
            shown += 1;
            if shown >= max {
                break;
            }
        }
    }
}

/// The dump's memory ranges (MemoryListStream 5 and Memory64ListStream 9): (start VA, file offset, size).
pub fn ranges(d: &[u8]) -> Vec<(u64, usize, usize)> {
    let n = u32_at(d, 8) as usize;
    let dir = u32_at(d, 12) as usize;
    let mut out = Vec::new();
    for i in 0..n {
        let o = dir + i * 12;
        let (ty, rva) = (u32_at(d, o), u32_at(d, o + 8) as usize);
        if ty == 5 {
            let count = u32_at(d, rva) as usize;
            for k in 0..count {
                let m = rva + 4 + k * 16;
                out.push((u64_at(d, m), u32_at(d, m + 12) as usize, u32_at(d, m + 8) as usize));
            }
        } else if ty == 9 {
            let count = u64_at(d, rva) as usize;
            let mut off = u64_at(d, rva + 8) as usize;
            for k in 0..count {
                let m = rva + 16 + k * 16;
                let size = u64_at(d, m + 8) as usize;
                out.push((u64_at(d, m), off, size));
                off += size;
            }
        }
    }
    out
}

/// The dword at `va`, if the dump holds it.
pub fn read32(d: &[u8], r: &[(u64, usize, usize)], va: u64) -> Option<u32> {
    r.iter().find(|(s, _, n)| va >= *s && va + 4 <= s + *n as u64).map(|(s, o, _)| u32_at(d, o + (va - s) as usize))
}

/// `FIND=hex[,hex..]`: every address in the dump holding that dword (e.g. a vtable pointer, to
/// find objects of a class); with `FIELDS=off,off,..` the dwords at those offsets of each hit.
pub fn find(d: &[u8], r: &[(u64, usize, usize)]) {
    let Ok(list) = std::env::var("FIND") else { return };
    let fields: Vec<u64> = std::env::var("FIELDS").ok().map(|s| s.split(',').filter_map(|x| u64::from_str_radix(x.trim_start_matches("0x"), 16).ok()).collect()).unwrap_or_default();
    for v in list.split(',').filter(|s| !s.is_empty()) {
        let want = u32::from_str_radix(v.trim_start_matches("0x"), 16).unwrap();
        let mut hits = 0;
        for &(start, off, size) in r {
            let mem = &d[off..off + size];
            for k in (0..mem.len().saturating_sub(3)).step_by(4) {
                if u32_at(mem, k) == want {
                    let va = start + k as u64;
                    let fv: Vec<String> = fields.iter().map(|&f| read32(d, r, va + f).map_or("?".into(), |x| format!("{f:#x}={x:#x}"))).collect();
                    println!("  {want:#x} at {va:#x} {}", fv.join(" "));
                    hits += 1;
                    if hits > 40 {
                        return;
                    }
                }
            }
        }
        println!("  {want:#x}: {hits} hits");
    }
}
