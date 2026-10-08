//! Napoleon: Total War binary DB tables (`db/<name>_tables/<name>` in data.pack).
//!
//! # Layout (verified from bytes)
//! ```text
//! [optional] FC FD FE FF  u32 version      -- present only when version > 0
//! u8   0x01                                -- marker (always 1 in NTW)
//! u32  row_count
//! rows: row_count x fields, no per-row framing, no schema in the file
//! ```
//! Field encodings observed:
//! * `Str`    : u16 code-unit count + UTF-16LE text (no terminator)
//! * `OptStr` : u8 flag (0/1); if 1, followed by a `Str`
//! * `Bool`   : u8 0/1
//! * `I32`/`F32`: 4 bytes little-endian (indistinguishable structurally;
//!   classified per column from the values)
//!
//! Because the file carries no column list, the schema is *inferred*: we search
//! for a type sequence that parses row 0 and row 1 in lock-step, then require
//! that the same schema parses every row and ends exactly at EOF.

use std::collections::HashSet;
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FieldType {
    Str,
    OptStr,
    Bool,
    /// 4-byte number before classification.
    N32,
    I32,
    F32,
    /// 2-byte integer (only tried in a fallback pass).
    I16,
}

impl FieldType {
    pub fn code(self) -> &'static str {
        match self {
            FieldType::Str => "str",
            FieldType::OptStr => "ostr",
            FieldType::Bool => "bool",
            FieldType::N32 => "n32",
            FieldType::I32 => "i32",
            FieldType::F32 => "f32",
            FieldType::I16 => "i16",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "str" | "s" => FieldType::Str,
            "ostr" | "o" => FieldType::OptStr,
            "bool" | "b" => FieldType::Bool,
            "n32" | "n" => FieldType::N32,
            "i32" | "i" => FieldType::I32,
            "f32" | "f" => FieldType::F32,
            "i16" | "h" => FieldType::I16,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Str(String),
    OptStr(Option<String>),
    Bool(bool),
    I32(i32),
    F32(f32),
    I16(i16),
    Raw4([u8; 4]),
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Str(s) => write!(f, "{s:?}"),
            Value::OptStr(Some(s)) => write!(f, "{s:?}"),
            Value::OptStr(None) => write!(f, "None"),
            Value::Bool(b) => write!(f, "{b}"),
            Value::I32(i) => write!(f, "{i}"),
            Value::F32(x) => write!(f, "{x}"),
            Value::I16(i) => write!(f, "{i}"),
            Value::Raw4(b) => write!(f, "{b:02x?}"),
        }
    }
}

/// Table header.
#[derive(Debug, Clone, Copy)]
pub struct DbHeader {
    pub version: u32,
    pub marker: u8,
    pub rows: u32,
    /// Offset of the first row.
    pub data_start: usize,
}

pub fn read_header(b: &[u8]) -> Option<DbHeader> {
    let (mut p, mut version) = (0usize, 0u32);
    if b.len() >= 8 && b[..4] == [0xFC, 0xFD, 0xFE, 0xFF] {
        version = u32::from_le_bytes(b[4..8].try_into().ok()?);
        p = 8;
    }
    if b.len() < p + 5 {
        return None;
    }
    let marker = b[p];
    let rows = u32::from_le_bytes(b[p + 1..p + 5].try_into().ok()?);
    Some(DbHeader { version, marker, rows, data_start: p + 5 })
}

fn ok_char(c: u16) -> bool {
    matches!(c, 0x20..=0x7e | 9 | 10 | 13 | 0xa0..=0x24f | 0x370..=0x4ff | 0x2000..=0x206f | 0x20ac | 0x2122)
}

