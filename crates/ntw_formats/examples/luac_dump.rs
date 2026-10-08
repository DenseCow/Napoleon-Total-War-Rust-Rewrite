//! Research helper: disassemble the shipped UI `.luac` bytecode (read-only, out of `data.pack`).
//!
//! Lua 5.1 chunk format. Every integer is little-endian.
//! ```text
//! "\x1bLua" u8(0x51) u8(format) u8(endian) u8(int) u8(size_t) u8(Instruction)
//!       u8(lua_Number) u8(integral)
//! then DumpFunction, recursively:
//!   source: string | size_t length INCLUDING the trailing NUL, then the bytes
//!   linedefined int, lastlinedefined int, nups u8, numparams u8, is_vararg u8, maxstacksize u8
//!   sizecode int, code[sizecode] u32
//!   sizek int, then sizek constants of { u8 tag; tag 1 = nil, 3 = bool + u8, 0 = double, 4 = string }
//!   sizelineinfo int, lineinfo[sizelineinfo] int
//!   sizelocvars int, { string; int startpc; int endpc }[]
//!   sizeupvalues int, upvalue names: string[]
//!   sizep int, nested DumpFunction[]
//! ```
//!
//! Instruction word `i`: `op = i & 0x3F`, `A = (i >> 6) & 0xFF`, `C = (i >> 14) & 0x1FF`,
//! `B = (i >> 23) & 0x1FF`, `Bx = (i >> 14) & 0x3FFFF`, `sBx = Bx - 131071`.
//! `BITRK = 1 << 8`. `ISK(x) = x & 256`. `RKB/RKC`: if the operand has BITRK it is
//! `K[Bx & !BITRK]`, otherwise a register.

use std::path::PathBuf;

use ntw_formats::pack::Vfs;

const DATA_DIR: &str = r"C:\Program Files (x86)\Steam\steamapps\common\Napoleon Total War\data";

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        eprintln!("usage: luac_dump <pack-relative path to .luac> [--all|--proto <pc>|--line <n>] [--grep <text>]");
        return;
    }
    let vfs = Vfs::open_install(std::env::var("NTW_DATA_DIR").map(PathBuf::from).unwrap_or_else(|_| PathBuf::from(DATA_DIR))).expect("open install");
    let bytes = vfs.read(&args[0]).unwrap_or_else(|e| panic!("read {}: {e}", args[0]));

    if let Some(at) = args.iter().position(|a| a == "--at").map(|i| args[i + 1].parse::<usize>().expect("offset")) {
        let n = args.iter().position(|a| a == "--n").map(|i| args[i + 1].parse().expect("n")).unwrap_or(96);
        let end = (at + n).min(bytes.len());
        for row in bytes[at..end].chunks(16) {
            let hex: Vec<String> = row.iter().map(|b| format!("{b:02X}")).collect();
            let asc: String = row.iter().map(|&b| if (0x20..0x7F).contains(&b) { b as char } else { '.' }).collect();
            println!("{at:04X}  {:<47}  {}", hex.join(" "), asc);
        }
        return;
    }

    if args.iter().any(|a| a == "--head") {
        let n = args
            .iter()
            .position(|a| a == "--n")
            .map(|i| args[i + 1].parse().expect("n"))
            .unwrap_or(96)
            .min(bytes.len());
        for row in bytes[..n].chunks(16) {
            let hex: Vec<String> = row.iter().map(|b| format!("{b:02X}")).collect();
            let asc: String = row.iter().map(|&b| if (0x20..0x7F).contains(&b) { b as char } else { '.' }).collect();
            println!("{:04X}  {:<47}  {}", row.as_ptr() as usize - bytes.as_ptr() as usize, hex.join(" "), asc);
        }
        return;
    }

    let show_all = args.iter().any(|a| a == "--all");
    let want_grep = args.iter().position(|a| a == "--grep").map(|i| args[i + 1].clone());
    let want_proto: Option<usize> = args.iter().position(|a| a == "--proto").map(|i| args[i + 1].parse().expect("proto pc"));
    // `--line <n>`: every function whose code comes from source line n (the line a runtime
    // error names, e.g. `template.city_info_bar.lua:125`).
    let want_line: Option<i32> = args.iter().position(|a| a == "--line").map(|i| args[i + 1].parse().expect("line"));

    println!("=== {} ({} bytes)", args[0], bytes.len());
    let mut p = Reader::new(&bytes);
    p.chunk();
    let all = p.protos;

    // Recompute each proto's source name from the run of source-line numbers that covers it, so
