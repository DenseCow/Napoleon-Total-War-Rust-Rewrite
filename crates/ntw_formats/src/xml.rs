//! A minimal XML reader for the game's small settings files.
//!
//! The battle-map files (`definition.xml`, `deployment_areas.xml`, `*.environment`, ...) are
//! plain XML that mostly uses elements and attributes. Text directly inside an element is kept,
//! trimmed, in [`XmlElement::text`] (the historical battle files: `<faction>france</faction>`).
//! It handles: the `<?xml ...?>` declaration, comments, `<!...>`
//! declarations, self-closing tags, single or double quoted attribute values, the five standard
//! entities, and UTF-8 or UTF-16LE (with BOM) input.

use std::fmt;

/// One XML element: its name, attributes (in file order) and child elements.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct XmlElement {
    /// Tag name as written, e.g. `"BATTLE_MAP_DEFINITION"`.
    pub name: String,
    /// `(name, value)` pairs, entities decoded.
    pub attrs: Vec<(String, String)>,
    /// Child elements in file order.
    pub children: Vec<XmlElement>,
    /// Text directly inside this element (its text runs joined by a space, trimmed, entities
    /// decoded). Empty when there is none.
    pub text: String,
}

/// Why an XML file could not be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XmlError(pub String);

impl fmt::Display for XmlError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "XML: {}", self.0)
    }
}

impl std::error::Error for XmlError {}

impl XmlElement {
    /// An attribute's value (attribute names compared case-insensitively).
    pub fn attr(&self, name: &str) -> Option<&str> {
        self.attrs.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str())
    }

    /// An attribute parsed as `f32`.
    pub fn attr_f32(&self, name: &str) -> Option<f32> {
        self.attr(name)?.trim().parse().ok()
    }

    /// An attribute parsed as `i64`.
    pub fn attr_i64(&self, name: &str) -> Option<i64> {
        self.attr(name)?.trim().parse().ok()
    }

    /// An attribute read as a boolean (`true`/`false`/`1`/`0`).
    pub fn attr_bool(&self, name: &str) -> Option<bool> {
        match self.attr(name)?.trim().to_ascii_lowercase().as_str() {
            "true" | "1" => Some(true),
            "false" | "0" => Some(false),
            _ => None,
        }
    }

    /// The first child with this tag name (case-insensitive).
    pub fn child(&self, name: &str) -> Option<&XmlElement> {
        self.children.iter().find(|c| c.name.eq_ignore_ascii_case(name))
    }

    /// All children with this tag name (case-insensitive).
    pub fn children_named<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a XmlElement> + 'a {
        self.children.iter().filter(move |c| c.name.eq_ignore_ascii_case(name))
    }

    /// The first element with this name at any depth below this one (depth-first).
    pub fn find(&self, name: &str) -> Option<&XmlElement> {
        for c in &self.children {
            if c.name.eq_ignore_ascii_case(name) {
                return Some(c);
            }
            if let Some(f) = c.find(name) {
                return Some(f);
            }
        }
        None
    }
}

/// Decodes bytes to text: UTF-16LE if it starts with a BOM `FF FE`, else UTF-8 (lossy, BOM removed).
pub fn decode_text(bytes: &[u8]) -> String {
    if bytes.starts_with(&[0xFF, 0xFE]) {
        let units: Vec<u16> = bytes[2..].chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
        return String::from_utf16_lossy(&units);
    }
    let b = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes);
    String::from_utf8_lossy(b).into_owned()
}

