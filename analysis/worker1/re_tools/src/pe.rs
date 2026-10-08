//! Minimal read-only PE32/PE32+ parser (std only).
use std::fmt::Write as _;

#[derive(Clone, Debug)]
pub struct Section {
    pub name: String,
    pub vsize: u32,
    pub va: u32,
    pub rsize: u32,
    pub raw: u32,
    pub ch: u32,
}

pub struct Pe {
    pub d: Vec<u8>,
    pub e_lfanew: usize,
    pub machine: u16,
    pub timestamp: u32,
    pub characteristics: u16,
    pub is64: bool,
    pub linker: (u8, u8),
    pub entry: u32,
    pub image_base: u64,
    pub size_image: u32,
    pub size_headers: u32,
    pub checksum: u32,
    pub subsystem: u16,
    pub dll_chars: u16,
    pub os_ver: (u16, u16),
    pub subsys_ver: (u16, u16),
    pub dirs: Vec<(u32, u32)>,
    pub sections: Vec<Section>,
}

pub const DIRNAMES: [&str; 16] = [
    "EXPORT", "IMPORT", "RESOURCE", "EXCEPTION", "SECURITY", "BASERELOC", "DEBUG", "ARCHITECTURE",
    "GLOBALPTR", "TLS", "LOAD_CONFIG", "BOUND_IMPORT", "IAT", "DELAY_IMPORT", "COM_DESCRIPTOR", "RESERVED",
];

impl Pe {
    pub fn u16at(&self, o: usize) -> u16 { u16::from_le_bytes([self.d[o], self.d[o + 1]]) }
    pub fn u32at(&self, o: usize) -> u32 { u32::from_le_bytes(self.d[o..o + 4].try_into().unwrap()) }
    pub fn u64at(&self, o: usize) -> u64 { u64::from_le_bytes(self.d[o..o + 8].try_into().unwrap()) }

    pub fn load(path: &str) -> std::io::Result<Pe> {
        let d = std::fs::read(path)?; // read-only
        Ok(Pe::parse(d))
    }

    pub fn parse(d: Vec<u8>) -> Pe {
        assert!(&d[..2] == b"MZ", "not MZ");
        let mut p = Pe {
            d, e_lfanew: 0, machine: 0, timestamp: 0, characteristics: 0, is64: false, linker: (0, 0),
            entry: 0, image_base: 0, size_image: 0, size_headers: 0, checksum: 0, subsystem: 0,
            dll_chars: 0, os_ver: (0, 0), subsys_ver: (0, 0), dirs: vec![], sections: vec![],
        };
        p.e_lfanew = p.u32at(0x3c) as usize;
        assert!(&p.d[p.e_lfanew..p.e_lfanew + 4] == b"PE\0\0", "not PE");
        let o = p.e_lfanew + 4;
        p.machine = p.u16at(o);
        let nsec = p.u16at(o + 2) as usize;
        p.timestamp = p.u32at(o + 4);
        let opt_size = p.u16at(o + 16) as usize;
        p.characteristics = p.u16at(o + 18);
        let opt = o + 20;
        p.is64 = p.u16at(opt) == 0x20b;
        p.linker = (p.d[opt + 2], p.d[opt + 3]);
        p.entry = p.u32at(opt + 16);
        p.image_base = if p.is64 { p.u64at(opt + 24) } else { p.u32at(opt + 28) as u64 };
        let base = opt + 32;
        p.os_ver = (p.u16at(base + 8), p.u16at(base + 10));
        p.subsys_ver = (p.u16at(base + 16), p.u16at(base + 18));
        p.size_image = p.u32at(base + 24);
        p.size_headers = p.u32at(base + 28);
        p.checksum = p.u32at(base + 32);
        p.subsystem = p.u16at(base + 36);
        p.dll_chars = p.u16at(base + 38);
        let dd = base + 40 + if p.is64 { 32 } else { 16 } + 8;
        let ndirs = p.u32at(dd - 4).min(16) as usize;
        for i in 0..ndirs {
            p.dirs.push((p.u32at(dd + i * 8), p.u32at(dd + i * 8 + 4)));
        }
        while p.dirs.len() < 16 { p.dirs.push((0, 0)); }
        let mut so = opt + opt_size;
        for _ in 0..nsec {
            let nm = &p.d[so..so + 8];
            let end = nm.iter().position(|&c| c == 0).unwrap_or(8);
            let name = String::from_utf8_lossy(&nm[..end]).to_string();
            let s = Section {
                name, vsize: p.u32at(so + 8), va: p.u32at(so + 12), rsize: p.u32at(so + 16),
                raw: p.u32at(so + 20), ch: p.u32at(so + 36),
            };
            p.sections.push(s);
            so += 40;
        }
        p
    }