/// Read a u16-length UTF-16LE string at `p`. Returns (text, next_pos).
pub fn read_str(b: &[u8], p: usize) -> Option<(String, usize)> {
    if p + 2 > b.len() {
        return None;
    }
    let n = u16::from_le_bytes([b[p], b[p + 1]]) as usize;
    let e = p + 2 + 2 * n;
    if n > 4000 || e > b.len() {
        return None;
    }
    let mut units = Vec::with_capacity(n);
    for i in 0..n {
        let c = u16::from_le_bytes([b[p + 2 + 2 * i], b[p + 3 + 2 * i]]);
        if !ok_char(c) {
            return None;
        }
        units.push(c);
    }
    Some((String::from_utf16_lossy(&units), e))
}

/// Structural step: where does a field of type `t` starting at `p` end? (no allocation)
fn step(b: &[u8], p: usize, t: FieldType) -> Option<usize> {
    match t {
        FieldType::Str => str_end(b, p),
        FieldType::OptStr => match b.get(p)? {
            0 => Some(p + 1),
            1 => str_end(b, p + 1),
            _ => None,
        },
        FieldType::Bool => (*b.get(p)? <= 1).then_some(p + 1),
        FieldType::N32 | FieldType::I32 | FieldType::F32 => (p + 4 <= b.len()).then_some(p + 4),
        FieldType::I16 => (p + 2 <= b.len()).then_some(p + 2),
    }
}

fn str_end(b: &[u8], p: usize) -> Option<usize> {
    if p + 2 > b.len() {
        return None;
    }
    let n = u16::from_le_bytes([b[p], b[p + 1]]) as usize;
    let e = p + 2 + 2 * n;
    if n > 4000 || e > b.len() {
        return None;
    }
    for i in 0..n {
        if !ok_char(u16::from_le_bytes([b[p + 2 + 2 * i], b[p + 3 + 2 * i]])) {
            return None;
        }
    }
    Some(e)
}

/// Does `schema` parse all rows and end exactly at EOF?
/// Can the 4-byte values at these cursors form one column? Either every value is a
/// plausible integer (|i| < 2,000,000; excludes UTF-16 text pairs >= 0x00200020) or every value is a plausible float
/// (0, or 1e-5 <= |f| <= 1e7). Rejects mis-aligned reads such as 0xCCCD0000.
pub fn n32_plausible(b: &[u8], cur: &[usize]) -> bool {
    let (mut int_ok, mut float_ok) = (true, true);
    for &c in cur {
        let Some(r) = b.get(c..c + 4) else { return false };
        let r: [u8; 4] = r.try_into().unwrap();
        int_ok &= i32_ok(i32::from_le_bytes(r));
        float_ok &= f32_ok(f32::from_le_bytes(r));
        if !int_ok && !float_ok {
            return false;
        }
    }
    true
}

fn i32_ok(i: i32) -> bool {
    // also reject byte-shifted values such as 0x00030000 (low 16 bits zero, >= 65536)
    let a = i.unsigned_abs();
    a < INT_LIMIT.load(std::sync::atomic::Ordering::Relaxed) && !(a >= 65536 && a & 0xFFFF == 0)
}
fn f32_ok(f: f32) -> bool {
    f.is_finite() && (f == 0.0 || (f.abs() >= 1e-5 && f.abs() <= 1e7))
}

/// Does `schema` parse all rows, end exactly at EOF, and give plausible numeric columns?
pub fn validate(b: &[u8], h: &DbHeader, schema: &[FieldType]) -> bool {
    let mut p = h.data_start;
    let mut flags = vec![(true, true); schema.len()];
    for _ in 0..h.rows {
        for (ci, &t) in schema.iter().enumerate() {
            if t == FieldType::N32 {
                let Some(r) = b.get(p..p + 4) else { return false };
                let r: [u8; 4] = r.try_into().unwrap();
                flags[ci].0 &= i32_ok(i32::from_le_bytes(r));
                flags[ci].1 &= f32_ok(f32::from_le_bytes(r));
                if !flags[ci].0 && !flags[ci].1 {
                    return false;
                }
            }
            match step(b, p, t) {
                Some(n) => p = n,
                None => return false,
            }
        }
    }
    p == b.len()
}

