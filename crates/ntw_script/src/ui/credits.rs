//! `FrontEnd.BuildCredits(parent)`: the credits pages from `text/credits.xml` (`0x0046B100`, line
//! builder `0x004635B0`; CONFIRMED unless tagged). Evidence: `analysis/frontend/FRONTEND_PAGES.md`
//! "Credits".
//!
//! - The file is `<credits>` with `<page [delay]>` elements of `<line gap fontsize colour>`. A line
//!   holds its own text (one centred string) and/or `<left indent fontsize>` / `<right indent
//!   fontsize>` parts (two columns).
//! - Per page, a container is created under `parent` (as wide as the parent). Per line part, a
//!   `string` component from the UIEd template library is placed at the running y. The line takes
//!   the height of its tallest part, then its `gap`. The container is then sized to the page
//!   height.
//! - `fontsize` picks the font through the table at `0x0144F030`: 12, 14, 16, 18, 22, 24, 38 → font
//!   ids 12..18. Our font is Frontend at that size (INFERRED: 38 ships only for Frontend).
//! - `colour` is hex RRGGBB with alpha 0xFF; without it the string is white.
//! - `indent` is the part's distance from its side of the page (default 0).
//! - Returns `{ {Page = container, Height, WordCount = characters of the page's texts, Delay = the
//!   page's delay, 0 without} , ... }`. credits.lua shows one page at a time, centred, and fades
//!   it out after 4 s.

use std::rc::Rc;

use mlua::{Lua, Table};
use ntw_formats::ui_layout::{UiComponent, UiState, ALIGN_CENTRE, ALIGN_LEFT, ALIGN_RIGHT, ALIGN_TOP};
use ntw_formats::xml::XmlElement;

use super::host::{Inner, addr, log, template_library};
use super::world::NodeId;

/// The credits font family (INFERRED: size 38 ships only as `font/frontend_38.cuf`).
pub const CREDITS_FAMILY: &str = "Frontend";

/// `0x0144F030`: credits font sizes → font ids (CONFIRMED table).
pub const FONT_IDS: [(u32, u32); 7] = [(12, 12), (14, 13), (16, 14), (18, 15), (22, 16), (24, 17), (38, 18)];

/// One string of a credits line, laid out.
#[derive(Debug, Clone, PartialEq)]
pub struct CreditString {
    pub text: String,
    /// Font size from the line or part (`fontsize`), if given.
    pub size: Option<u32>,
    /// 0xAARRGGBB from `colour`, if given.
    pub colour: Option<u32>,
    /// `indent` (default 0).
    pub indent: i32,
    /// Horizontal alignment: left part, right part or the line's own (centred) text.
    pub align: i32,
}

/// A credits line: its strings and the gap after it.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct CreditLine {
    pub strings: Vec<CreditString>,
    pub gap: i32,
}

/// A credits page.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct CreditPage {
    pub lines: Vec<CreditLine>,
    /// `delay` attribute (0 when absent).
    pub delay: f32,
}

fn part(e: &XmlElement, text: &str, align: i32, line: &XmlElement) -> CreditString {
    let attr = |n: &str| e.attr(n).or_else(|| line.attr(n));
    CreditString {
        text: text.to_owned(),
        size: attr("fontsize").and_then(|v| v.trim().parse().ok()),
        colour: attr("colour").and_then(|v| u32::from_str_radix(v.trim(), 16).ok()).map(|rgb| 0xFF00_0000 | (rgb & 0x00FF_FFFF)),
        indent: e.attr_i64("indent").unwrap_or(0) as i32,
        align,
    }
}

/// Reads `text/credits.xml`.
pub fn parse_credits(bytes: &[u8]) -> Result<Vec<CreditPage>, String> {
    let doc = ntw_formats::xml::parse_document(&ntw_formats::xml::decode_text(bytes)).map_err(|e| e.0)?;
    let root = doc.find("credits").ok_or("no <credits>")?;
    Ok(root
        .children_named("page")
        .map(|p| CreditPage {
            delay: p.attr_f32("delay").unwrap_or(0.0),
            lines: p
                .children_named("line")
                .map(|l| {
                    let mut strings = Vec::new();
                    if let Some(e) = l.child("left") {
                        strings.push(part(e, &e.text, ALIGN_LEFT, l));
                    }
                    if let Some(e) = l.child("right") {
                        strings.push(part(e, &e.text, ALIGN_RIGHT, l));
                    }
                    if !l.text.is_empty() {
                        strings.push(part(l, &l.text, ALIGN_CENTRE, l));
                    }
                    CreditLine { strings, gap: l.attr_i64("gap").unwrap_or(0) as i32 }
                })
                .collect(),
        })
        .collect())
}

