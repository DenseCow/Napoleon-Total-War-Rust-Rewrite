//! Research probe: the raw framing of `uied.templates` entries (read-only).
fn main() {
    let path = r"C:\Program Files (x86)\Steam\steamapps\common\Napoleon Total War\data\UI\Templates\uied.templates";
    let b = std::fs::read(path).unwrap();
    let u32_at = |p: usize| u32::from_le_bytes(b[p..p + 4].try_into().unwrap());
    let n = u32_at(0) as usize;
    let mut p = 4;
    for i in 0..n {
        let block = &b[p..p + 256];
        let nl = block.iter().position(|&c| c == 0).unwrap_or(256);
        let name = String::from_utf8_lossy(&block[..nl]);
        let size = u32_at(p + 256) as usize;
        let unk = u32_at(p + 260);
        let (v1, v2) = (u32_at(p + 248), u32_at(p + 252));
        if i < 4 || i + 6 > n || name == "InputWindow" || name == "BattleEditor" {
            println!("{i:3} {name:<28} size {size:7} unk {unk:#010x} ({unk}) +248 {v1:#x} +252 {v2:#x} payload starts {:02x?}", &b[p + 264..p + 264 + 24]);
        }
        p += 264 + size;
    }
    println!("end {p} of {}", b.len());
    if std::env::args().nth(1).as_deref() == Some("input") { dump(&b, 4 + 264, 674); }
    if std::env::args().nth(1).as_deref() == Some("battle") { for v in 1..=8 { try_entry(&b, 1, v); } let s: usize = std::env::args().nth(2).and_then(|s| s.parse().ok()).unwrap_or(0); if s > 0 { dump(&b, 4 + 264 + 674 + 264 + s - 1, 400); } }
}

#[allow(dead_code)]
pub fn dump(b: &[u8], from: usize, len: usize) {
    for (k, row) in b[from..from + len].chunks(16).enumerate() {
        let hex: Vec<String> = row.iter().map(|x| format!("{x:02x}")).collect();
        let asc: String = row.iter().map(|&x| if (32..127).contains(&x) { x as char } else { '.' }).collect();
        println!("{:6x}  {:<48} {asc}", k * 16, hex.join(" "));
    }
}

#[allow(dead_code)]
pub fn try_entry(b: &[u8], index: usize, version: u32) {
    let u32_at = |p: usize| u32::from_le_bytes(b[p..p + 4].try_into().unwrap());
    let mut p = 4;
    for _ in 0..index {
        p += 264 + u32_at(p + 256) as usize;
    }
    let size = u32_at(p + 256) as usize;
    let payload = &b[p + 264..p + 264 + size];
    let n = u32_at(p + 264) as usize;
    assert_eq!(n, 0, "images before the component not handled here");
    match ntw_formats::ui_layout::read_template_component(&payload[4..], version) {
        Ok((c, used, imgs)) => println!("v{version}: used {used} of {} ({} components, {} images)", payload.len() - 4, c.count(), imgs.len()),
        Err(e) => println!("v{version}: {e:?}"),
    }
}