/// Candidate types at a position, in preference order.
fn candidates(b: &[u8], p: usize, allow_i16: bool) -> Vec<FieldType> {
    let mut c = Vec::with_capacity(6);
    let s = str_end(b, p);
    let nonempty_str = s.map(|e| e > p + 2).unwrap_or(false);
    if nonempty_str {
        c.push(FieldType::Str);
    }
    if b.get(p) == Some(&1) && str_end(b, p + 1).map(|e| e > p + 3).unwrap_or(false) {
        c.push(FieldType::OptStr);
    }
    if p + 4 <= b.len() {
        c.push(FieldType::N32);
    }
    if b.get(p).map(|&x| x <= 1).unwrap_or(false) {
        c.push(FieldType::Bool);
    }
    if s.is_some() && !nonempty_str {
        c.push(FieldType::Str);
    }
    if b.get(p) == Some(&0) {
        c.push(FieldType::OptStr);
    }
    if allow_i16 && p + 2 <= b.len() {
        c.push(FieldType::I16);
    }
    c
}

pub struct InferResult {
    pub header: DbHeader,
    pub schema: Option<Vec<FieldType>>,
    pub nodes: u64,
}

struct Search<'a> {
    b: &'a [u8],
    h: DbHeader,
    s1: usize,
    dead: HashSet<(usize, usize)>,
    nodes: u64,
    max_nodes: u64,
    max_cols: usize,
    allow_i16: bool,
}

impl<'a> Search<'a> {
    /// Lock-step DFS: c0 walks row 0 (must land exactly on s1), c1 walks row 1.
    /// Returns (found_solution, reached_any_completion).
    fn dfs(&mut self, schema: &mut Vec<FieldType>, c0: usize, c1: usize) -> (bool, bool) {
        self.nodes += 1;
        if self.nodes > self.max_nodes {
            return (false, true);
        }
        if c0 == self.s1 {
            // row 0 complete; whole-table check.
            return (validate(self.b, &self.h, schema), true);
        }
        if c0 > self.s1 || schema.len() >= self.max_cols || self.dead.contains(&(c0, c1)) {
            return (false, false);
        }
        let mut reached = false;
        for t in candidates(self.b, c0, self.allow_i16) {
            if t == FieldType::N32 && !n32_plausible(self.b, &[c0, c1]) { continue; }
            let (Some(n0), Some(n1)) = (step(self.b, c0, t), step(self.b, c1, t)) else { continue };
            if n0 > self.s1 {
                continue;
            }
            schema.push(t);
            let (ok, r) = self.dfs(schema, n0, n1);
            if ok {
                return (true, true);
            }
            schema.pop();
            reached |= r;
        }
        if !reached {
            self.dead.insert((c0, c1));
        }
        (false, reached)
    }
}

/// Locate every row start using the leading run of non-empty key strings in row 0
/// (e.g. `key, category, ...`). Returns `None` unless exactly `rows` matches are found.
pub fn row_starts_by_signature(b: &[u8], h: &DbHeader) -> Option<Vec<usize>> {
    let mut sig = 0usize;
    let mut p = h.data_start;
    while sig < 4 {
        match str_end(b, p) {
            Some(e) if e > p + 2 => {
                sig += 1;
                p = e;
            }
            _ => break,
        }
    }
    // try longest signature first, then shorter ones
    for k in (1..=sig).rev() {
        let mut starts = Vec::with_capacity(h.rows as usize);
        let mut p = h.data_start;
        while p + 2 <= b.len() {
            let mut q = p;
            let mut ok = true;
            let mut first_end = p;
            for i in 0..k {
                match str_end(b, q) {
                    Some(e) if e > q + 2 => {
                        if i == 0 {
                            first_end = e;
                        }
                        q = e;
                    }
                    _ => {
                        ok = false;
                        break;
                    }
                }
            }
            if ok {
                starts.push(p);
                p = first_end;
            } else {
                p += 1;
            }
            if starts.len() > h.rows as usize {
                break;
            }
        }
        if starts.len() == h.rows as usize {
            return Some(starts);
        }
    }
    None
}