// `SelectFoo` (line 417) reads as `SelectFoo.lua:417` rather than the bare Lua source path.
let names = source_names(&all);
for (idx, pr) in all.iter().enumerate() {
        let wanted = show_all
            || want_proto == Some(pr.linedefined as usize)
            || want_line.is_some_and(|l| pr.lines.contains(&l));
        if wanted {
            pr.print(&names[idx]);
        }
    }
    if let Some(g) = want_grep {
        println!("\n### grep {g:?} across every constant");
        for pr in &all {
            for (i, c) in pr.constants.iter().enumerate() {
                if let Const::Str(s) = c
                    && s.contains(&g)
                {
                    println!("  {}:{} K[{}] = {:?}", pr.name, pr.linedefined, i, s);
                }
            }
        }
    }
}

#[derive(Debug)]
enum Const {
    Nil,
    Bool(bool),
    Num(f64),
    Str(String),
}

impl Const {
    fn render(&self) -> String {
        match self {
            Const::Nil => "nil  [TYPE nil]".into(),
            Const::Bool(b) => format!("{b}  [TYPE BOOLEAN]"),
            Const::Num(n) => format!("{n}  [TYPE number]"),
            Const::Str(s) => format!("{s:?}  [TYPE string]"),
        }
    }
    fn type_name(&self) -> &'static str {
        match self {
            Const::Nil => "nil",
            Const::Bool(_) => "BOOLEAN",
            Const::Num(_) => "number",
            Const::Str(_) => "string",
        }
    }
}

struct Proto {
    name: String,
    linedefined: i32,
    numparams: u8,
    is_vararg: u8,
    maxstack: u8,
    code: Vec<u32>,
    constants: Vec<Const>,
    upvalue_names: Vec<String>,
    children: Vec<usize>,
    /// The source line of each instruction (the debug section's lineinfo).
    lines: Vec<i32>,
}

/// A label per proto: `<file>:<linedefined>`.
/// The chunk's own `source` string (`@s:/.../UI/Templates/template.BuildingFrame.lua`) only
/// survives on the main proto; children dump an empty source, so they inherit the file part and
/// are distinguished by `linedefined`, which is unique per top-level function.
fn source_names(all: &[Proto]) -> Vec<String> {
    let file = all
        .first()
        .map(|p| p.name.rsplit(['/', '\\']).next().unwrap_or(&p.name).to_string())
        .unwrap_or_else(|| "<chunk>".into());
    all.iter().map(|pr| format!("{file}:{}", pr.linedefined)).collect()
}

/// `Bx` field width and the `sBx` bias (see the note in `disasm`).
const BX_MASK: u32 = 0x3FFFF;
const SBX_BIAS: i64 = 131071;

/// Opcodes that read their jump offset from the FOLLOWING word rather than from their own
/// `sBx`: the comparisons and tests (the VM consumes the next word as data, and `luac` gives
/// that word the `JMP` opcode bits), plus nothing else -- `JMP` / `FORLOOP` / `FORPREP` /
/// `TFORLOOP` carry their offset in place.
const JUMP_VIA_NEXT: [bool; 38] = {
    let mut t = [false; 38];
    t[23] = true; // EQ
    t[24] = true; // LT
    t[25] = true; // LE
    t[26] = true; // TEST
    t[27] = true; // TESTSET
    t
};

/// The same set, for the printing loop that skips the consumed jump word.
const COND_OP: [bool; 38] = JUMP_VIA_NEXT;

impl Proto {
    /// `RK(x)` in Lua 5.1: `ISK(x)` is `x & BITRK` with `BITRK = 1 << 8 = 256`; if set, the operand
/// is a constant index with bit 8 cleared (`x & ~BITRK`), otherwise it is a register number.
fn rk(&self, op: u32) -> String {
        if op & 256 != 0 {
            let k = (op & !256) as usize;
            format!("K[{}] = {}", k, self.const_at(k))
        } else {
            format!("R[{op}]")
        }
    }

    fn print(&self, label: &str) {
        println!(
            "--- proto {label} numparams={} is_vararg={} maxstack={} upvals={:?} ncode={} nconst={} children={:?}",
            self.numparams,
            self.is_vararg,
            self.maxstack,
            self.upvalue_names,
            self.code.len(),
            self.constants.len(),
            self.children
        );
        for (i, c) in self.constants.iter().enumerate() {
            println!("      K[{}] = {}", i, c.render());
        }
        // Conditionals take their jump offset from the following word, so print those two
        // together and skip the consumed word.
        let mut pc = 0usize;
        while pc < self.code.len() {
            let raw = self.code[pc];
            let op = raw & 0x3F;
            let line = self.disasm(pc, raw);
            let src = self.lines.get(pc).map_or(String::new(), |l| format!("[{l}]"));
            println!("    {pc:4} {src:>6}  {line}");
            if COND_OP[op as usize] && pc + 1 < self.code.len() {
                let nxt = self.code[pc + 1];
                println!("    {:4}  [jump word for pc {}] 0x{:08X}", pc + 1, pc, nxt);
                pc += 2;
            } else {
                pc += 1;
            }
        }
    }