    pub fn rva2off(&self, rva: u32) -> Option<usize> {
        for s in &self.sections {
            if rva >= s.va && rva < s.va + s.vsize.max(s.rsize) {
                let o = rva - s.va;
                if o >= s.rsize { return None; }
                return Some((s.raw + o) as usize);
            }
        }
        if rva < self.size_headers { Some(rva as usize) } else { None }
    }
    pub fn off2rva(&self, off: usize) -> Option<u32> {
        for s in &self.sections {
            if off >= s.raw as usize && off < (s.raw + s.rsize) as usize {
                return Some(off as u32 - s.raw + s.va);
            }
        }
        None
    }
    pub fn va2off(&self, va: u32) -> Option<usize> {
        if (va as u64) < self.image_base { return None; }
        self.rva2off((va as u64 - self.image_base) as u32)
    }
    pub fn off2va(&self, off: usize) -> Option<u32> {
        self.off2rva(off).map(|r| (r as u64 + self.image_base) as u32)
    }
    pub fn section(&self, name: &str) -> Option<&Section> { self.sections.iter().find(|s| s.name == name) }
    pub fn section_of_off(&self, off: usize) -> &str {
        for s in &self.sections {
            if off >= s.raw as usize && off < (s.raw + s.rsize) as usize { return &s.name; }
        }
        "hdr"
    }
    pub fn section_of_va(&self, va: u32) -> Option<&Section> {
        let rva = (va as u64).checked_sub(self.image_base)? as u32;
        self.sections.iter().find(|s| rva >= s.va && rva < s.va + s.vsize.max(s.rsize))
    }
    pub fn is_code_va(&self, va: u32) -> bool {
        self.section_of_va(va).map(|s| s.ch & 0x20000000 != 0).unwrap_or(false)
    }
    pub fn cstr(&self, off: usize, max: usize) -> String {
        let end = (off + max).min(self.d.len());
        let e = self.d[off..end].iter().position(|&c| c == 0).map(|x| off + x).unwrap_or(end);
        String::from_utf8_lossy(&self.d[off..e]).to_string()
    }
    pub fn cstr_rva(&self, rva: u32) -> String {
        self.rva2off(rva).map(|o| self.cstr(o, 4096)).unwrap_or_else(|| "?".into())
    }

    pub fn imports(&self) -> Vec<(String, Vec<(String, u32)>)> {
        let mut res: Vec<(String, Vec<(String, u32)>)> = vec![];
        let (rva, _) = self.dirs[1];
        if rva == 0 { return res; }
        let Some(mut o) = self.rva2off(rva) else { return res };
        let psz = if self.is64 { 8 } else { 4 };
        loop {
            let oft = self.u32at(o);
            let name = self.u32at(o + 12);
            let ft = self.u32at(o + 16);
            if oft == 0 && name == 0 && ft == 0 { break; }
            let dll = self.cstr_rva(name);
            let mut funcs = vec![];
            let mut t = self.rva2off(if oft != 0 { oft } else { ft });
            let mut idx = 0u32;
            while let Some(to) = t {
                let v = if self.is64 { self.u64at(to) } else { self.u32at(to) as u64 };
                if v == 0 { break; }
                let iat = ft + idx * psz as u32;
                let ordflag = if self.is64 { 1u64 << 63 } else { 1u64 << 31 };
                if v & ordflag != 0 {
                    funcs.push((format!("#{}", v & 0xffff), iat));
                } else {
                    let n = self.rva2off((v & 0x7fffffff) as u32).map(|x| self.cstr(x + 2, 512)).unwrap_or("?".into());
                    funcs.push((n, iat));
                }
                t = Some(to + psz);
                idx += 1;
            }
            res.push((dll, funcs));
            o += 20;
        }
        res
    }