struct MultiSearch<'a> {
    b: &'a [u8],
    ends: Vec<usize>,
    dead: HashSet<u64>,
    nodes: u64,
    max_nodes: u64,
    allow_i16: bool,
}

fn hash_cursors(c: &[usize]) -> u64 {
    // FNV-1a over the cursor vector
    let mut h: u64 = 0xcbf29ce484222325;
    for &x in c {
        for byte in (x as u64).to_le_bytes() {
            h ^= byte as u64;
            h = h.wrapping_mul(0x100000001b3);
        }
    }
    h
}

impl<'a> MultiSearch<'a> {
    fn candidates(&self, cur: &[usize]) -> Vec<FieldType> {
        let b = self.b;
        let all = |t: FieldType| cur.iter().zip(&self.ends).all(|(&c, &e)| step(b, c, t).map(|n| n <= e).unwrap_or(false));
        let any = |f: &dyn Fn(usize) -> bool| cur.iter().any(|&c| f(c));
        let mut v = Vec::new();
        let str_ok = all(FieldType::Str);
        let str_nonempty = str_ok && any(&|c| str_end(b, c).map(|e| e > c + 2).unwrap_or(false));
        let ostr_ok = all(FieldType::OptStr);
        let ostr_some = ostr_ok && any(&|c| b.get(c) == Some(&1));
        if str_nonempty {
            v.push(FieldType::Str);
        }
        if ostr_some {
            v.push(FieldType::OptStr);
        }
        if all(FieldType::N32) && n32_plausible(b, cur) {
            v.push(FieldType::N32);
        }
        if all(FieldType::Bool) {
            v.push(FieldType::Bool);
        }
        if str_ok && !str_nonempty {
            v.push(FieldType::Str);
        }
        if ostr_ok && !ostr_some {
            v.push(FieldType::OptStr);
        }
        if self.allow_i16 && all(FieldType::I16) {
            v.push(FieldType::I16);
        }
        v
    }

    fn dfs(&mut self, schema: &mut Vec<FieldType>, cur: &mut Vec<usize>) -> bool {
        self.nodes += 1;
        if self.nodes > self.max_nodes {
            return false;
        }
        let done = cur.iter().zip(&self.ends).filter(|(c, e)| c == e).count();
        if done == cur.len() {
            return !schema.is_empty();
        }
        if done > 0 || schema.len() >= 300 {
            return false;
        }
        let key = hash_cursors(cur);
        if self.dead.contains(&key) {
            return false;
        }
        for t in self.candidates(cur) {
            let saved = cur.clone();
            for c in cur.iter_mut() {
                *c = step(self.b, *c, t).unwrap();
            }
            schema.push(t);
            if self.dfs(schema, cur) {
                return true;
            }
            schema.pop();
            *cur = saved;
        }
        self.dead.insert(key);
        false
    }
}

/// Exact multi-row search given known row starts (all rows advance in lock-step
/// and must end exactly at the next row's start / EOF).
pub fn infer_with_starts(b: &[u8], starts: &[usize], allow_i16: bool, max_nodes: u64) -> (Option<Vec<FieldType>>, u64) {
    let mut ends: Vec<usize> = starts[1..].to_vec();
    ends.push(b.len());
    let mut s = MultiSearch { b, ends, dead: HashSet::new(), nodes: 0, max_nodes, allow_i16 };
    let mut sch = vec![];
    let mut cur = starts.to_vec();
    let ok = s.dfs(&mut sch, &mut cur);
    (ok.then_some(sch), s.nodes)
}

