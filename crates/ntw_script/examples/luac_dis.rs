//! Research helper: a Lua 5.1 bytecode lister for the original `.luac` UI scripts (read-only).
//! Prints each function's instructions with constants resolved, so the script flow can be read.
//!   cargo run -p ntw_script --release --example luac_dis -- "ui/frontend ui/main.main.luac"
use ntw_formats::pack::Vfs;

const DATA: &str = r"C:\Program Files (x86)\Steam\steamapps\common\Napoleon Total War\data";
const OPS: [&str; 38] = [
    "MOVE", "LOADK", "LOADBOOL", "LOADNIL", "GETUPVAL", "GETGLOBAL", "GETTABLE", "SETGLOBAL", "SETUPVAL", "SETTABLE",
    "NEWTABLE", "SELF", "ADD", "SUB", "MUL", "DIV", "MOD", "POW", "UNM", "NOT", "LEN", "CONCAT", "JMP", "EQ", "LT", "LE",
    "TEST", "TESTSET", "CALL", "TAILCALL", "RETURN", "FORLOOP", "FORPREP", "TFORLOOP", "SETLIST", "CLOSE", "CLOSURE",
    "VARARG",
];

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
            K::Str(s) => write!(f, "{s:?}"),
        }
    }
}

struct R<'a> {
    b: &'a [u8],
    p: usize,
}

impl R<'_> {
    fn u8(&mut self) -> u8 {
        self.p += 1;
        self.b[self.p - 1]
    }
    fn u32(&mut self) -> u32 {
        self.p += 4;
        u32::from_le_bytes(self.b[self.p - 4..self.p].try_into().unwrap())
    }
    fn s(&mut self) -> String {
        let n = self.u32() as usize;
        self.p += n;
        String::from_utf8_lossy(&self.b[self.p - n..self.p.saturating_sub(1).max(self.p - n)]).into_owned()
    }
    fn func(&mut self, name: &str, depth: usize) {
        let src = self.s();
        let line = self.u32();
        let _last = self.u32();
        let (nups, nparams, vararg, _stack) = (self.u8(), self.u8(), self.u8(), self.u8());
        let n = self.u32() as usize;
        let code: Vec<u32> = (0..n).map(|_| self.u32()).collect();
        let nk = self.u32() as usize;
        let ks: Vec<K> = (0..nk)
            .map(|_| match self.u8() {
                0 => K::Nil,
                1 => K::Bool(self.u8() != 0),
                3 => K::Num(f32::from_bits(self.u32())),
                4 => K::Str(self.s()),
                t => panic!("constant type {t}"),
            })
            .collect();
        let np = self.u32() as usize;
        let pad = "  ".repeat(depth);
        println!("{pad}function {name} (line {line}, params {nparams}, upvalues {nups}, vararg {vararg}) {}", if src.is_empty() { "" } else { &src });
        let rk = |x: u32| if x & 0x100 != 0 { format!("{}", ks[(x & 0xFF) as usize]) } else { format!("r{x}") };
        for (pc, &i) in code.iter().enumerate() {
            let op = (i & 0x3F) as usize;
            let a = (i >> 6) & 0xFF;
            let c = (i >> 14) & 0x1FF;
            let b = (i >> 23) & 0x1FF;
            let bx = i >> 14;
            let sbx = bx as i64 - 131071;
            let name = OPS.get(op).copied().unwrap_or("?");
            let text = match name {
                "LOADK" | "GETGLOBAL" | "SETGLOBAL" => format!("r{a} {}", ks[bx as usize]),
                "GETTABLE" => format!("r{a} = r{b}[{}]", rk(c)),
                "SETTABLE" => format!("r{a}[{}] = {}", rk(b), rk(c)),
                "SELF" => format!("r{a} = r{b}:{}", rk(c)),
                "ADD" | "SUB" | "MUL" | "DIV" | "MOD" | "POW" | "EQ" | "LT" | "LE" => format!("{a} {} {}", rk(b), rk(c)),
                "JMP" | "FORLOOP" | "FORPREP" => format!("-> {}", pc as i64 + 1 + sbx),
                "CLOSURE" => format!("r{a} = proto {bx}"),
                _ => format!("{a} {b} {c}"),
            };
            println!("{pad}  [{pc:3}] {name:<9} {text}");
        }
        for i in 0..np {
            self.func(&format!("{name}/{i}"), depth + 1);
        }
        let nl = self.u32() as usize;
        self.p += nl * 4;
        let nloc = self.u32() as usize;
        for _ in 0..nloc {
            self.s();
            self.p += 8;
        }
        let nu = self.u32() as usize;
        for _ in 0..nu {
            self.s();
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let vfs = Vfs::open_install(std::env::var("NTW_DATA_DIR").unwrap_or_else(|_| DATA.into())).unwrap();
    // `--list <prefix> [substring]`: the pack paths under a folder (to find a script's name).
    if args[0] == "--list" {
        let sub = args.get(2).map(|s| s.to_ascii_lowercase()).unwrap_or_default();
        for p in vfs.list(&args[1]).into_iter().filter(|p| p.to_ascii_lowercase().contains(&sub)) {
            println!("{p}");
        }
        return;
    }
    // `--loc <substring>`: localisation entries whose key contains the substring.
    if args[0] == "--loc" {
        let loc = ntw_formats::loc::Localisation::from_vfs(&vfs).unwrap();
        let mut v: Vec<(&str, &str)> = loc.iter().filter(|(k, _)| k.contains(args[1].as_str())).collect();
        v.sort();
        for (k, t) in v {
            println!("{k} = {t:?}");
        }
        return;
    }
    // `--fc`: the font categories (`ui/fontcategories.fc`): index, name, font, colour.
    if args[0] == "--fc" {
        let fc = ntw_formats::font::FontCategories::read(&vfs.read("ui/fontcategories.fc").unwrap()).unwrap();
        for e in fc.entries {
            println!("{:4} {:40} {:28} {:08x} lead {} trk {}", e.index, e.name, e.font, e.colour, e.leading, e.tracking);
        }
        return;
    }
    // `--cat <path>`: a pack file's raw bytes to stdout (text files such as `text\credits.xml`).
    if args[0] == "--cat" {
        use std::io::Write;
        std::io::stdout().write_all(&vfs.read(&args[1]).unwrap()).unwrap();
        return;
    }
    let b = vfs.read(&args[0]).unwrap();
    let mut r = R { b: &b, p: 12 };
    r.func("main", 0);
}