    pub fn delay_imports(&self) -> Vec<(String, Vec<String>)> {
        let mut res = vec![];
        let (rva, _) = self.dirs[13];
        if rva == 0 { return res; }
        let Some(mut o) = self.rva2off(rva) else { return res };
        loop {
            let attrs = self.u32at(o);
            let name = self.u32at(o + 4);
            let int_ = self.u32at(o + 16);
            if name == 0 { break; }
            let fix = |r: u32| if attrs & 1 != 0 { r } else { r.wrapping_sub(self.image_base as u32) };
            let dll = self.cstr_rva(fix(name));
            let mut f = vec![];
            let mut t = self.rva2off(fix(int_));
            while let Some(to) = t {
                let v = self.u32at(to);
                if v == 0 { break; }
                if v & 0x80000000 != 0 { f.push(format!("#{}", v & 0xffff)); } else { f.push(self.cstr_rva(fix(v) + 2)); }
                t = Some(to + 4);
            }
            res.push((dll, f));
            o += 32;
        }
        res
    }

    pub fn exports(&self) -> (String, Vec<(String, u32, u32)>) {
        let (rva, _) = self.dirs[0];
        let mut out = vec![];
        if rva == 0 { return (String::new(), out); }
        let Some(o) = self.rva2off(rva) else { return (String::new(), out) };
        let name = self.cstr_rva(self.u32at(o + 12));
        let base = self.u32at(o + 16);
        let nnames = self.u32at(o + 24) as usize;
        let (af, an, ao) = (self.u32at(o + 28), self.u32at(o + 32), self.u32at(o + 36));
        for i in 0..nnames {
            let nr = self.u32at(self.rva2off(an).unwrap() + 4 * i);
            let ord = self.u16at(self.rva2off(ao).unwrap() + 2 * i) as u32;
            let frva = self.u32at(self.rva2off(af).unwrap() + 4 * ord as usize);
            out.push((self.cstr_rva(nr), ord + base, frva));
        }
        (name, out)
    }

    pub fn debug(&self) -> Vec<String> {
        let (rva, size) = self.dirs[6];
        let mut out = vec![];
        if rva == 0 { return out; }
        let Some(o) = self.rva2off(rva) else { return out };
        for i in 0..(size / 28) as usize {
            let e = o + i * 28;
            let typ = self.u32at(e + 12);
            let sz = self.u32at(e + 16);
            let ptr = self.u32at(e + 24) as usize;
            let mut s = format!("type={} ({}) size={}", typ, match typ { 2 => "CODEVIEW", 12 => "VC_FEATURE", 13 => "POGO", 14 => "ILTCG", 16 => "REPRO", _ => "?" }, sz);
            if typ == 2 && ptr != 0 && &self.d[ptr..ptr + 4] == b"RSDS" {
                let g = &self.d[ptr + 4..ptr + 20];
                let a = u32::from_le_bytes(g[0..4].try_into().unwrap());
                let b = u16::from_le_bytes(g[4..6].try_into().unwrap());
                let c = u16::from_le_bytes(g[6..8].try_into().unwrap());
                let hex: String = g[8..].iter().map(|x| format!("{:02X}", x)).collect();
                let _ = write!(s, " RSDS guid={:08X}-{:04X}-{:04X}-{}-{} age={} pdb={}", a, b, c, &hex[..4], &hex[4..], self.u32at(ptr + 20), self.cstr(ptr + 24, 512));
            }
            if typ == 13 && ptr != 0 {
                // POGO: 'PGU '/'PGI ' signature then entries (rva, size, name)
                let sig = &self.d[ptr..ptr + 4];
                let _ = write!(s, " sig={:?}", String::from_utf8_lossy(sig));
                let mut q = ptr + 4;
                let end = ptr + sz as usize;
                let mut names = vec![];
                while q + 8 < end {
                    let (r, l) = (self.u32at(q), self.u32at(q + 4));
                    let n = self.cstr(q + 8, 64);
                    names.push(format!("{}@0x{:x}+0x{:x}", n, r, l));
                    q += 8 + ((n.len() + 1 + 3) & !3);
                }
                let _ = write!(s, " sections: {}", names.join(", "));
            }
            out.push(s);
        }
        out
    }

    /// Rich header entries (prodid, build, count)
    pub fn rich(&self) -> Vec<(u16, u16, u32)> {
        let hdr = &self.d[..self.e_lfanew];
        let Some(end) = hdr.windows(4).position(|w| w == b"Rich") else { return vec![] };
        let key = self.u32at(end + 4);
        let mut i = end - 4;
        while i >= 0x80 {
            if self.u32at(i) ^ key == 0x536e6144 { break; }
            i -= 4;
        }
        let mut v = vec![];
        let mut j = i + 16;
        while j < end {
            let comp = self.u32at(j) ^ key;
            let cnt = self.u32at(j + 4) ^ key;
            v.push(((comp >> 16) as u16, (comp & 0xffff) as u16, cnt));
            j += 8;
        }
        v
    }
}