/// Builds the pages under `parent` and returns the script's table.
pub(super) fn build_credits(lua: &Lua, inner: &Rc<Inner>, parent: NodeId) -> mlua::Result<Table> {
    let out = lua.create_table()?;
    let pages = match inner.source.find("text/credits.xml").ok_or_else(|| "no text/credits.xml".to_owned()).and_then(|f| parse_credits(&f.bytes)) {
        Ok(p) => p,
        Err(e) => {
            log(inner, format!("BuildCredits: {e}"));
            return Ok(out);
        }
    };
    let Some(string) = template_library(inner).and_then(|lib| lib.get("string").map(|t| t.component.clone())) else {
        log(inner, "BuildCredits: no `string` template".into());
        return Ok(out);
    };
    let base_state = string.initial_state().cloned().unwrap_or_default();
    super::host::lay_out_if_stale(inner);
    let page_w = inner.world.borrow().get(parent).map_or(0.0, |n| n.rect.w) as i32;
    let mut all_created = Vec::new();
    for (i, page) in pages.iter().enumerate() {
        // The page container: an empty component as wide as the parent (INFERRED: the library
        // has no `container` template; the exe's library call creates a plain component).
        let container = UiComponent {
            id: format!("credits_page{}", i + 1),
            visible: true,
            states: vec![UiState { this: 1, name: "NewState".into(), width: page_w, height: 0, ..Default::default() }],
            ..Default::default()
        };
        let page_id = inner.world.borrow_mut().instantiate(&container, Some(parent), "text/credits.xml", &mut all_created);
        let mut y = 0;
        let mut chars = 0usize;
        for line in &page.lines {
            let mut tallest = 0;
            for s in &line.strings {
                chars += s.text.encode_utf16().count();
                let mut st = base_state.clone();
                let font = match s.size {
                    Some(size) if FONT_IDS.iter().any(|(sz, _)| *sz == size) => format!("{CREDITS_FAMILY} {size}, Normal"),
                    _ => base_state.font.clone(),
                };
                let width = (page_w - 2 * s.indent).max(1);
                let (_, h, _) = super::host::text_extent(inner, &font, &s.text, width);
                st.text = s.text.clone();
                st.text_localised = false;
                st.text_label.clear();
                st.font = font;
                st.font_index = None;
                // Without `colour` the string is white (the builder starts from -1, CONFIRMED).
                st.font_colour = s.colour.unwrap_or(0xFFFF_FFFF);
                st.text_align = (s.align, ALIGN_TOP);
                st.text_x_offset = 0;
                st.text_y_offset = 0;
                st.width = width;
                st.height = h.ceil() as i32;
                let comp = UiComponent { offset: (s.indent, y), visible: true, states: vec![st], children: Vec::new(), ..string.clone() };
                inner.world.borrow_mut().instantiate(&comp, Some(page_id), "ui/templates/uied.templates", &mut all_created);
                tallest = tallest.max(h.ceil() as i32);
            }
            y += tallest + line.gap;
        }
        if let Some(n) = inner.world.borrow_mut().get_mut(page_id) {
            n.size_override = Some((page_w as f32, y as f32));
            n.visible = false;
        }
        let e = lua.create_table()?;
        e.set("Page", addr(page_id))?;
        e.set("Height", y)?;
        e.set("WordCount", chars)?;
        e.set("Delay", page.delay)?;
        out.set(i + 1, e)?;
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn credits_lines_and_parts() {
        let xml = "<credits>\r<page>\r<line fontsize=\"22\" gap=\"0\" colour=\"D5C770\">Studio Director</line>\r<line gap=\"8\" fontsize=\"18\">Tim Heaton\r</line>\r<line gap=\"4\">\r<left fontsize=\"18\" indent=\"30\">A</left>\r<right fontsize=\"18\" indent=\"30\">B</right>\r</line>\r</page>\r<page delay=\"2.5\"></page></credits>";
        let pages = parse_credits(xml.as_bytes()).unwrap();
        assert_eq!(pages.len(), 2);
        let l = &pages[0].lines;
        assert_eq!(l[0].strings[0], CreditString { text: "Studio Director".into(), size: Some(22), colour: Some(0xFFD5C770), indent: 0, align: ALIGN_CENTRE });
        assert_eq!((l[1].strings[0].text.as_str(), l[1].gap), ("Tim Heaton", 8));
        assert_eq!(l[2].strings.iter().map(|s| (s.align, s.indent)).collect::<Vec<_>>(), vec![(ALIGN_LEFT, 30), (ALIGN_RIGHT, 30)]);
        assert_eq!(pages[1].delay, 2.5);
    }
}