/// Given known row starts `starts` (rows 0..m-1 closed: each must end at the next
/// start; the last one is "open"), return every position at which the open row can
/// end under *some* common schema. Exact reachability over cursor tuples.
fn reachable_ends(b: &[u8], starts: &[usize], allow_i16: bool, budget: &mut u64) -> Vec<usize> {
    let m = starts.len();
    let closed_ends = &starts[1..];
    let mut visited: HashSet<u64> = HashSet::new();
    let mut out: Vec<usize> = Vec::new();
    let mut stack: Vec<Vec<usize>> = vec![starts.to_vec()];
    let types: &[FieldType] = if allow_i16 {
        &[FieldType::Str, FieldType::OptStr, FieldType::N32, FieldType::Bool, FieldType::I16]
    } else {
        &[FieldType::Str, FieldType::OptStr, FieldType::N32, FieldType::Bool]
    };
    while let Some(cur) = stack.pop() {
        if *budget == 0 {
            break;
        }
        *budget -= 1;
        let fin = (0..m - 1).filter(|&i| cur[i] == closed_ends[i]).count();
        if fin == m - 1 && m > 1 {
            if !out.contains(&cur[m - 1]) {
                out.push(cur[m - 1]);
            }
            continue;
        }
        if fin > 0 {
            continue;
        }
        'types: for &t in types {
            if t == FieldType::N32 && !n32_plausible(b, &cur) {
                continue;
            }
            let mut next = Vec::with_capacity(m);
            for (i, &c) in cur.iter().enumerate() {
                match step(b, c, t) {
                    Some(n) if i == m - 1 || n <= closed_ends[i] => next.push(n),
                    _ => continue 'types,
                }
            }
            // an empty Str is the same width as I16 etc.; dedupe via visited
            if visited.insert(hash_cursors(&next)) {
                stack.push(next);
            }
        }
    }
    out.sort();
    out
}

/// Incremental row-boundary discovery (see `reachable_ends`).
fn chain(b: &[u8], h: &DbHeader, starts: &mut Vec<usize>, allow_i16: bool, budget: &mut u64) -> Option<Vec<FieldType>> {
    let m = starts.len();
    // With >= 3 closed rows, try to solve and validate directly.
    if m >= 4 {
        let closed = &starts[..m];
        let ends: Vec<usize> = starts[1..m].to_vec();
        // exact lock-step on closed rows only
        let mut s = MultiSearch { b, ends, dead: HashSet::new(), nodes: 0, max_nodes: 2_000_000, allow_i16 };
        let mut sch = vec![];
        let mut cur = closed[..m - 1].to_vec();
        if s.dfs(&mut sch, &mut cur) && validate(b, h, &sch) {
            return Some(sch);
        }
    }
    if *budget == 0 {
        return None;
    }
    let ends = reachable_ends(b, starts, allow_i16, budget);
    for e in ends {
        if m == h.rows as usize {
            if e == b.len() {
                let (sch, _) = infer_with_starts(b, starts, allow_i16, 5_000_000);
                if let Some(sch) = sch {
                    if validate(b, h, &sch) {
                        return Some(sch);
                    }
                }
            }
        } else if e < b.len() {
            starts.push(e);
            if let Some(s) = chain(b, h, starts, allow_i16, budget) {
                return Some(s);
            }
            starts.pop();
        }
        if *budget == 0 {
            return None;
        }
    }
    None
}

/// Schema inference by incremental row-boundary discovery. Tries row-1 starts in
/// increasing order.
pub fn infer_chain(b: &[u8], budget_total: u64) -> Option<InferResult> {
    let h = read_header(b)?;
    if h.rows < 2 {
        return None;
    }
    let max_s1 = b.len().saturating_sub(h.rows as usize - 1);
    let mut used = 0;
    for allow_i16 in [false, true] {
        if allow_i16 && NO_I16.load(std::sync::atomic::Ordering::Relaxed) {
            continue;
        }
        let mut budget = budget_total;
        for s1 in h.data_start + 1..=max_s1 {
            let mut starts = vec![h.data_start, s1];
            // row 1 must at least admit some first field consistent with row 0
            if !candidates(b, s1, allow_i16).iter().any(|&t| step(b, h.data_start, t).is_some()) {
                continue;
            }
            if let Some(s) = chain(b, &h, &mut starts, allow_i16, &mut budget) {
                return Some(InferResult { header: h, schema: Some(s), nodes: budget_total - budget + used });
            }
            if budget == 0 {
                break;
            }
        }
        used += budget_total - budget;
    }
    Some(InferResult { header: h, schema: None, nodes: used })
}