pub fn entropy(b: &[u8]) -> f64 {
    if b.is_empty() { return 0.0; }
    let mut c = [0usize; 256];
    for &x in b { c[x as usize] += 1; }
    let n = b.len() as f64;
    c.iter().filter(|&&v| v > 0).map(|&v| { let p = v as f64 / n; -p * p.log2() }).sum()
}

pub fn rich_name(pid: u16) -> &'static str {
    match pid {
        0x0001 => "Import0 (old-style import entries)",
        0x005a => "Linker710 (VS2003)", 0x005d => "Utc1310_C (VS2003)",
        0x0078 => "Linker800 (VS2005)", 0x007a => "Utc1400? (VS2005)", 0x007b => "Implib800? (VS2005)",
        0x007c => "Cvtres800", 0x007d => "Export800",
        0x0091 => "Linker900 (VS2008)", 0x0093 => "Implib900 (VS2008)", 0x0094 => "Cvtres900",
        0x00c7 => "Masm1000? (VS2010-era id)",
        0x00dd => "Implib1200 (VS2013)", 0x00de => "Linker1200 (VS2013)",
        0x00ff => "Cvtres1400 (VS2015+)", 0x0100 => "Export1400 (VS2015+)", 0x0101 => "Implib1400 (VS2015+)",
        0x0102 => "Linker1400 (VS2015+)", 0x0103 => "Masm1400 (VS2015+)", 0x0104 => "Utc1900_C (VS2015+)",
        0x0105 => "Utc1900_CPP (VS2015+)", 0x0106 => "Utc1900_CVTCIL_C (VS2015+)",
        _ => "unknown",
    }
}

fn sec_flags(ch: u32) -> String {
    let mut f = vec![];
    if ch & 0x20 != 0 { f.push("CODE") }
    if ch & 0x40 != 0 { f.push("IDATA") }
    if ch & 0x80 != 0 { f.push("UDATA") }
    if ch & 0x20000000 != 0 { f.push("X") }
    if ch & 0x40000000 != 0 { f.push("R") }
    if ch & 0x80000000 != 0 { f.push("W") }
    f.join("|")
}

fn fmt_time(ts: u32) -> String {
    // civil-from-days (Howard Hinnant), UTC
    let secs = ts as i64;
    let days = secs.div_euclid(86400);
    let sod = secs.rem_euclid(86400);
    let z = days + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{:04}-{:02}-{:02} {:02}:{:02}:{:02} UTC", y, m, d, sod / 3600, (sod / 60) % 60, sod % 60)
}

