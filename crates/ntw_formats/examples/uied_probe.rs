//! Research helper: reads the UIEd template library `data/UI/Templates/uied.templates` (loose
//! file) with `ntw_formats::ui_templates` and lists its entries. Read-only.
//!   cargo run -p ntw_formats --release --example uied_probe [-- <name> (prints that template's tree)]
use ntw_formats::ui_layout::UiComponent;
use ntw_formats::ui_templates::UiTemplateLibrary;

const FILE: &str = r"C:\Program Files (x86)\Steam\steamapps\common\Napoleon Total War\data\UI\Templates\uied.templates";

fn tree(c: &UiComponent, depth: usize) {
    let st = c.initial_state();
    println!(
        "{:indent$}{} off={:?} vis={} imgs={} states=[{}] script_override={:?} events={:?}",
        "",
        c.id,
        c.offset,
        c.visible,
        c.images.len(),
        c.states.iter().map(|s| format!("{}:{}x{}", s.name, s.width, s.height)).collect::<Vec<_>>().join(","),
        c.script_override,
        c.events,
        indent = depth * 2
    );
    let _ = st;
    for k in &c.children {
        tree(k, depth + 1);
    }
}

fn main() {
    let b = std::fs::read(FILE).unwrap();
    if let Ok(n) = std::env::var("UIED_DEBUG") {
        debug_entry(&b, &n);
        return;
    }
    let lib = match UiTemplateLibrary::read(&b) {
        Ok(l) => l,
        Err(e) => {
            println!("{e}");
            return;
        }
    };
    match std::env::args().nth(1) {
        Some(name) => {
            let t = lib.get(&name).expect("no such template");
            println!("{} v{} images {:?}", t.name, t.version, t.images.iter().map(|i| (&i.0, i.1.len())).collect::<Vec<_>>());
            tree(&t.component, 0);
        }
        None => {
            for t in &lib.templates {
                println!("{:<40} v{:<3} images {:>2} components {}", t.name, t.version, t.images.len(), t.component.count());
            }
            println!("unreadable ({}): {:?}", lib.unreadable.len(), lib.unreadable);
        }
    }
}

/// `UIED_DEBUG=<name>`: re-reads that entry's component and prints the reader's error.
#[allow(dead_code)]
pub fn debug_entry(b: &[u8], want: &str) {
    let count = u32::from_le_bytes(b[0..4].try_into().unwrap()) as usize;
    let mut p = 4;
    for _ in 0..count {
        let n = b[p..p + 256].iter().position(|&c| c == 0).unwrap_or(256);
        let name = String::from_utf8_lossy(&b[p..p + n]).into_owned();
        let size = u32::from_le_bytes(b[p + 256..p + 260].try_into().unwrap()) as usize;
        let stored = u32::from_le_bytes(b[p + 252..p + 256].try_into().unwrap());
        let start = p + 264;
        if name == want {
            let payload = &b[start..start + size];
            let mut q = 4;
            let imgs = u32::from_le_bytes(payload[0..4].try_into().unwrap());
            for _ in 0..imgs {
                let len = u16::from_le_bytes(payload[q..q + 2].try_into().unwrap()) as usize;
                q += 2 + len;
                q += ntw_formats::ui_templates::tga_len(&payload[q..]).unwrap();
            }
            println!("{name}: stored v{stored}, images {imgs}, component at payload+{q} (file {}), {} bytes", start + q, size - q);
            for v in [stored, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39] {
                match ntw_formats::ui_layout::read_template_component(&payload[q..], v) {
                    Ok((c, used, _)) => println!("  v{v}: ok, used {used} of {}, {} components", size - q, c.count()),
                    Err(e) => println!("  v{v}: {e:?}"),
                }
            }
        }
        p = start + size;
    }
}
