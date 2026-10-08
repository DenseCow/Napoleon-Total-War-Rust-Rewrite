//! Research probe: find binary-layout components whose id also appears in
//! `ui/templates/post_battle_entry.twui` and print their UNKNOWN fields, to name them from the
//! .twui (an editor text format with field names). Read-only.
use ntw_formats::pack::Vfs;
use ntw_formats::ui_layout::{UiComponent, UiLayout};

const DATA: &str = r"C:\Program Files (x86)\Steam\steamapps\common\Napoleon Total War\data";

fn walk(c: &UiComponent, ids: &[&str], path: &str) {
    if ids.contains(&c.id.as_str()) {
        println!("{path}: {} this {} da {} e5 {} 140 {} default_state {}", c.id, c.this, c.unknown_da, c.unknown_e5, c.unknown_140, c.default_state);
        for s in &c.states {
            println!("   state {} this {} d4 {} 60 {} 64 {} d0 {} align {:?} behaviour {:?} size {}x{} editor_pos {:?}", s.name, s.this, s.unknown_d4, s.text_x_offset, s.text_y_offset, s.unknown_d0, s.text_align, s.text_behaviour, s.width, s.height, s.editor_pos);
        }
    }
    for ch in &c.children {
        walk(ch, ids, path);
    }
}

fn main() {
    let vfs = Vfs::open_install(std::env::var("NTW_DATA_DIR").unwrap_or_else(|_| DATA.into())).unwrap();
    let ids = ["killed_dy", "deployed_dy", "lost_dy", "experience_old", "arrow", "post_battle_entry"];
    let mut h = std::collections::BTreeMap::new();
    for p in vfs.list("ui") {
        if p.contains('.') {
            continue;
        }
        let Ok(b) = vfs.read(p) else { continue };
        if !b.starts_with(b"Version") {
            continue;
        }
        if let Ok(l) = UiLayout::read(&b) {
            if std::env::args().nth(1).as_deref() == Some("stats") { stats(&l.root, &mut h, l.version); continue; }
            if std::env::args().nth(1).as_deref() == Some("flagged") { let mut o = Vec::new(); flagged(&l.root, p, &mut o); for x in o { println!("{x}"); } continue; }
            walk(&l.root, &ids, p);
        }
    }
    for (k, v) in &h {
        let top: Vec<_> = v.iter().take(12).collect();
        println!("{k}: {} distinct, {top:?}", v.len());
    }
}

#[allow(dead_code)]
pub fn stats(c: &UiComponent, h: &mut std::collections::BTreeMap<String, std::collections::BTreeMap<i64, usize>>, v: u32) {
    let mut add = |k: &str, x: i64| *h.entry(format!("v{v} {k}")).or_default().entry(x).or_default() += 1;
    add("da", c.unknown_da as i64);
    add("e5", c.unknown_e5 as i64);
    add("140", c.unknown_140 as i64);
    for s in &c.states {
        add("d4", s.unknown_d4 as i64);
        add("60", s.text_x_offset as i64);
        add("64", s.text_y_offset as i64);
        add("d0", s.unknown_d0 as i64);
        add("f0x", s.editor_pos.0 as i64);
        add("f0y", s.editor_pos.1 as i64);
        add("valign", s.text_align.1 as i64);
        add("halign", s.text_align.0 as i64);
    }
    for ch in &c.children {
        stats(ch, h, v);
    }
}


#[allow(dead_code)]
pub fn flagged(c: &UiComponent, path: &str, out: &mut Vec<String>) {
    if c.unknown_da != 0 {
        out.push(format!("da=1 {path}: {} ({} children)", c.id, c.children.len()));
    }
    if c.unknown_140 != 0 {
        out.push(format!("140={} {path}: {}", c.unknown_140, c.id));
    }
    for s in &c.states {
        if s.unknown_d4 != 0 || s.text_x_offset != 0 || s.text_y_offset != 0 {
            out.push(format!("state {path}: {}/{} d4 {} 60 {} 64 {} font {:?} text {:?}", c.id, s.name, s.unknown_d4, s.text_x_offset, s.text_y_offset, s.font, s.text.chars().take(20).collect::<String>()));
        }
    }
    for ch in &c.children {
        flagged(ch, path, out);
    }
}
