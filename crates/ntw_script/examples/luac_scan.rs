//! Research helper: find every call site of one method across the original's compiled UI `.luac`
//! scripts (read-only, out of `data.pack`).
//!
//! The exe's Lua API surface (`analysis/worker3/lua_api.txt`) lists `panel_manager.OpenPanel` with
//! a call count but not the arguments. This scans the bytecode with a small register tracker, so
//! each site prints as `<file>:<line>  recv:Method(arg, arg, ...)`.
//!
//! ```text
//! cargo run -p ntw_script --example luac_scan -- OpenPanel
//! cargo run -p ntw_script --example luac_scan -- ClosePanel
//! cargo run -p ntw_script --example luac_scan -- OpenPanel ui\army
//! cargo run -p ntw_script --example luac_scan -- strings OpenAgentOptionsPopup
//! ```
//!
//! `strings <text> [file substring]` is the other mode: it lists every `.luac` whose **constants**
//! contain the text, which is how a name that is only ever a string (a root-layout global, an engine
//! call the scripts never make) is proved to have no caller at all.
//!
//! A register's description is whatever the last instruction wrote into it, so `nil` shows as
//! `nil` and a parameter shows as `local0`. Anything the tracker cannot follow is `r<n>`, and
//! `panel_manager` itself is an upvalue in the shipped scripts, so it reads as
//! `upval:panel_manager`.
use std::collections::BTreeMap;

use ntw_formats::pack::Vfs;

const DATA: &str = r"C:\Program Files (x86)\Steam\steamapps\common\Napoleon Total War\data";

#[derive(Clone)]
enum K {
    Nil,
    Bool(bool),
    Num(f32),
    Str(String),
}

impl std::fmt::Display for K {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            K::Nil => write!(f, "nil"),
            K::Bool(b) => write!(f, "{b}"),
            K::Num(n) => write!(f, "{n}"),
            K::Str(s) => write!(f, "{s}"),
        }
    }
}

struct Proto {
    line: i32,
    numparams: u8,
    code: Vec<u32>,
    ks: Vec<K>,
    ups: Vec<String>,
}

struct Reader<'a> {
    b: &'a [u8],
    p: usize,
}

impl Reader<'_> {
    fn u8(&mut self) -> u8 {
        let v = self.b[self.p];
        self.p += 1;
        v
    }
    fn u32(&mut self) -> u32 {
        let v = u32::from_le_bytes(self.b[self.p..self.p + 4].try_into().unwrap());
        self.p += 4;
        v
    }
    /// A stored string: `size_t` length including the trailing NUL.
    fn s(&mut self) -> String {
        let n = self.u32() as usize;
        let raw = &self.b[self.p..self.p + n];
        self.p += n;
        String::from_utf8_lossy(raw).trim_end_matches('\0').to_owned()
    }
    fn proto(&mut self, out: &mut Vec<Proto>) {
        self.s();
        let line = self.u32() as i32;
        let _last = self.u32();
        let _nups = self.u8();
        let numparams = self.u8();
        let _vararg = self.u8();
        let _stack = self.u8();
        let n = self.u32() as usize;
        let code: Vec<u32> = (0..n).map(|_| self.u32()).collect();
        let nk = self.u32() as usize;
        let mut ks = Vec::with_capacity(nk);
        for _ in 0..nk {
            ks.push(match self.u8() {
                0 => K::Nil,
                1 => K::Bool(self.u8() != 0),
                3 => {
                    let v = self.u32();
                    K::Num(f32::from_bits(v))
                }
                4 => K::Str(self.s()),
                t => panic!("constant type {t}"),
            });
        }
        let me = out.len();
        out.push(Proto { line, numparams, code, ks, ups: Vec::new() });
        let np = self.u32() as usize;
        for _ in 0..np {
            self.proto(out);
        }
        let nl = self.u32() as usize;
        self.p += nl * 4;
        let nloc = self.u32() as usize;
        for _ in 0..nloc {
            self.s();
            self.p += 8;
        }
        let nu = self.u32() as usize;
        let mut ups = Vec::with_capacity(nu);
        for _ in 0..nu {
            ups.push(self.s());
        }
        out[me].ups = ups;
    }
}

/// One call site: the receiver/method and the argument descriptions, in order.
struct Site {
    file: String,
    line: i32,
    pc: usize,
    recv: String,
    method: String,
    args: Vec<String>,
}

