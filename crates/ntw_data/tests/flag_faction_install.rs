//! The flag a faction flies: the key it is looked up under. Read-only, against a real install.
//! All `#[ignore]`d. Run with:
//! ```text
//! cargo test -p ntw_data --test flag_faction_install -- --ignored --nocapture
//! ```
//! The install path can be overridden with `NTW_DATA_DIR`.
//!
//! This is the install-side evidence for `napoleon::battle::flag::FlagLibrary::flag_of`. It needs
//! both the `factions` table (this crate) and the `*.tai` atlas reader, which is why it is here and
//! not beside the cloth solve in `ntw_sim`: `ntw_data` depends on `ntw_sim`, so the reverse
//! dev-dependency would be a cycle.

use std::collections::BTreeSet;
use std::path::PathBuf;

use ntw_data::GameDatabase;
use ntw_formats::pack::Vfs;
use ntw_formats::texture_atlas::TaiAtlas;

const ATLAS: &str = r"rigidmodels\flags\textures\flags.tai";

fn stem(p: &str) -> &str {
    p.rsplit(['\\', '/']).next().unwrap_or(p)
}

fn install() -> (GameDatabase, Vfs) {
    let dir = std::env::var_os("NTW_DATA_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\Program Files (x86)\Steam\steamapps\common\Napoleon Total War\data"));
    (GameDatabase::from_install(&dir).expect("load the database"), Vfs::open_install(&dir).expect("open install"))
}

/// The flag a faction flies is looked up under the **last segment of `factions.flag_path`**
/// (`data\ui\flags\france` -> `flag_france.tga`), and that is the only linkage the shipped data gives
/// for the **39** dependent factions. CONFIRMED over all 77 `factions` rows: the segment is a key of
/// the shipped battle atlas for **every** one, so no faction needs `flag_default.tga`.
///
/// This also records the negative that closed round 8's open item: `factions.faction_group` does
/// **not** name the parent, because the rebels' group is literally their own key.
#[test]
#[ignore]
fn every_factions_flag_key_is_in_the_shipped_atlas() {
    let (db, vfs) = install();
    let a = TaiAtlas::read(&String::from_utf8(vfs.read(ATLAS).expect("read")).expect("utf8")).expect("parse flags.tai");
    assert_eq!(db.factions.len(), 77, "77 faction rows");

    let mut own_image = 0;
    let mut needs_the_path = Vec::new();
    for f in db.factions.iter() {
        let s = stem(&f.flag_path);
        assert!(!s.is_empty(), "{:?} has no flag_path", f.key);
        assert!(a.find(&format!("flag_{s}.tga")).is_some(), "{:?}: flag_path {s:?} is not in {ATLAS}", f.key);
        if a.find(&format!("flag_{}.tga", f.key)).is_some() {
            own_image += 1;
        } else {
            needs_the_path.push(f.key.clone());
        }
    }
    assert_eq!(own_image, 38, "38 factions have a flag of their own key");
    assert_eq!(needs_the_path.len(), 39, "the 39 that need flag_path: {needs_the_path:?}");
    println!("{own_image} factions have flag_<key>.tga of their own; all 77 name an image through flag_path");

    // Every one of the 39 is a dependent faction: `egy_*`, `ita_*`, `spa_*`, `tut_*` or `*_rebels` --
    // plus `sicily`, whose `flag_path` is `naples_sicily` and which is the only one that does not
    // announce itself with a prefix.
    for k in &needs_the_path {
        let dependent = ["egy_", "ita_", "spa_", "tut_"].iter().any(|p| k.starts_with(p))
            || k.ends_with("_rebels")
            || k == "sicily";
        assert!(dependent, "{k} has no own image but is not a dependent faction -- re-check this test");
    }

    // `faction_group` is NOT the linkage, and this is the negative that closed round 8's open item:
    // most rebels' group is literally their own key (`austrian_rebels` -> `austrian_rebels`), so
    // stripping `_group` finds no image. A few happen to resolve (`greek_rebels` sits in
    // `venice_group`), which is exactly why it cannot be the rule.
    let groups: BTreeSet<&str> = db.factions.iter().filter(|f| f.key.ends_with("_rebels")).map(|f| f.faction_group.as_str()).collect();
    assert!(!groups.is_empty(), "no *_rebels factions at all");
    let (works, fails): (Vec<&str>, Vec<&str>) =
        groups.iter().partition(|g| a.find(&format!("flag_{}.tga", g.strip_suffix("_group").unwrap_or(g))).is_some());
    assert!(
        fails.len() > works.len(),
        "faction_group would now resolve most rebels ({works:?} vs {fails:?}) -- re-check this test and the round-8 note"
    );
    println!("faction_group would resolve only {} of the {} rebel groups ({works:?}); flag_path resolves all", works.len(), groups.len());
    // The rebels' `flag_path` is what does work, and they share two images.
    let rebel_stems: BTreeSet<&str> = db.factions.iter().filter(|f| f.key.ends_with("_rebels")).map(|f| stem(&f.flag_path)).collect();
    assert_eq!(rebel_stems, BTreeSet::from(["rebels_eastern", "rebels_europe"]), "the rebels share two flag images");

    // The aliases are the interesting part: `ita_piedmont` flies Sardinia's flag, and the two
    // French republics point at different images.
    for (key, want) in [("ita_piedmont", "sardinia"), ("egy_britain", "britain"), ("ita_french_republic", "france"), ("egy_french_republic", "france_republic")] {
        let f = db.factions.get(key).unwrap_or_else(|| panic!("{key} is not a faction"));
        assert_eq!(stem(&f.flag_path), want, "{key}'s flag_path");
    }
}