fn unescape(s: &str) -> String {
    if !s.contains('&') {
        return s.to_owned();
    }
    s.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

/// Parses a document and returns a synthetic root element (name `""`) whose children are the
/// top-level elements. Most files have exactly one; see [`parse`] for that case.
pub fn parse_document(text: &str) -> Result<XmlElement, XmlError> {
    let s = text.as_bytes();
    let mut stack: Vec<XmlElement> = vec![XmlElement::default()];
    let mut i = 0;
    while i < s.len() {
        if s[i] != b'<' {
            // Text content up to the next '<' (ASCII, so always a char boundary).
            let end = text[i..].find('<').map_or(s.len(), |e| i + e);
            let run = text[i..end].trim();
            if !run.is_empty() && stack.len() > 1 {
                let el = stack.last_mut().expect("root");
                if !el.text.is_empty() {
                    el.text.push(' ');
                }
                el.text.push_str(&unescape(run));
            }
            i = end;
            continue;
        }
        let rest = &text[i..];
        if rest.starts_with("<!--") {
            i += rest.find("-->").ok_or_else(|| XmlError("unclosed comment".into()))? + 3;
        } else if rest.starts_with("<?") {
            i += rest.find("?>").ok_or_else(|| XmlError("unclosed <?".into()))? + 2;
        } else if rest.starts_with("<!") {
            i += rest.find('>').ok_or_else(|| XmlError("unclosed <!".into()))? + 1;
        } else if rest.starts_with("</") {
            let end = rest.find('>').ok_or_else(|| XmlError("unclosed end tag".into()))?;
            let name = rest[2..end].trim();
            let el = stack.pop().filter(|_| !stack.is_empty()).ok_or_else(|| XmlError(format!("stray </{name}>")))?;
            if !el.name.eq_ignore_ascii_case(name) {
                return Err(XmlError(format!("</{name}> closes <{}>", el.name)));
            }
            stack.last_mut().expect("root").children.push(el);
            i += end + 1;
        } else {
            // Start tag: name, then attributes, then `>` or `/>`. Quoted values may contain '>'.
            let mut j = i + 1;
            while j < s.len() && !s[j].is_ascii_whitespace() && s[j] != b'>' && s[j] != b'/' {
                j += 1;
            }
            let mut el = XmlElement { name: text[i + 1..j].to_owned(), ..Default::default() };
            let self_closing;
            loop {
                while j < s.len() && s[j].is_ascii_whitespace() {
                    j += 1;
                }
                match s.get(j) {
                    None => return Err(XmlError(format!("unclosed <{}>", el.name))),
                    Some(b'>') => {
                        self_closing = false;
                        j += 1;
                        break;
                    }
                    Some(b'/') if s.get(j + 1) == Some(&b'>') => {
                        self_closing = true;
                        j += 2;
                        break;
                    }
                    _ => {}
                }
                let k = j;
                while j < s.len() && s[j] != b'=' && !s[j].is_ascii_whitespace() && s[j] != b'>' {
                    j += 1;
                }
                let key = text[k..j].to_owned();
                while j < s.len() && (s[j].is_ascii_whitespace() || s[j] == b'=') {
                    j += 1;
                }
                let quote = *s.get(j).ok_or_else(|| XmlError("attribute without value".into()))?;
                if quote != b'"' && quote != b'\'' {
                    return Err(XmlError(format!("unquoted value for {key}")));
                }
                let close = text[j + 1..]
                    .find(quote as char)
                    .ok_or_else(|| XmlError("unclosed attribute value".into()))?;
                el.attrs.push((key, unescape(&text[j + 1..j + 1 + close])));
                j += close + 2;
            }
            if self_closing {
                stack.last_mut().expect("root").children.push(el);
            } else {
                stack.push(el);
            }
            i = j;
        }
    }
    // Be lenient with unclosed elements at end of file: close them.
    while stack.len() > 1 {
        let el = stack.pop().expect("len > 1");
        stack.last_mut().expect("root").children.push(el);
    }
    Ok(stack.pop().expect("root"))
}

/// Parses a document that has one top-level element and returns that element.
pub fn parse(text: &str) -> Result<XmlElement, XmlError> {
    let mut doc = parse_document(text)?;
    if doc.children.is_empty() {
        return Err(XmlError("no element".into()));
    }
    Ok(doc.children.swap_remove(0))
}

/// [`decode_text`] then [`parse`].
pub fn parse_bytes(bytes: &[u8]) -> Result<XmlElement, XmlError> {
    parse(&decode_text(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nested_attrs_comments() {
        let x = parse(
            "<?xml version='1.0'?>\n<A k='1' b=\"x&amp;y\">\n<!-- c > d --><B v='2.5'/>text<C><D/></C></A>",
        )
        .unwrap();
        assert_eq!(x.name, "A");
        assert_eq!(x.attr("K"), Some("1"));
        assert_eq!(x.attr("b"), Some("x&y"));
        assert_eq!(x.child("b").unwrap().attr_f32("v"), Some(2.5));
        assert!(x.find("D").is_some());
        assert_eq!(x.children.len(), 2);
        assert_eq!(x.text, "text");
        assert_eq!(parse("<F> france &amp; co </F>").unwrap().text, "france & co");
    }

    #[test]
    fn mismatched_end_is_error() {
        assert!(parse("<A><B></A>").is_err());
    }

    #[test]
    fn utf16_bom() {
        let mut b = vec![0xFF, 0xFE];
        for u in "<X a='é'/>".encode_utf16() {
            b.extend(u.to_le_bytes());
        }
        assert_eq!(parse_bytes(&b).unwrap().attr("a"), Some("é"));
    }
}