fn track(pr: &Proto, file: &str, method: &str, out: &mut Vec<Site>) {
    let mut reg: BTreeMap<u32, String> = BTreeMap::new();
    for i in 0..pr.numparams as u32 {
        reg.insert(i, format!("local{i}"));
    }
    fn rk(x: u32, pr: &Proto, reg: &BTreeMap<u32, String>) -> String {
        if x & 0x100 != 0 {
            let i = (x & 0xFF) as usize;
            pr.ks.get(i).map(|k| k.to_string()).unwrap_or_else(|| format!("K[{i}]"))
        } else {
            reg.get(&x).cloned().unwrap_or_else(|| format!("r{x}"))
        }
    }
    for (pc, &i) in pr.code.iter().enumerate() {
        let op = (i & 0x3F) as usize;
        let a = (i >> 6) & 0xFF;
        let c = (i >> 14) & 0x1FF;
        let b = (i >> 23) & 0x1FF;
        let bx = i >> 14;
        let get = |r: &BTreeMap<u32, String>, k: u32| r.get(&k).cloned().unwrap_or_else(|| format!("r{k}"));
        match op {
            // GETGLOBAL and SETGLOBAL read Bx (18 bits), not the B field printed above.
            5 => {
                reg.insert(a, format!("global:{}", pr.ks.get(bx as usize).map(|k| k.to_string()).unwrap_or_default()));
            }
            1 => {
                reg.insert(a, format!("const:{}", pr.ks.get(bx as usize).map(|k| k.to_string()).unwrap_or_default()));
            }
            4 => {
                reg.insert(a, format!("upval:{}", pr.ups.get(b as usize).cloned().unwrap_or_else(|| b.to_string())));
            }
            0 | 7 | 9 => {
                let v = get(&reg, a);
                reg.insert(a, v);
            }
            6 => {
                let base = get(&reg, b);
                let key = rk(c, pr, &reg);
                reg.insert(a, format!("{base}[{key}]"));
            }
            11 => {
                let base = get(&reg, b);
                let key = rk(c, pr, &reg);
                reg.insert(a + 1, format!("{base}.fn"));
                reg.insert(a, format!("{base}:{key}"));
            }
            28 => {
                // CALL encodes `nargs + 1` in B and `nresults + 1` in C (Lua 5.1 `lvm.c`).
                let nargs = b.saturating_sub(1);
                let recv = get(&reg, a);
                // The callee is written either `recv:Method(...)` (SELF, so `recv:Method`) or
                // `recv.Method(...)` (GETTABLE, so `recv[Method]`); both name the method last.
                let tail = |s: &str| s.rsplit(':').next().unwrap_or(s).to_owned();
                let named = [recv.clone(), tail(&recv)]
                    .iter()
                    .filter_map(|s| s.rsplit_once('[').map(|x| x.1.trim_end_matches(']').to_owned()))
                    .chain(std::iter::once(if recv.contains('[') { String::new() } else { tail(&recv) }))
                    .find(|m| !m.is_empty() && m.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'));
                if named.as_deref() == Some(method) {
                    let args = (1..=nargs).map(|k| get(&reg, a + k)).collect();
                    out.push(Site { file: file.to_owned(), line: pr.line, pc, recv: recv.clone(), method: method.to_owned(), args });
                }
                for k in 0..c.saturating_sub(1) {
                    reg.insert(a + k, "ret".to_owned());
                }
            }
            _ => {}
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    // `strings <text> [file substring]`: every `.luac` whose constants contain the text. Use this
    // when looking for a name that is only ever a string (a root-layout global, an engine call).
    if args.first().map(String::as_str) == Some("strings") {
        let needle = args.get(1).cloned().unwrap_or_default();
        let only = args.get(2).cloned().unwrap_or_default();
        let vfs = Vfs::open_install(std::env::var("NTW_DATA_DIR").unwrap_or_else(|_| DATA.into())).unwrap();
        for p in vfs.list("ui") {
            if !p.to_ascii_lowercase().ends_with(".luac") {
                continue;
            }
            if !only.is_empty() && !p.to_ascii_lowercase().contains(&only.to_ascii_lowercase()) {
                continue;
            }
            let Ok(b) = vfs.read(p) else { continue };
            if !b.starts_with(b"\x1bLua") {
                continue;
            }
            let mut r = Reader { b: &b, p: 12 };
            let mut protos = Vec::new();
            r.proto(&mut protos);
            for pr in &protos {
                for (i, k) in pr.ks.iter().enumerate() {
                    if let K::Str(s) = k
                        && s.contains(&needle)
                    {
                        println!("{}:{} K[{i}] = {s:?}", p, pr.line);
                    }
                }
            }
        }
        return;
    }
    let method = args.first().cloned().unwrap_or_else(|| "OpenPanel".into());
    let only = args.get(1).cloned().unwrap_or_default();
    let vfs = Vfs::open_install(std::env::var("NTW_DATA_DIR").unwrap_or_else(|_| DATA.into())).unwrap();
    let mut out = Vec::new();
    let mut files = 0usize;
    for p in vfs.list("ui") {
        if !p.to_ascii_lowercase().ends_with(".luac") {
            continue;
        }
        if !only.is_empty() && !p.to_ascii_lowercase().contains(&only.to_ascii_lowercase()) {
            continue;
        }
        files += 1;
        let Ok(b) = vfs.read(p) else { continue };
        if !b.starts_with(b"\x1bLua") {
            continue;
        }
        let mut r = Reader { b: &b, p: 12 };
        let mut protos = Vec::new();
        r.proto(&mut protos);
        for pr in &protos {
            track(pr, p, &method, &mut out);
        }
    }
    out.sort_by_key(|s| (s.file.clone(), s.line));
    for s in &out {
        println!("{}:{} pc={}  {}:{}({})", s.file, s.line, s.pc, s.recv, s.method, s.args.join(", "));
    }
    eprintln!("{files} .luac files scanned, {} call sites of {method}", out.len());
}