/// Infer a schema. First tries signature-based row starts + exact lock-step over
/// all rows; falls back to a 2-row lock-step search over every plausible row-1 start.
pub fn infer(b: &[u8], max_nodes_per_start: u64) -> Option<InferResult> {
    let h = read_header(b)?;
    let mut total = 0;
    if h.rows == 0 {
        return Some(InferResult { header: h, schema: Some(vec![]), nodes: 0 });
    }
    if let Some(starts) = row_starts_by_signature(b, &h) {
        for allow_i16 in [false, true] {
        if allow_i16 && NO_I16.load(std::sync::atomic::Ordering::Relaxed) {
            continue;
        }
            let (sch, n) = infer_with_starts(b, &starts, allow_i16, 5_000_000);
            total += n;
            if let Some(sch) = sch {
                if validate(b, &h, &sch) {
                    return Some(InferResult { header: h, schema: Some(sch), nodes: total });
                }
            }
        }
    }
    if h.rows >= 2 {
        if let Some(r) = infer_chain(b, 30_000_000) {
            total += r.nodes;
            if r.schema.is_some() {
                return Some(InferResult { nodes: total, ..r });
            }
        }
    }
    for allow_i16 in [false, true] {
        if allow_i16 && NO_I16.load(std::sync::atomic::Ordering::Relaxed) {
            continue;
        }
        if h.rows == 1 {
            // Single row: row "1" start is EOF.
            let mut s = Search { b, h, s1: b.len(), dead: HashSet::new(), nodes: 0, max_nodes: max_nodes_per_start * 20, max_cols: 250, allow_i16 };
            let mut sch = vec![];
            let (ok, _) = s.dfs(&mut sch, h.data_start, h.data_start);
            total += s.nodes;
            if ok {
                return Some(InferResult { header: h, schema: Some(sch), nodes: total });
            }
            continue;
        }
        // Upper bound on row 0 size: total payload minus (rows-1) minimal rows of 1 byte.
        let max_s1 = b.len().saturating_sub(h.rows as usize - 1);
        let first = candidates(b, h.data_start, allow_i16);
        for s1 in h.data_start + 1..=max_s1 {
            if total > 40_000_000 {
                break;
            }
            // cheap filter: the first field type must be acceptable at both starts
            if !first.iter().any(|&t| step(b, s1, t).is_some()) {
                continue;
            }
            // if row 0 starts with a non-empty key string, row 1 must too
            if first.first() == Some(&FieldType::Str) && str_end(b, h.data_start).unwrap() > h.data_start + 2 {
                match str_end(b, s1) {
                    Some(e) if e > s1 + 2 => {}
                    _ => continue,
                }
            }
            let mut s = Search { b, h, s1, dead: HashSet::new(), nodes: 0, max_nodes: max_nodes_per_start, max_cols: 250, allow_i16 };
            let mut sch = vec![];
            let (ok, _) = s.dfs(&mut sch, h.data_start, s1);
            total += s.nodes;
            if ok {
                return Some(InferResult { header: h, schema: Some(sch), nodes: total });
            }
        }
    }
    Some(InferResult { header: h, schema: None, nodes: total })
}

/// Decode all rows with a (possibly un-classified) schema; N32 columns are
/// classified as I32 or F32 from their values.
pub fn decode(b: &[u8], schema: &[FieldType]) -> Option<(Vec<FieldType>, Vec<Vec<Value>>)> {
    let refined = refine_bool_runs(b, schema)?;
    decode_exact(b, &refined)
}

