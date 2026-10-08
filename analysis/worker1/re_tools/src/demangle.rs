//! Best-effort demangler for MSVC RTTI type-descriptor names (".?AVFoo@Bar@@").
//! Handles nested namespaces, templates, simple types, integer template args and back-references.
//! Falls back to None on anything it does not understand.

struct P<'a> { s: &'a [u8], i: usize }

impl<'a> P<'a> {
    fn peek(&self) -> Option<u8> { self.s.get(self.i).copied() }
    fn eat(&mut self, c: u8) -> bool { if self.peek() == Some(c) { self.i += 1; true } else { false } }
    fn ident(&mut self) -> Option<String> {
        let st = self.i;
        while let Some(c) = self.peek() { if c == b'@' { break; } self.i += 1; }
        let r = String::from_utf8_lossy(&self.s[st..self.i]).to_string();
        if !self.eat(b'@') { return None; }
        Some(r)
    }
    fn number(&mut self) -> Option<i64> {
        let neg = self.eat(b'?');
        let c = self.peek()?;
        let v = if c.is_ascii_digit() { self.i += 1; (c - b'0') as i64 + 1 } else {
            let mut v: i64 = 0;
            loop { let c = self.peek()?; self.i += 1; if c == b'@' { break; } if !(b'A'..=b'P').contains(&c) { return None; } v = v * 16 + (c - b'A') as i64; }
            v
        };
        Some(if neg { -v } else { v })
    }
    /// one name fragment
    fn fragment(&mut self, names: &mut Vec<String>) -> Option<String> {
        let c = self.peek()?;
        if c.is_ascii_digit() { self.i += 1; return names.get((c - b'0') as usize).cloned(); }
        if self.s[self.i..].starts_with(b"?$") {
            self.i += 2;
            let tname = self.ident()?;
            let mut inner_names = vec![tname.clone()];
            let mut tbr: Vec<String> = vec![];
            let mut args = vec![];
            while !self.eat(b'@') {
                let a = self.typ(&mut inner_names, &mut tbr)?;
                if !a.is_empty() { args.push(a); }
            }
            let full = format!("{}<{}>", tname, args.join(","));
            if names.len() < 10 { names.push(full.clone()); }
            return Some(full);
        }
        if self.s[self.i..].starts_with(b"?A") {
            // anonymous namespace ?A0x1234abcd@
            self.i += 1;
            let _ = self.ident()?;
            let r = "`anonymous namespace'".to_string();
            if names.len() < 10 { names.push(r.clone()); }
            return Some(r);
        }
        if c == b'?' { return None; }
        let id = self.ident()?;
        if names.len() < 10 { names.push(id.clone()); }
        Some(id)
    }
    /// qualified name: fragments until terminating '@'. Returns "a::b::c" (outer first).
    fn qualified(&mut self, names: &mut Vec<String>) -> Option<String> {
        let mut parts = vec![];
        loop {
            if self.eat(b'@') { break; }
            parts.push(self.fragment(names)?);
            if parts.len() > 64 { return None; }
        }
        parts.reverse();
        Some(parts.join("::"))
    }
    fn typ(&mut self, names: &mut Vec<String>, tbr: &mut Vec<String>) -> Option<String> {
        let c = self.peek()?;
        let simple = match c {
            b'C' => Some("signed char"), b'D' => Some("char"), b'E' => Some("unsigned char"), b'F' => Some("short"),
            b'G' => Some("unsigned short"), b'H' => Some("int"), b'I' => Some("unsigned int"), b'J' => Some("long"),
            b'K' => Some("unsigned long"), b'M' => Some("float"), b'N' => Some("double"), b'O' => Some("long double"),
            b'X' => Some("void"), _ => None,
        };
        if let Some(s) = simple { self.i += 1; return Some(s.into()); }
        if c.is_ascii_digit() { self.i += 1; return tbr.get((c - b'0') as usize).cloned(); }
        let start_len = self.i;
        let r = match c {
            b'_' => {
                self.i += 1;
                let t = self.peek()?; self.i += 1;
                match t { b'N' => "bool".into(), b'J' => "__int64".into(), b'K' => "unsigned __int64".into(), b'W' => "wchar_t".into(), _ => return None }
            }
            b'V' | b'U' => { self.i += 1; self.qualified(names)? }
            b'W' => { self.i += 2; format!("enum {}", self.qualified(names)?) }
            b'P' | b'Q' => {
                self.i += 1; let cv = self.peek()?; self.i += 1;
                if cv == b'6' { return None; } // function pointer - give up
                let inner = self.typ(names, tbr)?;
                format!("{}{}*", if cv == b'B' { "const " } else { "" }, inner)
            }
            b'A' => { self.i += 1; let cv = self.peek()?; self.i += 1; let inner = self.typ(names, tbr)?; format!("{}{}&", if cv == b'B' { "const " } else { "" }, inner) }
            b'$' => {
                self.i += 1;
                let t = self.peek()?; self.i += 1;
                match t {
                    b'0' => { return self.number().map(|n| n.to_string()); }
                    b'$' => { let u = self.peek()?; self.i += 1; match u { b'V' | b'Z' => return Some(String::new()), b'Q' => { self.i += 1; let inner = self.typ(names, tbr)?; format!("{}&&", inner) } b'C' => { let _cv = self.peek()?; self.i += 1; self.typ(names, tbr)? } _ => return None } }
                    _ => return None,
                }
            }
            _ => return None,
        };
        let consumed = self.i - start_len;
        if consumed > 1 && tbr.len() < 10 { tbr.push(r.clone()); }
        Some(r)
    }
}

/// ".?AVUNIT@BATTLE@@" -> "BATTLE::UNIT"
pub fn demangle_td(raw: &str) -> Option<String> {
    let b = raw.as_bytes();
    if b.len() < 5 || !(raw.starts_with(".?AV") || raw.starts_with(".?AU") || raw.starts_with(".?AW")) { return None; }
    let mut p = P { s: &b[4..], i: 0 };
    if raw.starts_with(".?AW") { p.i += 1; }
    let mut names = vec![];
    let r = p.qualified(&mut names)?;
    if p.i != p.s.len() { return None; }
    Some(r)
}

#[cfg(test)]
mod t {
    use super::*;
    #[test]
    fn basic() {
        assert_eq!(demangle_td(".?AVUNIT@BATTLE@@").unwrap(), "BATTLE::UNIT");
        assert_eq!(demangle_td(".?AVtype_info@@").unwrap(), "type_info");
        assert_eq!(demangle_td(".?AV?$basic_ostream@DU?$char_traits@D@std@@@std@@").unwrap(), "std::basic_ostream<char,std::char_traits<char>>");
    }
}
