//! Research helper: runs the original front end headless (no window) and clicks components.
//! Read-only. Usage:
//!   cargo run -p ntw_script --release --example ui_run -- [--tree] [--key ESCAPE] id1,id2,...
//! Prints the scripts' log after each click and, with `--tree`, the visible components at the end.
use ntw_formats::loc::Localisation;
use ntw_script::ScriptSource;
use ntw_script::ui::{FrontEndFacts, NodeId, PointerEvent, UiScriptHost, UiWorld};

const DATA: &str = r"C:\Program Files (x86)\Steam\steamapps\common\Napoleon Total War\data";

fn print_tree(w: &UiWorld, id: NodeId, depth: usize) {
    let Some(n) = w.get(id) else { return };
    if !n.visible && std::env::var_os("UI_RUN_ALL").is_none() {
        return;
    }
    let text = n.current().map(|s| s.text.as_str()).unwrap_or("");
    let tip = n.tooltip.clone().unwrap_or_else(|| {
        let s = n.current().map(|s| s.tooltip_text.clone()).unwrap_or_default();
        if s.is_empty() { n.data.tooltip_text.clone() } else { s }
    });
    let tip = if tip.is_empty() { String::new() } else { format!(" tip={:?}", tip.chars().take(60).collect::<String>()) };
    println!(
        "{:indent$}{} #{} [{}] {:.0},{:.0} {:.0}x{:.0} {}{tip}",
        "",
        n.data.id,
        id,
        n.state_name(),
        n.rect.x,
        n.rect.y,
        n.rect.w,
        n.rect.h,
        if text.is_empty() { String::new() } else { format!("{text:?}") },
        indent = depth * 2
    );
    for &c in &n.children {
        print_tree(w, c, depth + 1);
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let dir = std::env::var("NTW_DATA_DIR").unwrap_or_else(|_| DATA.into());
    let source = ScriptSource::from_install(&dir).unwrap();
    let vfs = ntw_formats::pack::Vfs::open_install(&dir).unwrap();
    let loc = Localisation::from_vfs(&vfs).unwrap();
    let original = std::env::var_os("APPDATA").map(|a| std::path::PathBuf::from(a).join("The Creative Assembly").join("Napoleon"));
    let facts = FrontEndFacts { campaign_saves_exist: true, game_version: "1.3.0".into(), original_user_dir: original, user_dir: None, nap_unlock: 1, ..Default::default() };
    let limits = ntw_data::load_game_limits(&vfs, &mut Vec::new()).unwrap();
    let host = UiScriptHost::new(source, loc, facts, (1280.0, 960.0), limits).unwrap();
    let root = host.load_root_layout("data/ui/frontend ui/layout").unwrap();
    for l in host.take_log() {
        println!("  {l}");
    }
    let tree = args.iter().any(|a| a == "--tree");
    let mut it = args.iter().filter(|a| *a != "--tree");
    while let Some(a) = it.next() {
        if a == "--key" {
            let key = it.next().unwrap();
            println!("== key {key}");
            host.key(key);
            for l in host.take_log() {
                println!("  {l}");
            }
            continue;
        }
        for id in a.split(',').filter(|s| !s.is_empty()) {
            // "type:<text>": typed into the focused text field ("_" = space); "key:<NAME>": a key.
            if let Some(text) = id.strip_prefix("type:") {
                println!("== type {text}: taken {}", host.text_input(&text.replace('_', " ")));
                continue;
            }
            if let Some(k) = id.strip_prefix("key:") {
                println!("== key {k}: taken {}", host.key(k));
                continue;
            }
            // "hover:<id>": rest the pointer on the component (its tooltip).
            let (hover, id) = match id.strip_prefix("hover:") {
                Some(h) => (true, h),
                None => (false, id),
            };
            let mut target = None;
            host.world().visit_visible(root, &mut |n, node| {
                if target.is_none() && node.data.id == id {
                    target = Some(n);
                }
            });
            println!("== {} {id} ({target:?})", if hover { "hover" } else { "click" });
            if hover {
                if let Some(r) = target.and_then(|t| host.world().get(t).map(|n| n.rect)) {
                    host.set_cursor_position(r.x + r.w / 2.0, r.y + r.h / 2.0);
                }
                host.hover(target);
            } else if let Some(t) = target {
                for e in [PointerEvent::Enter, PointerEvent::LeftDown, PointerEvent::LeftUp, PointerEvent::Leave] {
                    host.pointer(t, e);
                }
            }
            for l in host.take_log() {
                println!("  {l}");
            }
            for r in host.take_requests() {
                println!("  request {r:?}");
            }
        }
    }
    if tree {
        print_tree(&host.world(), root, 0);
    }
}