/// A 4-byte column whose bytes are all 0/1 in every row, with byte 1..3 set in some
/// row (values like 0x00010100), is really 4 consecutive bools. Split it.
pub fn refine_bool_runs(b: &[u8], schema: &[FieldType]) -> Option<Vec<FieldType>> {
    let h = read_header(b)?;
    let mut all01 = vec![true; schema.len()];
    let mut hi_set = vec![false; schema.len()];
    let mut p = h.data_start;
    for _ in 0..h.rows {
        for (ci, &t) in schema.iter().enumerate() {
            if t == FieldType::N32 {
                let r = b.get(p..p + 4)?;
                all01[ci] &= r.iter().all(|&x| x <= 1);
                hi_set[ci] |= r[1..].iter().any(|&x| x == 1);
            }
            p = step(b, p, t)?;
        }
    }
    let mut out = Vec::with_capacity(schema.len());
    for (ci, &t) in schema.iter().enumerate() {
        if t == FieldType::N32 && all01[ci] && hi_set[ci] {
            out.extend([FieldType::Bool; 4]);
        } else {
            out.push(t);
        }
    }
    Some(out)
}

fn decode_exact(b: &[u8], schema: &[FieldType]) -> Option<(Vec<FieldType>, Vec<Vec<Value>>)> {
    let h = read_header(b)?;
    let mut p = h.data_start;
    let mut rows = Vec::with_capacity(h.rows as usize);
    for _ in 0..h.rows {
        let mut row = Vec::with_capacity(schema.len());
        for &t in schema {
            let v = match t {
                FieldType::Str => {
                    let (s, n) = read_str(b, p)?;
                    p = n;
                    Value::Str(s)
                }
                FieldType::OptStr => {
                    if *b.get(p)? == 0 {
                        p += 1;
                        Value::OptStr(None)
                    } else {
                        let (s, n) = read_str(b, p + 1)?;
                        p = n;
                        Value::OptStr(Some(s))
                    }
                }
                FieldType::Bool => {
                    let v = *b.get(p)? != 0;
                    p += 1;
                    Value::Bool(v)
                }
                FieldType::I16 => {
                    let v = i16::from_le_bytes(b.get(p..p + 2)?.try_into().ok()?);
                    p += 2;
                    Value::I16(v)
                }
                FieldType::N32 | FieldType::I32 | FieldType::F32 => {
                    let r: [u8; 4] = b.get(p..p + 4)?.try_into().ok()?;
                    p += 4;
                    match t {
                        FieldType::I32 => Value::I32(i32::from_le_bytes(r)),
                        FieldType::F32 => Value::F32(f32::from_le_bytes(r)),
                        _ => Value::Raw4(r),
                    }
                }
            };
            row.push(v);
        }
        rows.push(row);
    }
    // classify N32 columns
    let mut types = schema.to_vec();
    for (ci, t) in schema.iter().enumerate() {
        if *t != FieldType::N32 {
            continue;
        }
        let raws: Vec<[u8; 4]> = rows.iter().map(|r| if let Value::Raw4(x) = r[ci] { x } else { [0; 4] }).collect();
        let int_ok = raws.iter().all(|r| { let a = i32::from_le_bytes(*r).unsigned_abs(); a < 2_000_000 && !(a >= 65536 && a & 0xFFFF == 0) });
        let float_ok = raws.iter().all(|r| {
            let f = f32::from_le_bytes(*r);
            f.is_finite() && (f == 0.0 || (f.abs() > 1e-6 && f.abs() < 1e8))
        });
        let as_float = !int_ok && float_ok;
        types[ci] = if as_float { FieldType::F32 } else { FieldType::I32 };
        for (ri, r) in rows.iter_mut().enumerate() {
            r[ci] = if as_float { Value::F32(f32::from_le_bytes(raws[ri])) } else { Value::I32(i32::from_le_bytes(raws[ri])) };
        }
    }
    Some((types, rows))
}

/// Debug helper: how many row-start matches each signature length gives.
pub fn debug_signature_counts(b: &[u8], h: &DbHeader) -> String {
    let mut out = format!("rows={}\n", h.rows);
    for k in 1..=4usize {
        let mut n = 0;
        let mut p = h.data_start;
        let mut first = vec![];
        while p + 2 <= b.len() {
            let mut q = p;
            let mut ok = true;
            let mut fe = p;
            for i in 0..k {
                match str_end(b, q) {
                    Some(e) if e > q + 2 => {
                        if i == 0 { fe = e; }
                        q = e;
                    }
                    _ => { ok = false; break; }
                }
            }
            if ok {
                n += 1;
                if first.len() < 8 { first.push(read_str(b, p).unwrap().0); }
                p = fe;
            } else {
                p += 1;
            }
        }
        out += &format!("k={k}: {n} matches, first: {first:?}\n");
    }
    out
}

