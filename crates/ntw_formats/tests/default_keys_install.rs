//! `text/default_keys.xml` against a real install (read-only). `#[ignore]`d. Run with:
//! ```text
//! cargo test -p ntw_formats --test default_keys_install -- --ignored --nocapture
//! ```

use std::path::PathBuf;

use ntw_formats::default_keys::{Held, Modifier, PATH, Shortcuts};
use ntw_formats::pack::Vfs;

const DEFAULT_DATA_DIR: &str = r"C:\Program Files (x86)\Steam\steamapps\common\Napoleon Total War\data";

fn vfs() -> Vfs {
    let dir = std::env::var_os("NTW_DATA_DIR").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(DEFAULT_DATA_DIR));
    Vfs::open_install(dir).expect("open install")
}

/// The shipped file: version 32, keysets `fps` and `total_war` (the exe's default is `fps`), and the
/// campaign root's shortcuts with their vanilla keys.
#[test]
#[ignore]
fn shipped_campaign_root_shortcuts() {
    let s = Shortcuts::parse(&vfs().read(PATH).unwrap()).unwrap();
    assert_eq!(s.version, 32);
    assert_eq!(s.modes, ["battle", "campaign", "all"]);
    let names: Vec<&str> = s.keysets.iter().map(|k| k.name.as_str()).collect();
    assert_eq!(names, ["fps", "total_war"]);
    assert_eq!(s.default_keyset().unwrap().name, "fps");
    let root = s.bindings("campaign", "root");
    let key = |func: &str| root.0.iter().find(|b| b.func == func).map(|b| (b.keys[0].key.as_str(), b.keys[0].modifier));
    assert_eq!(key("quick_save"), Some(("S", Modifier::Ctrl)));
    assert_eq!(key("quick_load"), Some(("L", Modifier::Ctrl)));
    assert_eq!(key("escape_menu"), Some(("ESCAPE", Modifier::Any)));
    let ctrl = Held { ctrl: true, ..Held::default() };
    assert!(root.fired("S", ctrl).eq(["quick_save"]));
    assert!(root.fired("ESCAPE", Held::default()).eq(["escape_menu"]));
    // End turn (RETURN) goes to its button, not the root.
    assert_eq!(s.component("campaign", "end_turn"), "button_end_turn");
    assert_eq!(root.fired("RETURN", Held::default()).count(), 0);
    // Every campaign binding with its component: Return to the end turn button, the camera's
    // repeating steps (`repeating = "1"`, spaces around the `=` in the shipped file).
    let all = s.mode_bindings("campaign");
    let find = |func: &str| all.0.iter().find(|b| b.func == func).unwrap();
    assert_eq!((find("end_turn").component.as_str(), find("end_turn").keys[0].key.as_str()), ("button_end_turn", "RETURN"));
    let fwd = find("step_fwd");
    assert_eq!((fwd.component.as_str(), fwd.keys[0].key.as_str(), fwd.keys[0].modifier, fwd.repeating), ("camera", "W", Modifier::AllowShift, true));
    assert!(!find("end_turn").repeating);
    assert_eq!(find("escape_menu").component, "root");
}