    fn const_at(&self, idx: usize) -> String {
        match self.constants.get(idx) {
            Some(Const::Str(s)) => format!("STRING {s:?}"),
            Some(c) => format!("{} {}", c.type_name(), c.render()),
            None => format!("K[{idx}] OUT OF RANGE"),
        }
    }

    fn disasm(&self, pc: usize, i: u32) -> String {
        let op = i & 0x3F;
        let a = (i >> 6) & 0xFF;
        let c = (i >> 14) & 0x1FF;
        let b = (i >> 23) & 0x1FF;
        // CONFIRMED by measurement on this build's own chunks: the `Bx` field is 18 bits at
        // `POS_Bx` = 14 and `sBx` is biased by 131071. The jump word `0x80014016` must
        // decode to `sBx` +6, which only the 18-bit mask gives (17 bits / 65535 yields
        // -65530). So bits 14..31 all belong to `Bx`.
        let bx = (i >> 14) & BX_MASK;
        // The comparison / test opcodes take their jump offset from the NEXT word, which the
        // VM consumes as data rather than executing (Lua 5.1 `lvm.c`, `OP_EQ` & friends).
        let (sbx, target) = match JUMP_VIA_NEXT[op as usize] {
            true => {
                let nxt = self.code.get(pc + 1).copied().unwrap_or(0);
                let s = ((nxt >> 14) & BX_MASK) as i64 - SBX_BIAS;
                (s, (pc as i64 + 2 + s).max(0) as usize)
            }
            false => {
                let s = bx as i64 - SBX_BIAS;
                (s, (pc as i64 + 1 + s).max(0) as usize)
            }
        };
        let jump = |t: usize| format!("  -> {t}");
        match op {
            0 => format!("MOVE        R[{a}] = R[{b}]"),
            1 => format!("LOADK       R[{a}] = K[{}] = {}", bx, self.const_at(bx as usize)),
            2 => format!("LOADBOOL    R[{a}] = {} ; skip next if {}", b != 0, c != 0),
            3 => format!("LOADNIL     R[{a}]..R[{a}+{}] = nil", b),
            4 => format!(
                "GETUPVAL    R[{a}] = upval[{}] ({})",
                b,
                self.upvalue_names.get(b as usize).map(String::as_str).unwrap_or("?")
            ),
            5 => format!("GETGLOBAL   R[{a}] = {}", self.const_at(bx as usize)),
            6 => format!("GETTABLE    R[{a}] = R[{b}][{}]", self.rk(c)),
            7 => format!("SETGLOBAL   {} = R[{a}]", self.const_at(bx as usize)),
            8 => format!("SETUPVAL    upval[{}] = R[{a}]", b),
            9 => format!("SETTABLE    R[{a}][{}] = R[{}]", self.rk(b), self.rk(c)),
            10 => format!("NEWTABLE    R[{a}] {{}}, array={b} hash={c}"),
            11 => format!("SELF        R[{a}] = R[{b}]; R[{a}+1] = R[{b}][{}]", self.rk(c)),
            12 => format!("ADD         R[{a}] = {} + {}", self.rk(b), self.rk(c)),
            13 => format!("SUB         R[{a}] = {} - {}", self.rk(b), self.rk(c)),
            14 => format!("MUL         R[{a}] = {} * {}", self.rk(b), self.rk(c)),
            15 => format!("DIV         R[{a}] = {} / {}", self.rk(b), self.rk(c)),
            16 => format!("MOD         R[{a}] = {} % {}", self.rk(b), self.rk(c)),
            17 => format!("POW         R[{a}] = {} ^ {}", self.rk(b), self.rk(c)),
            18 => format!("UNM         R[{a}] = -{}", self.rk(b)),
            19 => format!("NOT         R[{a}] = not {}", self.rk(b)),
            20 => format!("LEN         R[{a}] = #{}", self.rk(b)),
            21 => format!("CONCAT      R[{a}] = {} .. .. {}", self.rk(b), self.rk(c)),
            22 => format!("JMP         {}{}", sbx, jump(target)),
            23 => format!("EQ          if ({} == {}) == {a} then jump{}", self.rk(b), self.rk(c), jump(target)),
            24 => format!("LT          if ({} <  {}) == {a} then jump{}", self.rk(b), self.rk(c), jump(target)),
            25 => format!("LE          if ({} <= {}) == {a} then jump{}", self.rk(b), self.rk(c), jump(target)),
            26 => {
                // Lua 5.1 `OP_TEST`: jump when `l_isfalse(R[A]) != C`, so C = 0 jumps on a false
                // value and C = 1 on a true one. (This used to key on A's parity, which printed
                // "jump if true" for every even register; every TEST in `template.buildingframe.luac` has C = 0.)
                let cond = if c != 0 { "true" } else { "false" };
                format!("TEST        R[{a}] ; jump if {cond}{}", jump(target))
            }
            27 => {
                // `OP_TESTSET`: copy and jump when R[B]'s truthiness equals C (C = 0: `and`).
                let cond = if c != 0 { "true" } else { "false" };
                format!("TESTSET     R[{a}] = {} ; jump if {cond}{}", self.rk(b), jump(target))
            }
            28 => format!("CALL        R[{a}] nargs={} nres={}", b as i64 - 1, c as i64 - 1),
            29 => format!("TAILCALL    R[{a}] nargs={}", b as i64 - 1),
            30 => format!("RETURN      R[{a}]..R[{a}+{}]", b as i64 - 1),
            31 => format!("FORLOOP     R[{a}] ; jump{}", jump(target)),
            32 => format!("FORPREP     R[{a}] ; jump{}", jump(target)),
            33 => format!("TFORLOOP    R[{a}] ; jump{}", jump(target)),
            34 => format!("SETLIST     R[{a}] n={}", b as i64 - 1),
            35 => format!("CLOSE       R[{a}]"),
            // CLOSURE's operand is Bx, not B (it made every proto index print as 0).
            36 => format!("CLOSURE     R[{a}] = proto[{bx}]"),
            37 => format!("VARARG      R[{a}] n={}", b as i64 - 1),
            o => format!("??? opcode {o} raw 0x{i:08X}"),
        }
    }
}