/// Upper bound for "plausible" integers. 2,000,000 first (excludes UTF-16 text such as
/// 0x00610061); `infer` retries with 10,000,000 if nothing is found.
pub static INT_LIMIT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(2_000_000);

/// `infer` with the adaptive integer bound.
pub fn infer_adaptive(b: &[u8], max_nodes_per_start: u64) -> Option<InferResult> {
    use std::sync::atomic::Ordering::Relaxed;
    let mut nodes = 0;
    // i16 is a last resort: first try every integer bound without it.
    for (no_i16, lim) in [(true, 2_000_000u32), (true, 10_000_000), (true, u32::MAX), (false, 2_000_000), (false, u32::MAX)] {
        NO_I16.store(no_i16, Relaxed);
        INT_LIMIT.store(lim, Relaxed);
        let r = infer(b, max_nodes_per_start)?;
        nodes += r.nodes;
        if r.schema.is_some() {
            NO_I16.store(false, Relaxed);
            INT_LIMIT.store(2_000_000, Relaxed);
            return Some(InferResult { nodes, ..r });
        }
    }
    NO_I16.store(false, Relaxed);
    INT_LIMIT.store(2_000_000, Relaxed);
    let r = infer(b, 1)?;
    Some(InferResult { nodes, schema: None, ..r })
}

/// When set, the i16 fallback passes are skipped.
pub static NO_I16: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Field layouts confirmed from the exe row readers (Worker 1, DB_BUILDERS.md), for
/// the latest shipped table versions. Used instead of inference where present.
/// Codes: s str, o optstr, b bool, n 4-byte (classified from data), i i32, f f32.
pub fn known_schema(table: &str) -> Option<&'static str> {
    Some(match table {
        "unit_stats_land" => "s,n,n,n,s,o,o,s,s,s,s,n,s,n,o,o,o,s,s,o,o,o,o,b,o,o,n,n,s,s,o,n,o,s,n,n,n,n,s,s,s,s,n,n,n,n,n,n,n,n,n,n,b,b,b,b,b,b,b,b,b,b,b,b,b,b,n,n,n,b,b,b,b,b,b,b,b,b,b,b,b,b,b,b,b,o,o,o,b",
        "units" => "s,s,s,s,n,n,n,n,n,n,o,s,s,s,o,n,s,b,b,b,n,b,n,o,b",
        "projectiles" => "s,s,s,s,o,o,o,s,n,o,s,n,n,n,n,n,n,n,o,o,o,b,b,n,n,n,o,n,n,s,s,o,o,o,o",
        "factions" => "s,n,s,s,s,s,s,s,o,b,b,b,s,s,o,o,n,n,n,n,n,n,n,n,n,n,n,n,n,n,n,n,n,n,s,o,n,n,n,o,s,b,b,s,s,s,s,o",
        "building_levels" => "s,s,n,s,n,n,n,n,n,n,n,n,n,n,n,n,s,s,n,b,n,n,n,n",
        // campaign tables: exe layouts (DB_BUILDERS.md), agreeing with Worker 3
        "campaign_variables" => "s,f",
        "ancillaries" => "s,s,s,b,b,b,n,n,n",
        "character_traits" => "s,n,b,n,s",
        "government_types" => "s,b,b,n,s,s",
        "cultures" => "s,n,o",
        "cultures_subcultures" => "s,s,n,s",
        "slots" => "s,b,b,b,b,b",
        "historical_characters" => "s,b,s,s,s,n,n,s",
        "fatigue_effects" => "s,s,n,n,n,n",
        _ => return None,
    })
}