pub fn report(path: &str) -> String {
    let p = match Pe::load(path) { Ok(p) => p, Err(e) => return format!("ERROR {}: {}\n", path, e) };
    let mut w = String::new();
    let _ = writeln!(w, "{}\nFILE: {} ({} bytes)", "=".repeat(78), path, p.d.len());
    let _ = writeln!(w, "Machine: 0x{:x} {}", p.machine, match p.machine { 0x14c => "i386 (32-bit)", 0x8664 => "AMD64 (64-bit)", _ => "?" });
    let _ = writeln!(w, "TimeDateStamp: 0x{:08x} = {}", p.timestamp, fmt_time(p.timestamp));
    let _ = writeln!(w, "Characteristics: 0x{:04x} {} LargeAddressAware={}", p.characteristics, if p.characteristics & 0x2000 != 0 { "DLL" } else { "EXE" }, p.characteristics & 0x20 != 0);
    let _ = writeln!(w, "Format: {} Linker {}.{}  OS {}.{}  Subsystem {} ({}.{})", if p.is64 { "PE32+" } else { "PE32" }, p.linker.0, p.linker.1, p.os_ver.0, p.os_ver.1, match p.subsystem { 2 => "WINDOWS_GUI", 3 => "WINDOWS_CUI", _ => "?" }, p.subsys_ver.0, p.subsys_ver.1);
    let _ = writeln!(w, "ImageBase 0x{:x} EntryPoint RVA 0x{:x} (VA 0x{:x}) SizeOfImage 0x{:x} Checksum 0x{:x} DllChars 0x{:04x}{}{}{}", p.image_base, p.entry, p.image_base + p.entry as u64, p.size_image, p.checksum, p.dll_chars,
        if p.dll_chars & 0x40 != 0 { " ASLR" } else { "" }, if p.dll_chars & 0x100 != 0 { " NX" } else { "" }, if p.dll_chars & 0x400 != 0 { " NO_SEH" } else { "" });
    let _ = writeln!(w, "Data directories:");
    for (i, (r, s)) in p.dirs.iter().enumerate() {
        if *r != 0 || *s != 0 { let _ = writeln!(w, "  {:<14} rva=0x{:08x} size=0x{:x}", DIRNAMES[i], r, s); }
    }
    let _ = writeln!(w, "Sections:\n  {:<8} {:>10} {:>10} {:>10} {:>10} {:<16} entropy", "name", "VA", "VSize", "RawOff", "RawSize", "flags");
    for s in &p.sections {
        let e = entropy(&p.d[s.raw as usize..(s.raw + s.rsize) as usize]);
        let _ = writeln!(w, "  {:<8} 0x{:08x} 0x{:08x} 0x{:08x} 0x{:08x} {:<16} {:.3}{}", s.name, s.va, s.vsize, s.raw, s.rsize, sec_flags(s.ch), e, if e > 7.2 { "  <-- HIGH" } else { "" });
    }
    let eps = p.sections.iter().find(|s| p.entry >= s.va && p.entry < s.va + s.vsize).map(|s| s.name.clone()).unwrap_or("NONE".into());
    let _ = writeln!(w, "Entry point section: {}", eps);
    let ov = p.sections.iter().map(|s| (s.raw + s.rsize) as usize).max().unwrap_or(0);
    if ov < p.d.len() { let _ = writeln!(w, "Overlay: {} bytes at 0x{:x} (SECURITY dir {:?})", p.d.len() - ov, ov, p.dirs[4]); }
    let steamstub = p.sections.iter().any(|s| s.name == ".bind");
    let _ = writeln!(w, "SteamStub .bind section present: {}", steamstub);
    let rich = p.rich();
    if !rich.is_empty() {
        let _ = writeln!(w, "Rich header:");
        for (pid, b, c) in rich { let _ = writeln!(w, "  prodid=0x{:04x} {:<28} build={:<6} count={}", pid, rich_name(pid), b, c); }
    }
    let imps = p.imports();
    let _ = writeln!(w, "Imports: {} DLLs, {} functions", imps.len(), imps.iter().map(|x| x.1.len()).sum::<usize>());
    for (dll, f) in &imps {
        let _ = writeln!(w, "  {} ({}): {}", dll, f.len(), f.iter().map(|x| x.0.as_str()).collect::<Vec<_>>().join(", "));
    }
    for (dll, f) in p.delay_imports() { let _ = writeln!(w, "  DELAY {} ({}): {}", dll, f.len(), f.join(", ")); }
    let (en, ex) = p.exports();
    if !ex.is_empty() {
        let _ = writeln!(w, "Exports: dllname={} count={}", en, ex.len());
        for (n, o, r) in ex { let _ = writeln!(w, "  {:5} 0x{:08x} {}", o, r, n); }
    }
    for d in p.debug() { let _ = writeln!(w, "Debug: {}", d); }
    let (tr, _) = p.dirs[9];
    if tr != 0 {
        let o = p.rva2off(tr).unwrap();
        let _ = writeln!(w, "TLS: start=0x{:x} end=0x{:x} index@0x{:x} callbacks@0x{:x}", p.u32at(o), p.u32at(o + 4), p.u32at(o + 8), p.u32at(o + 12));
    } else { let _ = writeln!(w, "TLS: none"); }
    let (lr, _) = p.dirs[10];
    if lr != 0 && !p.is64 {
        let o = p.rva2off(lr).unwrap();
        let sz = p.u32at(o);
        let _ = write!(w, "LoadConfig: size=0x{:x}", sz);
        if sz >= 0x48 { let _ = write!(w, " SecurityCookie=0x{:x} SEHandlerTable=0x{:x} SEHandlerCount={}", p.u32at(o + 0x3c), p.u32at(o + 0x40), p.u32at(o + 0x44)); }
        if sz >= 0x5c { let _ = write!(w, " GuardCFCheck=0x{:x} GuardFlags=0x{:x}", p.u32at(o + 0x48), p.u32at(o + 0x58)); }
        let _ = writeln!(w);
    } else { let _ = writeln!(w, "LoadConfig: none/64-bit"); }
    let _ = writeln!(w, "Exception directory: {}", if p.dirs[3].0 != 0 { format!("{:?}", p.dirs[3]) } else { "none (normal for x86)".into() });
    w
}