#[allow(dead_code)]
/// Is a plausible `sizelocvars` int sitting at `at`?
fn plausible_locvars_count(b: &[u8], at: usize) -> bool {
    if at + 4 > b.len() {
        return false;
    }
    let n = i32::from_le_bytes(b[at..at + 4].try_into().unwrap());
    if !(0..=4096).contains(&n) {
        return false;
    }
    if n == 0 {
        return true;
    }
    // walk the records: name (size_t len + bytes incl. NUL) then two ints
    let mut p = at + 4;
    for _ in 0..n {
        if p + 4 > b.len() {
            return false;
        }
        let len = u32::from_le_bytes(b[p..p + 4].try_into().unwrap()) as usize;
        if len > 4096 {
            return false;
        }
        if len != 0 {
            // a non-empty name must be NUL-terminated printable ASCII
            if p + 4 + len > b.len() {
                return false;
            }
            let name = &b[p + 4..p + 4 + len];
            if name.last() != Some(&0) || !name[..len - 1].iter().all(|&c| c.is_ascii_graphic() || c == b' ') {
                return false;
            }
        }
        p += 4 + len + 8;
    }
    p <= b.len()
}

struct Reader<'a> {
    b: &'a [u8],
    pos: usize,
    number_size: u8,
    protos: Vec<Proto>,
}

impl<'a> Reader<'a> {
    fn new(b: &'a [u8]) -> Self {
        Reader { b, pos: 0, number_size: 8, protos: Vec::new() }
    }