/// Round 10's addition: of every *other* column that could carry the battle flag's key, only
/// `flag_path` resolves for **all** 39 dependent factions -- so the shipped data leaves it as the
/// only linkage it can support, and the rest are refuted by count rather than by argument.
///
/// The rivals, and what each resolves (probe `cargo run -p ntw_data --example unit0d_probe --
/// flagcands` prints the whole table):
///
/// | column | factions / 77 | dependents / 39 |
/// |---|---|---|
/// | `flag_path` | **77** | **39** |
/// | `model_faction` | 51 | 22 |
/// | `rebel_flag_path` | 64 | 26 (13 of them give nothing at all) |
/// | `republic_flag_path` | 17 | 10 (29 give nothing) |
/// | `faction_group` | 9 | 7 |
/// | `subculture` | 0 | 0 |
///
/// The rivals that do resolve also *disagree* with `flag_path`, which is what makes them refutable
/// rather than merely weaker: `french_rebels` -> `france` under `model_faction` but
/// `rebels_europe` under `flag_path`, and `egy_bedouin` -> `rebels_eastern` under
/// `rebel_flag_path` but `bedouin` under `flag_path`.
#[test]
#[ignore]
fn only_flag_path_resolves_every_dependent_faction() {
    /// One rival column: a name and the key it would give a faction.
    type Rival = (&'static str, fn(&ntw_data::FactionRecord) -> String);
    fn stem(p: &str) -> String {
        p.rsplit(['\\', '/']).next().unwrap_or(p).to_owned()
    }
    fn group(f: &ntw_data::FactionRecord) -> String {
        f.faction_group.strip_suffix("_group").unwrap_or(&f.faction_group).to_owned()
    }
    let (db, vfs) = install();
    let a = TaiAtlas::read(&String::from_utf8(vfs.read(ATLAS).expect("read")).expect("utf8")).expect("parse flags.tai");
    let has = |k: &str| a.find(&format!("flag_{k}.tga")).is_some();

    let rows: Vec<&ntw_data::FactionRecord> = db.factions.iter().collect();
    let dependents: Vec<&&ntw_data::FactionRecord> = rows.iter().filter(|f| !has(&f.key)).collect();
    assert_eq!(dependents.len(), 39, "39 dependent factions");

    let rivals: [Rival; 5] = [
        ("model_faction", |f: &ntw_data::FactionRecord| f.model_faction.clone()),
        ("subculture", |f: &ntw_data::FactionRecord| f.subculture.clone()),
        ("republic_flag_path", |f: &ntw_data::FactionRecord| f.republic_flag_path.as_deref().map(stem).unwrap_or_default()),
        ("rebel_flag_path", |f: &ntw_data::FactionRecord| f.rebel_flag_path.as_deref().map(stem).unwrap_or_default()),
        ("faction_group", group),
    ];
    for (name, get) in &rivals {
        let all = rows.iter().filter(|f| has(&get(f))).count();
        let dep = dependents.iter().filter(|f| has(&get(f))).count();
        println!("{name:20} resolves {all:2}/77 factions, {dep:2}/39 dependents");
        assert!(dep < 39, "{name} now resolves every dependent faction ({dep}/39) -- re-check this test and the round-10 note");
    }
    // And `flag_path`, the one we use, resolves all 39.
    assert_eq!(dependents.iter().filter(|f| has(&stem(&f.flag_path))).count(), 39);
}

/// **A correction to round 9, kept here so the next reader sees it.** The note said
/// `0x01227BD0` is what the exe looks a faction's battle flag up by, and that what it is *handed*
/// was UNKNOWN. That is wrong in its second half: `0x01227BD0` has **exactly one** direct caller in
/// `Napoleon.exe` (`0x011CE120`, the flag system's constructor) and it hands it the literal string
/// `"default"` -- the exe-wide negative is that the ASCII `flag_` is referenced from **two**
/// addresses only (`0x01227C32` in `0x01227BD0` and `0x0123C585` in `0x0123C570`).
///
/// So `0x01227BD0` is the flag system's *registry lookup* -- it searches the record array at
/// `this+0x2FF4` for a record whose name matches and whose `+0x64` equals the kind, and only then
/// builds `flag_<name>.tga` and falls back to `flag_default.tga` -- and the caller uses it once to
/// get the default record, which it stores at `this+0x3008`. `0x0123C570(index, name, kind)` is
/// the setter that completes the same name for a record that already exists, and it has **zero**
/// direct callers.
///
/// What is still UNKNOWN is the same thing round 9 said, one step further on: **which string names
/// a flag record.** It is a record field at `+0x38`, and the records themselves come from a table
/// this pass did not identify. The `flags.tai` atlas, `campaignflag.fx`,
/// `Flag_primary`/`Flag_secondary`/`naval_ID_frame`, `naval_id_`, `flag_orn_group_0..10` and the
/// `army_flag_scale` / `navy_flag_scale` / `occupied_settlement_flag_scale` CVars all say this is
/// the **campaign-map** flag system, so whether the standard bearer even uses it is open too.
#[test]
#[ignore]
fn the_battle_flag_target_is_the_campaign_flag_system_not_the_bearer() {
    // The shipped half of the claim: the atlas the flag system reads is the campaign one, and the
    // art around it is campaign art. `naval_id.tai` and the ornament models exist only beside it.
    let (db, vfs) = install();
    for p in [
        r"rigidmodels\flags\textures\flags.tai",
        r"rigidmodels\flags\textures\exp_flags.tai",
        r"rigidmodels\flags\textures\naval_id.tai",
        r"rigidmodels\flags\textures\overlay.tai",
    ] {
        assert!(vfs.contains(p), "{p} missing");
    }
    // `flag_default.tga` is the fallback the registry reaches for, and it is in the atlas.
    let a = TaiAtlas::read(&String::from_utf8(vfs.read(ATLAS).expect("read")).expect("utf8")).expect("parse flags.tai");
    assert!(a.find("flag_default.tga").is_some(), "the fallback image is in the atlas");
    // And the eight atlas images no faction column names are what make the "this is the campaign's
    // flag chain" reading concrete: the fallback itself, two republic variants the `factions` rows
    // never point at, `pirates` and `rebels_other` -- the last two only a campaign-side chain could
    // want.
    let used: std::collections::BTreeSet<String> = db
        .factions
        .iter()
        .flat_map(|f| {
            [
                Some(f.key.clone()),
                Some(stem(&f.flag_path).to_owned()),
                Some(f.model_faction.clone()),
                f.republic_flag_path.as_deref().map(|p| stem(p).to_owned()),
                f.rebel_flag_path.as_deref().map(|p| stem(p).to_owned()),
            ]
            .into_iter()
            .flatten()
        })
        .map(|k| format!("flag_{k}.tga"))
        .collect();
    let unused: Vec<&String> = a.entries.keys().filter(|n| !used.contains(*n)).collect();
    assert_eq!(unused.len(), 8, "the images no faction column names: {unused:?}");
    for name in ["flag_pirates.tga", "flag_rebels_other.tga", "flag_default.tga"] {
        assert!(unused.contains(&&name.to_owned()), "{name} should be one of the unnamed images: {unused:?}");
    }
}