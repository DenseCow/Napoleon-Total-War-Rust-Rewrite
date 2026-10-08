//! Research helper for the UI files. Read-only.
//!   cargo run -p ntw_formats --example ui_probe -- all              parse every layout, report failures
//!   cargo run -p ntw_formats --example ui_probe -- tree <path>      print a layout's component tree
//!   cargo run -p ntw_formats --example ui_probe -- statefns [prefix]          states with enter / exit functions
//!   cargo run -p ntw_formats --example ui_probe -- statekey <state> <key> [prefix]  transitions out of a state
use ntw_formats::pack::Vfs;
use ntw_formats::ui_layout::{is_layout, UiComponent, UiLayout};

const DATA: &str = r"C:\Program Files (x86)\Steam\steamapps\common\Napoleon Total War\data";

fn tree(c: &UiComponent, depth: usize) {
    let st = c.initial_state();
    println!(
        "{:indent$}{} this={:x} off={:?} vis={} dock={} resize={}{} prio={} imgs={} states=[{}] {}{}",
        "",
        c.id,
        c.this,
        c.offset,
        c.visible,
        c.docking,
        u8::from(c.allow_horizontal_resize),
        u8::from(c.allow_vertical_resize),
        c.priority,
        c.images.len(),
        c.states.iter().map(|s| format!("{}:{}x{}", s.name, s.width, s.height)).collect::<Vec<_>>().join(","),
        st.map(|s| format!("text={:?} label={:?} font={:?} tb={:?} ta={:?} toff={:?} col={:08x} lead={} trk={} u60={} u64={} ud0={} ud4={} metrics={:?} ", s.text, s.text_label, s.font, s.text_align, s.text_behaviour, s.editor_pos, s.font_colour, s.font_leading, s.font_tracking, s.text_x_offset, s.text_y_offset, s.unknown_d0, s.unknown_d4, s.image_metrics.iter().map(|m| (m.image, m.offset, m.width, m.height, m.tile, m.dock_point, m.colour)).collect::<Vec<_>>())).unwrap_or_default(),
        c.events.iter().map(|(e, f)| format!("{e}->{f} ")).collect::<String>(),
        indent = depth * 2
    );
    for i in &c.images {
        println!("{:indent$}  [img {:x}] {} {}x{} {:08x}", "", i.this, i.path, i.width, i.height, i.colour, indent = depth * 2);
    }
    if !c.properties.is_empty() {
        println!("{:indent$}  props={:?}", "", c.properties, indent = depth * 2);
    }
    if !c.script.is_empty() || !c.script_override.is_empty() || !c.template.is_empty() {
        println!("{:indent$}  script={:?} override={:?} template={:?}", "", c.script, c.script_override, c.template, indent = depth * 2);
    }
    for ch in &c.children {
        tree(ch, depth + 1);
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let dir = std::env::var("NTW_DATA_DIR").unwrap_or_else(|_| DATA.into());
    let Some(command) = args.first() else {
        eprintln!("usage: ui_probe <all [verbose] | tree <path> | statefns [prefix] | statekey <state> <key> [prefix]>");
        std::process::exit(2);
    };
    let vfs = Vfs::open_install(&dir).unwrap();
    match command.as_str() {
        "all" => {
            let (mut ok, mut bad) = (0, 0);
            for p in vfs.list("ui/") {
                let Ok(b) = vfs.read(p) else { continue };
                if !is_layout(&b) {
                    continue;
                }
                match UiLayout::read(&b) {
                    Ok(l) => {
                        ok += 1;
                        if args.get(1).is_some() {
                            println!("ok  v{} {} components={}", l.version, p, l.root.count());
                        }
                    }
                    Err(e) => {
                        bad += 1;
                        println!("ERR {} {}: {e}", String::from_utf8_lossy(&b[..10]), p);
                    }
                }
            }
            println!("parsed {ok}, failed {bad}");
        }
        "statefns" => {
            // statefns [prefix]: every component whose states have enter / exit functions, and
            // whether a bound event calls the same function (it would then run twice per click).
            fn walk(c: &UiComponent, path: &str) {
                for s in c.states.iter().filter(|s| !s.enter_function.is_empty() || !s.exit_function.is_empty()) {
                    let also_event: Vec<_> = c.events.iter().filter(|(_, f)| *f == s.enter_function || *f == s.exit_function).map(|(e, _)| e.as_str()).collect();
                    println!("{path} {} state={:?} enter={:?} exit={:?} same_fn_events={also_event:?}", c.id, s.name, s.enter_function, s.exit_function);
                }
                for ch in &c.children {
                    walk(ch, path);
                }
            }
            let prefix = args.get(1).map(String::as_str).unwrap_or("ui/");
            for p in vfs.list(prefix) {
                let Ok(b) = vfs.read(p) else { continue };
                if is_layout(&b)
                    && let Ok(l) = UiLayout::read(&b)
                {
                    walk(&l.root, p);
                }
            }
        }
        "statekey" => {
            // statekey <state name> <transition key> [prefix]: every component whose state of that
            // name has a transition on that pointer key (e.g. statekey Selected 3: left up).
            fn walk(c: &UiComponent, path: &str, name: &str, key: u32) {
                for s in c.states.iter().filter(|s| s.name.eq_ignore_ascii_case(name)) {
                    for t in s.transitions.iter().filter(|t| t.key == key) {
                        let to = c.states.iter().find(|x| x.this == t.value).map(|x| x.name.as_str()).unwrap_or("?");
                        println!("{path} {} {} --{key}--> {to}", c.id, s.name);
                    }
                }
                for ch in &c.children {
                    walk(ch, path, name, key);
                }
            }
            let (Some(state), Some(Ok(key))) = (args.get(1), args.get(2).map(|k| k.parse::<u32>())) else {
                eprintln!("usage: ui_probe statekey <state name> <transition key (number)> [path prefix]");
                std::process::exit(2);
            };
            for p in vfs.list(args.get(3).map(String::as_str).unwrap_or("ui/")) {
                let Ok(b) = vfs.read(p) else { continue };
                if is_layout(&b)
                    && let Ok(l) = UiLayout::read(&b)
                {
                    walk(&l.root, p, state, key);
                }
            }
        }
        "tree" => {
            let l = UiLayout::read(&vfs.read(&args[1]).unwrap()).unwrap();
            println!("version {}", l.version);
            tree(&l.root, 0);
        }
        "states" => {
            let l = UiLayout::read(&vfs.read(&args[1]).unwrap()).unwrap();
            let c = l.root.find(&args[2]).unwrap();
            for s in &c.states {
                println!("state {:x} {} interactive={} disabled={} enter={:?} exit={:?} transitions={:?}", s.this, s.name, s.interactive, s.disabled, s.enter_function, s.exit_function, s.transitions);
            }
        }
        "db" => {
            // db <table> <codes>, e.g. db battles "s,s,b,s,o,i,i,b,b,b,b,o,i"
            let bytes = vfs.read(&format!("db/{0}_tables/{0}", args[1])).unwrap();
            let t = ntw_formats::db::DbTable::read(&bytes, &ntw_formats::db::Schema::from_codes(&args[2]).unwrap()).unwrap();
            println!("version {} rows {}", t.version, t.rows.len());
            for r in &t.rows {
                println!("{}", r.iter().map(|v| format!("{v:?}")).collect::<Vec<_>>().join(" | "));
            }
        }
        "locgrep" => {
            let loc = ntw_formats::loc::Localisation::from_vfs(&vfs).unwrap();
            let mut hits: Vec<_> = loc.iter().filter(|(k, v)| k.contains(args[1].as_str()) || v.contains(args[1].as_str())).collect();
            hits.sort();
            for (k, v) in hits.iter().take(200) {
                println!("{k} => {:?}", v.chars().take(160).collect::<String>());
            }
        }
        "loc" => {
            let loc = ntw_formats::loc::Localisation::from_vfs(&vfs).unwrap();
            for k in &args[1..] {
                println!("{k} => {:?}", loc.get(k));
            }
        }
        _ => eprintln!("unknown command"),
    }
}