    fn take(&mut self, n: usize) -> &'a [u8] {
        let s = &self.b[self.pos..self.pos + n];
        self.pos += n;
        s
    }
    fn u8(&mut self) -> u8 {
        self.take(1)[0]
    }
    fn i32(&mut self) -> i32 {
        i32::from_le_bytes(self.take(4).try_into().unwrap())
    }
    fn f64(&mut self) -> f64 {
        f64::from_le_bytes(self.take(8).try_into().unwrap())
    }
    fn number(&mut self, size: u8) -> f64 {
        match size {
            4 => f32::from_le_bytes(self.take(4).try_into().unwrap()) as f64,
            8 => self.f64(),
            s => panic!("unsupported lua_Number size {s}"),
        }
    }
    fn string(&mut self) -> String {
        let n = u32::from_le_bytes(self.take(4).try_into().unwrap()) as usize;
        let raw = self.take(n);
        // the stored length includes the trailing NUL
        let raw = if raw.last() == Some(&0) { &raw[..raw.len() - 1] } else { raw };
        String::from_utf8_lossy(raw).into_owned()
    }

    fn chunk(&mut self) {
        let magic = self.take(4);
        assert_eq!(magic, b"\x1bLua", "not a Lua chunk");
        let version = self.u8();
        let format = self.u8();
        let endian = self.u8();
        let int_size = self.u8();
        let size_t = self.u8();
        let instr = self.u8();
        let num_size = self.u8();
        let integral = self.u8();
        println!(
            "  header: version=0x{version:02X} format={format} endian={endian} int={int_size} \
             size_t={size_t} instruction={instr} number={num_size} integral={integral}"
        );
        assert_eq!(int_size, 4, "int size");
        assert_eq!(size_t, 4, "size_t");
        assert_eq!(instr, 4, "instruction size");
        assert!(num_size == 4 || num_size == 8, "lua_Number size {num_size}");
        assert_eq!(endian, 1, "little endian");
        assert_eq!(format, 0, "official format");
        self.number_size = num_size;

        let idx = self.function(0);
        println!("  main proto index = {idx}, total protos = {}", self.protos.len());
    }

    fn function(&mut self, depth: usize) -> usize {
        let at = self.pos;
        let source = self.string();
        let linedefined = self.i32();
        let _lastlinedefined = self.i32();
        let nups = self.u8();
        let numparams = self.u8();
        let is_vararg = self.u8();
        let maxstacksize = self.u8();
        let trace = std::env::var_os("LUAC_TRACE").is_some();
        if trace {
            eprintln!("[trace] @{at} proto {}:{linedefined} nups={nups} numparams={numparams} is_vararg={is_vararg} maxstack={maxstacksize}", source);
        }

        let ncode = self.i32() as usize;
        if trace {
            eprintln!("[trace]   ncode={ncode}");
        }
        let mut code = Vec::with_capacity(ncode);
        for _ in 0..ncode {
            code.push(u32::from_le_bytes(self.take(4).try_into().unwrap()));
        }

        let nk = self.i32() as usize;
        if trace {
            eprintln!("[trace]   sizek={nk}");
        }
        let mut constants = Vec::with_capacity(nk);
        for k in 0..nk {
            let before = self.pos;
            let tag = self.u8();
            let c = match tag {
                0 => Const::Nil,
                1 => Const::Bool(self.u8() != 0),
                3 => Const::Num(self.number(self.number_size)),
                4 => Const::Str(self.string()),
                t => panic!("unknown constant tag {t} at {before}"),
            };
            if trace {
                let brief = match &c {
                    Const::Str(s) if s.len() > 40 => format!("{:?}...", &s[..40]),
                    other => format!("{other:?}"),
                };
                eprintln!("[trace]     K[{k}] @{before} tag={tag} {brief} -> @{}", self.pos);
            }
            constants.push(c);
        }

        let me = self.protos.len();
        self.protos.push(Proto {
            name: source,
            linedefined,
            numparams,
            is_vararg,
            maxstack: maxstacksize,
            code,
            constants,
            upvalue_names: Vec::new(),
            children: Vec::new(),
            lines: Vec::new(),
        });
        let _ = depth;

        // Order matters: `protos` come BEFORE the debug section (the layout `ntw_script::luac`
        // documents and the chunk converter walks, CONFIRMED by that code running the game's
        // scripts). So: children first, then lineinfo / locvars / upvalue names.
        let np = self.i32() as usize;
        let mut kids = Vec::with_capacity(np);
        for _ in 0..np {
            kids.push(self.function(depth + 1));
        }
        self.protos[me].children = kids;

        let nline = self.i32() as usize;
        if trace {
            eprintln!("[trace]   sizelineinfo={nline}");
        }
        let lines: Vec<i32> = (0..nline).map(|_| self.i32()).collect();
        self.protos[me].lines = lines;
        let nloc = self.i32() as usize;
        if trace {
            eprintln!("[trace]   sizelocvars={nloc}");
        }
        for _ in 0..nloc {
            self.string();
            self.pos += 8;
        }
        let nupn = self.i32() as usize;
        if trace {
            eprintln!("[trace]   sizeupvalues={nupn} (header nups said {nups})");
        }
        let mut upvalue_names = Vec::with_capacity(nupn);
        for _ in 0..nupn {
            upvalue_names.push(self.string());
        }
        self.protos[me].upvalue_names = upvalue_names;
        me
    }
}
