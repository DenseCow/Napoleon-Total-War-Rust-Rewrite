//! Characters rules against the install (slot 0-G, `analysis/fidelity/CHARACTERS_FIDELITY.md`): the
//! trait and ancillary rules on real DB data, and the yearly natural-death pass. Skipped without an
//! install.

use std::path::PathBuf;

use ntw_data::GameDatabase;
use ntw_formats::campaign_map::GameFiles;
use ntw_formats::db::DbValue;
use ntw_formats::db_folder::{RawTable, RowKey};
use ntw_formats::esf::EsfFile;
use ntw_formats::pack::Vfs;
use ntw_sim::campaign::characters::{self, AncillaryRefusal, TraitOutcome};
use ntw_sim::campaign::{CampaignModel, CharacterId};

fn data_dir() -> PathBuf {
    std::env::var_os("NTW_DATA_DIR").map(PathBuf::from).unwrap_or_else(|| {
        PathBuf::from(r"C:\Program Files (x86)\Steam\steamapps\common\Napoleon Total War\data")
    })
}

fn model() -> Option<CampaignModel> {
    let dir = data_dir();
    if !dir.is_dir() {
        eprintln!("skipped: no install at {}", dir.display());
        return None;
    }
    let db = GameDatabase::from_install(&dir).expect("DB");
    let vfs = Vfs::open_install(&dir).expect("vfs");
    let bytes = GameFiles { vfs: &vfs }.read("campaigns/eur_napoleon/startpos.esf").expect("startpos");
    let esf = EsfFile::from_bytes(&bytes).expect("esf");
    let mut l = ntw_campaign::read_esf(&esf, &db).expect("load");
    assert!(l.set_human("france"));
    Some(l.model)
}

/// A French character of `kind` with no post.
fn pick(m: &CampaignModel, kind: &str) -> CharacterId {
    let france = m.faction_by_key("france").unwrap().id;
    m.world
        .characters
        .values()
        .find(|c| c.faction == france && c.kind.esf_name() == kind && m.world.character_details.get(&c.id).is_some_and(|d| d.post == 0))
        .map(|c| c.id)
        .expect("a character")
}

#[test]
fn traits_follow_the_exe_rules() {
    let Some(mut m) = model() else { return };
    let rules = m.rules.characters.clone();
    assert_eq!((rules.max_traits, rules.max_ancillaries), (6, 3));
    let g = pick(&m, "General");
    m.world.character_details.get_mut(&g).unwrap().traits.clear();
    // New trait, then more points on it.
    assert!(matches!(characters::add_trait_points(&mut m, g, "C_General_Brave", 2), TraitOutcome::Added { .. }));
    assert!(matches!(characters::add_trait_points(&mut m, g, "C_General_Brave", 1), TraitOutcome::Raised { .. }));
    // An antitrait absorbs points first: C_General_Good_Field_Commander / C_General_Bad_Field_Commander
    // style pairs come from trait_to_antitraits.
    let (t, anti) = rules
        .traits
        .iter()
        .find_map(|(k, r)| r.antitraits.first().map(|a| (k.clone(), a.clone())))
        .expect("an antitrait pair");
    // `t` lists `anti`: gaining `anti` takes points off `t`.
    characters::add_trait_points(&mut m, g, &t, 5);
    assert_eq!(characters::add_trait_points(&mut m, g, &anti, 2), TraitOutcome::Absorbed);
    let held = m.world.character_details[&g].traits.iter().find(|x| x.key == t).map(|x| x.points);
    assert!(held.is_some_and(|p| p <= 3), "{t} after {anti}: {held:?}");
    // Six traits at most.
    let keys: Vec<String> = rules.traits.keys().filter(|k| k.starts_with("C_General_")).take(12).cloned().collect();
    for k in &keys {
        characters::add_trait_points(&mut m, g, k, 1);
    }
    assert!(m.world.character_details[&g].traits.len() <= 6);
}

/// stickytrait: a held trait's level on vanilla thresholds (`0x008B5380`, CHARACTERS_FIDELITY.md §1):
/// kept below the first threshold, and a fall from the no-going-back level lands on it.
///
/// Run on the install: `cargo test -p ntw_campaign --test characters held_trait_levels -- --nocapture`.
#[test]
fn held_trait_levels_follow_the_exe_on_vanilla_thresholds() {
    let Some(mut m) = model() else { return };
    let rules = m.rules.characters.clone();
    let rule = |k: &str| rules.traits.get(k).unwrap_or_else(|| panic!("{k} in character_traits"));
    let (bad, brave) = (rule("C_General_Attacker_Bad"), rule("C_General_Brave"));
    assert_eq!((bad.no_going_back_level, bad.thresholds.clone()), (3, vec![(1, 4), (2, 8), (3, 16)]));
    assert_eq!((brave.no_going_back_level, brave.thresholds.clone()), (3, vec![(1, 1), (2, 2), (3, 4), (4, 8), (5, 16)]));
    let anti = |r: &ntw_sim::campaign::characters::TraitRule| r.antitraits.first().cloned().expect("an antitrait");
    let g = pick(&m, "General");
    let held = |m: &CampaignModel, k: &str| m.world.character_details[&g].traits.iter().find(|t| t.key == k).map(|t| (t.points, t.level));
    // Attacker_Bad at level 2 (8 points); its antitrait takes 6: 2 points, below level 1's 4, level 2 kept.
    m.world.character_details.get_mut(&g).unwrap().traits.clear();
    characters::add_trait_points(&mut m, g, "C_General_Attacker_Bad", 8);
    assert_eq!(held(&m, "C_General_Attacker_Bad"), Some((8, 2)));
    assert_eq!(characters::add_trait_points(&mut m, g, &anti(bad), 6), TraitOutcome::Absorbed);
    assert_eq!(held(&m, "C_General_Attacker_Bad"), Some((2, 2)));
    // Brave at level 5 (16); its antitrait takes 13: 3 points reach level 2, so it lands on level 3 at 4.
    m.world.character_details.get_mut(&g).unwrap().traits.clear();
    characters::add_trait_points(&mut m, g, "C_General_Brave", 16);
    assert_eq!(held(&m, "C_General_Brave"), Some((16, 5)));
    assert_eq!(characters::add_trait_points(&mut m, g, &anti(brave), 13), TraitOutcome::Absorbed);
    assert_eq!(held(&m, "C_General_Brave"), Some((4, 3)));
}

#[test]
fn ancillaries_follow_the_exe_rules() {
    let Some(mut m) = model() else { return };
    let g = pick(&m, "General");
    m.world.character_details.get_mut(&g).unwrap().ancillaries.clear();
    // A minister-only ancillary is refused for a general (agent type).
    let rules = m.rules.characters.clone();
    let (minister_only, _) = rules
        .ancillaries
        .iter()
        .find(|(_, r)| r.character && r.agents == ["minister"] && r.start_year <= 1805 && r.end_year > 1805)
        .expect("a minister ancillary");
    assert_eq!(characters::add_ancillary(&mut m, g, minister_only), Err(AncillaryRefusal::CharacterType));
    // General ancillaries for France's subculture, three at most.
    let sub = rules.subculture("france").to_string();
    let ok: Vec<String> = rules
        .ancillaries
        .iter()
        .filter(|(_, r)| {
            r.character && r.agents.iter().any(|a| a == "General") && r.subcultures.contains(&sub) && r.start_year <= 1805 && r.end_year > 1805 && !r.world_unique && !r.faction_unique && r.excluded.is_empty()
        })
        .map(|(k, _)| k.clone())
        .take(6)
        .collect();
    assert!(ok.len() >= 4);
    assert_eq!(characters::add_ancillary(&mut m, g, &ok[0]), Ok(None));
    assert_eq!(characters::add_ancillary(&mut m, g, &ok[0]), Err(AncillaryRefusal::AlreadyHeld));
    for k in &ok[1..] {
        let _ = characters::add_ancillary(&mut m, g, k);
    }
    assert!(m.world.character_details[&g].ancillaries.len() <= 3);
}

#[test]
fn yearly_pass_kills_by_age_and_only_at_the_year_end() {
    let Some(mut m) = model() else { return };
    let old = pick(&m, "General");
    let young = pick(&m, "colonel");
    m.world.character_details.get_mut(&old).unwrap().birth.as_mut().unwrap().year = 1700;
    m.world.character_details.get_mut(&young).unwrap().birth.as_mut().unwrap().year = 1780;
    // Not the year's last turn: nothing happens.
    assert_eq!(characters::yearly_character_pass(&mut m), Default::default());
    // The last turn of the year: the 105-year-old dies, the 25-year-old does not.
    m.calendar.turn_in_year = m.calendar.turns_per_year - 1;
    let pass = characters::yearly_character_pass(&mut m);
    assert!(pass.died.contains(&old));
    assert!(m.world.characters.contains_key(&young) && !m.world.characters.contains_key(&old));
    assert!(!m.world.character_details.contains_key(&old));
}

#[test]
fn recruitment_pools_load_and_their_candidates_gain_nothing() {
    let Some(mut m) = model() else { return };
    let france = m.faction_by_key("france").unwrap().id;
    let d = &m.world.faction_details[&france];
    let (generals, admirals) = (d.general_pool.0.clone(), d.admiral_pool.0.clone());
    println!("france pools: generals {generals:?} timer {}, admirals {admirals:?} timer {}", d.general_pool.1, d.admiral_pool.1);
    assert_eq!((generals.len(), admirals.len()), (3, 3));
    for c in generals.iter().chain(&admirals) {
        assert!(m.world.characters.contains_key(c), "pool candidate {c:?} is a character of the faction");
        assert!(characters::is_pool_candidate(&m, *c));
    }
    // A candidate never gains from the scripts, even with chance 100.
    let c = generals[0];
    let before = m.world.character_details[&c].traits.clone();
    for _ in 0..20 {
        assert_eq!(characters::roll_trait(&mut m, c, "C_General_Brave", 1, 100), None);
    }
    assert_eq!(m.world.character_details[&c].traits, before);
}

/// The families load from the start position (FAMILY, CHARACTERS_FIDELITY.md §5c): ten members, the
/// leader present, the ordinal pairs.
#[test]
fn families_load() {
    let Some(m) = model() else { return };
    let austria = m.faction_by_key("austria").unwrap().id;
    let fam = m.world.faction_details[&austria].family.as_ref().expect("family");
    assert_eq!(fam.members.len(), 10);
    let leader = &fam.members[0];
    assert!(leader.exists && leader.male && leader.age > 20);
    assert!(leader.names.iter().any(|n| n.contains("royalty")), "{:?}", leader.names);
    assert!(!fam.ordinals.is_empty());
}

/// A monarch who dies of old age: the post goes at once to the family's successor, as a new
/// minister named after him and of his age; a minister's post goes to a new minister; nothing is
/// left vacant; the save keeps every rule and reads back with the new leader and family.
#[test]
fn a_dead_monarch_is_succeeded_and_posts_are_refilled() {
    let Some(mut m) = model() else { return };
    let dir = data_dir();
    let db = GameDatabase::from_install(&dir).expect("DB");
    let austria = m.faction_by_key("austria").unwrap().id;
    let d = &m.world.faction_details[&austria];
    let old_leader = d.leader().expect("leader");
    let (minister_post, minister) = d.posts.iter().enumerate().find(|(_, p)| p.key != "faction_leader" && p.governorship.is_none() && p.holder.is_some()).map(|(i, p)| (i, p.holder.unwrap())).expect("a minister");
    let heir = d.family.as_ref().unwrap().members[6].clone();
    for c in [old_leader, minister] {
        m.world.character_details.get_mut(&c).unwrap().birth.as_mut().unwrap().year = 1650;
    }
    m.calendar.turn_in_year = m.calendar.turns_per_year - 1;
    let year = m.calendar.date.year as i32;
    let pass = characters::yearly_character_pass(&mut m);
    assert!(pass.died.contains(&old_leader) && pass.died.contains(&minister));
    let d = &m.world.faction_details[&austria];
    let new_leader = d.leader().expect("a new leader");
    assert_ne!(new_leader, old_leader);
    let nl = &m.world.character_details[&new_leader];
    let fam = d.family.as_ref().unwrap();
    // Austria has no children: the first relative inherits (aged a year by the family pass).
    assert_eq!(fam.members[0].names, heir.names);
    assert_eq!(nl.forename, heir.names[0]);
    assert_eq!(year - nl.birth.unwrap().year as i32, fam.members[0].age);
    assert_eq!(m.world.characters[&new_leader].kind.esf_name(), "minister");
    let refilled = d.posts[minister_post].holder.expect("refilled");
    assert_ne!(refilled, minister);
    assert_eq!(m.world.character_details[&refilled].post, d.posts[minister_post].id as u32);
    // Portraits (`0x009D21B0`, `0x00990900`): the monarch has the family leader's, the new
    // minister one from the minister decks, a picture the install has.
    assert_eq!(nl.portrait, fam.members[0].portrait);
    let vfs = Vfs::open_install(&dir).unwrap();
    let p = &m.world.character_details[&refilled].portrait;
    assert!(p.index >= 0 && p.card.contains("/Cards/minister/") && p.info.contains("/Info/minister/"), "{p:?}");
    assert!(vfs.contains(&p.card) && vfs.contains(&p.info), "{p:?}");
    // Every post of a faction with characters has a living holder.
    for (f, d) in &m.world.faction_details {
        if m.world.characters.values().any(|c| c.faction == *f) {
            for p in &d.posts {
                if let Some(h) = p.holder {
                    assert!(m.world.characters.contains_key(&h));
                }
            }
        }
    }
    // The save.
    let bytes = GameFiles { vfs: &Vfs::open_install(&dir).unwrap() }.read("campaigns/eur_napoleon/startpos.esf").unwrap();
    let src = EsfFile::from_bytes(&bytes).unwrap();
    let out = ntw_campaign::save::write_save(&src, &m, "france", 1, None).unwrap();
    let back = EsfFile::from_bytes(&out.to_bytes().unwrap()).unwrap();
    let r = ntw_campaign::save_check::check(&back);
    assert!(r.violations.is_empty(), "{:?}", r.violations);
    let l = ntw_campaign::read_esf(&back, &db).unwrap();
    let bd = &l.model.world.faction_details[&austria];
    assert_eq!(bd.leader(), Some(new_leader));
    assert_eq!(bd.family, d.family);
    assert_eq!(l.model.world.character_details[&new_leader].forename, heir.names[0]);
    assert_eq!(l.model.world.character_details[&refilled].post, d.posts[minister_post].id as u32);
    for c in [new_leader, refilled] {
        assert_eq!(l.model.world.character_details[&c].portrait, m.world.character_details[&c].portrait, "the save carries the new portraits");
    }
}

/// Dismissing and appointing ministers (CHARACTERS_FIDELITY.md §5c): France (absolute monarchy)
/// seats a spare, Britain (constitutional) makes a new minister and refuses spares; swaps; the
/// save keeps every rule.
#[test]
fn ministers_are_dismissed_and_appointed() {
    use ntw_sim::campaign::CampaignCommand;
    let Some(mut m) = model() else { return };
    let dir = data_dir();
    let france = m.faction_by_key("france").unwrap().id;
    let britain = m.faction_by_key("britain").unwrap().id;
    assert_eq!(m.spare_ministers(france).len(), 5);
    let holders = |m: &CampaignModel, f| -> Vec<(String, CharacterId)> {
        m.world.faction_details[&f].posts.iter().filter(|p| p.key != "faction_leader" && p.governorship.is_none()).filter_map(|p| Some((p.key.clone(), p.holder?))).collect()
    };
    let fh = holders(&m, france);
    // Dismiss: a spare takes the post.
    let spares = m.spare_ministers(france);
    m.apply(CampaignCommand::DismissMinister { minister: fh[0].1 }).unwrap();
    let now = holders(&m, france);
    assert!(!m.world.characters.contains_key(&fh[0].1));
    assert!(spares.contains(&now[0].1));
    assert_eq!(m.spare_ministers(france).len(), 4);
    // Swap two holders.
    m.apply(CampaignCommand::AppointMinister { a: now[1].1, b: now[2].1 }).unwrap();
    let swapped = holders(&m, france);
    assert_eq!((swapped[1].1, swapped[2].1), (now[2].1, now[1].1));
    assert_eq!(m.world.character_details[&now[1].1].post, m.world.faction_details[&france].posts.iter().find(|p| p.holder == Some(now[1].1)).unwrap().id as u32);
    // A spare replaces a holder, who leaves.
    let spare = m.spare_ministers(france)[0];
    m.apply(CampaignCommand::AppointMinister { a: swapped[1].1, b: spare }).unwrap();
    assert!(!m.world.characters.contains_key(&swapped[1].1));
    assert_eq!(holders(&m, france)[1].1, spare);
    // The leader is out of reach.
    let leader = m.world.faction_details[&france].leader().unwrap();
    assert!(m.apply(CampaignCommand::DismissMinister { minister: leader }).is_err());
    // Britain: a new minister, no spares used; spares cannot be appointed.
    assert_eq!(m.world.factions[&britain].government, ntw_sim::campaign::GovernmentType::ConstitutionalMonarchy);
    {
        let bh = holders(&m, britain);
        let before = m.spare_ministers(britain);
        m.apply(CampaignCommand::DismissMinister { minister: bh[0].1 }).unwrap();
        assert_eq!(m.spare_ministers(britain), before);
        let new = holders(&m, britain)[0].1;
        assert!(!before.contains(&new) && new != bh[0].1);
        assert!(m.apply(CampaignCommand::AppointMinister { a: new, b: before[0] }).is_err());
    }
    // The save.
    let bytes = GameFiles { vfs: &Vfs::open_install(&dir).unwrap() }.read("campaigns/eur_napoleon/startpos.esf").unwrap();
    let src = EsfFile::from_bytes(&bytes).unwrap();
    let out = ntw_campaign::save::write_save(&src, &m, "france", 1, None).unwrap();
    let back = EsfFile::from_bytes(&out.to_bytes().unwrap()).unwrap();
    let r = ntw_campaign::save_check::check(&back);
    assert!(r.violations.is_empty(), "{:?}", r.violations);
}

/// The character types' main attributes come from the merged `agents` table's #8 (record +0x44, read by
/// `0x00A198C0`): the vanilla rows give each type the attribute its rank and its cards' primary picture
/// use (CHARACTERS_FIDELITY.md §13). The `ntw_sim` rules fixture copies these values.
#[test]
fn vanilla_agent_main_attributes() {
    use ntw_sim::campaign::agents::main_attribute;
    use ntw_sim::campaign::CharacterKind as K;
    let Some(m) = model() else { return };
    for (kind, main) in [
        (K::General, "command_land"),
        (K::Colonel, "command_land"),
        (K::Admiral, "command_sea"),
        (K::Captain, "command_sea"),
        (K::Gentleman, "research"),
        (K::EasternScholar, "research"),
        (K::Minister, "management"),
        (K::Rake, "subterfuge"),
        (K::Assassin, "subterfuge"),
        (K::Guerilla, "subterfuge"),
        (K::CatholicMissionary, "zeal"),
        (K::OrthodoxMissionary, "zeal"),
        (K::ProtestantMissionary, "zeal"),
    ] {
        assert_eq!(main_attribute(&m.rules, kind), main, "{kind:?}");
    }
}

/// A character's stored attribute levels (`AgentAttributes`, character +0x394) already hold his
/// traits' and ancillaries' attribute effects (`0x00A0ECB0` / `0x00A0EC10` add them as they are
/// gained): on the vanilla start position every character's levels are his type's starting levels
/// plus those effects (CHARACTERS_FIDELITY.md §16). One exception: a rake (749615820) holds `C_Rake_Counter_Spy` at 6
/// points (level 2, `subterfuge` +2) with his starting `subterfuge` 2, as the start position was
/// written; the levels are loaded as stored (the loader runs no gain slot), so he plays at 2 there.
#[test]
fn stored_attributes_are_the_start_levels_plus_the_effects() {
    let Some(m) = model() else { return };
    let fx = &m.rules.effects;
    let mut checked = 0;
    let mut wrong = Vec::new();
    for (id, ch) in &m.world.characters {
        let Some(d) = m.world.character_details.get(id) else { continue };
        let mut expected = m.rules.characters.starting_attributes(ch.kind.esf_name());
        let mut add = |rows: Option<&Vec<(String, i32)>>| {
            for (attr, v) in rows.into_iter().flatten() {
                match expected.iter_mut().find(|(k, _)| k == attr) {
                    Some((_, l)) => *l += v,
                    None => expected.push((attr.clone(), -1 + v)),
                }
            }
        };
        for t in &d.traits {
            add(fx.held_level_key(&t.key, t.level).and_then(|k| fx.trait_attribute.get(k)));
        }
        for a in &d.ancillaries {
            add(fx.ancillary_attribute.get(a));
        }
        let mut stored = d.attributes.clone();
        stored.sort();
        expected.sort();
        if stored != expected {
            wrong.push((id.0, ch.kind.esf_name(), stored, expected));
        }
        checked += 1;
    }
    assert_eq!(checked, 531);
    assert_eq!(wrong.iter().map(|w| (w.0, w.1)).collect::<Vec<_>>(), vec![(749615820, "rake")], "{wrong:?}");
}

/// `agent_attribute_situations` {situation key, attribute}: loader `0x00DCC540` (row reader
/// `0x00E55090`) hashes record +0, column 0.
const SITUATIONS: RawTable = RawTable { name: "agent_attribute_situations", codes: "s,s", key: RowKey::Concat(&[0]) };

/// The situation effect tables {level or ancillary key, situation key, i32 value}, and
/// `trait_attribute_effects`, which the exe reads the same way: all three name getters (`0x00F098E0`,
/// `0x00F95160`, `0x00F09890`) pass row reader thunk `0x00DE2610` (reader `0x00DD2250`: column 0 at
/// record +0, column 1 at +0xC, the i32 at +0x18) to loader `0x00DBDB90`, which hashes record +0 joined
/// with record +0xC (`0x004F1200` at `0x00DBE511`): the key is columns 0 + 1.
const fn level_situation_table(name: &'static str) -> RawTable {
    RawTable { name, codes: "s,s,i", key: RowKey::Concat(&[0, 1]) }
}

/// One table's rows through the merged view (every file, the exe's row rule and key); the install
/// must ship it and every file must decode.
fn merged(vfs: &Vfs, table: RawTable) -> Vec<Vec<DbValue>> {
    let mut warnings = Vec::new();
    let rows = table.read_with(vfs, &mut warnings).unwrap_or_else(|e| panic!("{}: {e}", table.name));
    assert!(warnings.is_empty(), "{}: {warnings:?}", table.name);
    rows
}

/// The attribute situation tables are Empire's and give no attribute or rank in Napoleon
/// (CHARACTERS_FIDELITY.md §17). `agent_attribute_situations` (16 rows, getter `0x00E0E4C0`) is only
/// loaded: its two callers are the database loaders. The effect tables
/// `trait_attribute_situation_effects` (into the trait level's +0x88 list, `0x00F72B80`) and
/// `ancillary_to_attribute_situation_effects` (`0x00F944C0`) ship with no rows, and neither the trait
/// nor the ancillary gain (`0x00A0ECB0` / `0x00A0EC10`) nor `GetCharacterRank` `0x00A198D0` reads
/// them; the rank's situation bonuses are its own hardcoded effect bonuses (`agents::rank_original`).
#[test]
fn vanilla_attribute_situation_tables_feed_nothing() {
    let dir = data_dir();
    if !dir.is_dir() {
        eprintln!("skipped: no install at {}", dir.display());
        return;
    }
    let vfs = Vfs::open_install(&dir).expect("vfs");
    let mut pairs: Vec<(String, String)> = merged(&vfs, SITUATIONS)
        .iter()
        .map(|r| (r[0].as_str().unwrap_or("").to_owned(), r[1].as_str().unwrap_or("").to_owned()))
        .collect();
    pairs.sort();
    let land = ["Ambushes", "Artillery_Troops", "Attack", "Cavalry_Troops", "Command_Amphibious_Attack", "Defence", "In_Americas", "In_India", "Infantry_Troops", "Siege_Attack", "Siege_Defence"];
    let mut expected: Vec<(String, String)> = land.iter().map(|k| (format!("Command_Land_{k}"), "command_land".to_string())).collect();
    expected.extend(["Attack", "Defence"].iter().map(|k| (format!("Command_Sea_{k}"), "command_sea".to_string())));
    expected.extend(["Artillery", "Cavalry", "Infantry"].iter().map(|k| (format!("Movement_Points_Land_{k}"), "movement_points_land".to_string())));
    expected.sort();
    assert_eq!(pairs, expected);
    for table in ["trait_attribute_situation_effects", "ancillary_to_attribute_situation_effects"] {
        let rows = merged(&vfs, level_situation_table(table));
        assert!(rows.is_empty(), "{table}: {rows:?}");
    }
}

/// The situation effect tables' `ssi` layout, which their own empty files cannot show: the exe reads
/// them with the same row reader (`0x00DD2250`) and loader (`0x00DBDB90`) as `trait_attribute_effects`,
/// whose 157 shipped rows read in full with that layout (a key, one of the attributes, a small level
/// bonus), and a row merges with a later file's row only on the same level and situation key.
#[test]
fn the_situation_effect_layout_reads_its_sibling_table() {
    use ntw_sim::campaign::agents::ATTRIBUTES;
    let dir = data_dir();
    if !dir.is_dir() {
        eprintln!("skipped: no install at {}", dir.display());
        return;
    }
    let vfs = Vfs::open_install(&dir).expect("vfs");
    let rows = merged(&vfs, level_situation_table("trait_attribute_effects"));
    assert_eq!(rows.len(), 157);
    for r in &rows {
        assert_eq!(r.len(), 3, "{r:?}");
        assert!(r[0].as_str().is_some_and(|k| !k.is_empty()), "{r:?}");
        assert!(r[1].as_str().is_some_and(|a| ATTRIBUTES.contains(&a)), "{r:?}");
        assert!(r[2].as_i32().is_some_and(|v| (-10..=10).contains(&v) && v != 0), "{r:?}");
    }
    // One level with two attributes keeps both rows: the key is not the level alone.
    let row = |l: &str, s: &str| vec![DbValue::Str(l.into()), DbValue::Str(s.into()), DbValue::I32(1)];
    let key = level_situation_table("x").key;
    assert_ne!(key.of(&row("lvl", "Command_Land_Attack")), key.of(&row("lvl", "Command_Land_Defence")));
}

/// `CHARACTER_DETAILS` #14 `AgentAttributeBonuses` is the one per-character situation slot (details
/// +0x13C): the character maker sets it to 0 (`0x00992E60`, at `0x00992F5D`), the copy keeps it, and
/// the save writer `0x009A0580` writes exactly one entry, `dummy` (the only key of the situation
/// list at `0x0145DA24`) with that value. Nothing else reads or writes it, so every start-position
/// character holds `[("dummy", 0)]`.
#[test]
fn vanilla_attribute_bonuses_are_one_dummy_zero() {
    let Some(m) = model() else { return };
    let odd: Vec<_> = m
        .world
        .character_details
        .iter()
        .filter(|(_, d)| d.attribute_bonuses != [("dummy".to_string(), 0)])
        .map(|(id, d)| (id.0, d.attribute_bonuses.clone()))
        .collect();
    assert!(!m.world.character_details.is_empty());
    assert!(odd.is_empty(), "{odd:?}");
}

/// A character made in play starts with the maker's `[("dummy", 0)]` (`0x00992F5D`), and the save our
/// writer builds for him (his `CHARACTER` cloned from a template minister) reads back with it.
#[test]
fn a_new_characters_attribute_bonuses_survive_the_save() {
    use ntw_sim::campaign::CampaignCommand;
    let Some(mut m) = model() else { return };
    let dir = data_dir();
    let britain = m.faction_by_key("britain").unwrap().id;
    let minister = m.world.faction_details[&britain]
        .posts
        .iter()
        .filter(|p| p.key != "faction_leader" && p.governorship.is_none())
        .find_map(|p| p.holder)
        .expect("a British minister");
    let before: std::collections::BTreeSet<CharacterId> = m.world.characters.keys().copied().collect();
    m.apply(CampaignCommand::DismissMinister { minister }).unwrap();
    let new: Vec<CharacterId> = m.world.characters.keys().copied().filter(|c| !before.contains(c)).collect();
    assert_eq!(new.len(), 1, "Britain makes one new minister");
    let dummy = vec![("dummy".to_string(), 0)];
    assert_eq!(m.world.character_details[&new[0]].attribute_bonuses, dummy);
    let db = GameDatabase::from_install(&dir).expect("DB");
    let bytes = GameFiles { vfs: &Vfs::open_install(&dir).unwrap() }.read("campaigns/eur_napoleon/startpos.esf").unwrap();
    let out = ntw_campaign::save::write_save(&EsfFile::from_bytes(&bytes).unwrap(), &m, "france", 1, None).unwrap();
    let back = ntw_campaign::read_esf(&EsfFile::from_bytes(&out.to_bytes().unwrap()).unwrap(), &db).unwrap();
    assert_eq!(back.model.world.character_details[&new[0]].attribute_bonuses, dummy);
}

/// The battle card the campaign builds for each unit (`0x008AF200`, CHARACTERS_FIDELITY.md §16,
/// BATTLE_FIDELITY.md §11) on the eur start: a commander's own unit is his army's general at his
/// rank, and no other unit of a land army is.
#[test]
fn battle_cards_carry_the_commanders_rank() {
    use ntw_sim::campaign::{agents, BattleCommandRole};
    let Some(m) = model() else { return };
    let mut checked = 0;
    for f in m.world.forces.values().filter(|f| !f.is_navy) {
        for u in &f.units {
            let card = m.battle_general_card(f, u);
            let Some(c) = u.character else {
                assert_eq!((card.role, card.rank), (BattleCommandRole::None, 0), "force {:?} unit {:?}", f.id, u.id);
                continue;
            };
            assert_eq!(card.rank, agents::rank(&m, c), "force {:?} unit {:?}", f.id, u.id);
            assert_eq!(card.army_general_rank().is_some(), f.commander == Some(c) || f.commander.is_none(), "force {:?} unit {:?}", f.id, u.id);
            checked += 1;
        }
    }
    assert!(checked > 50, "{checked} character units");
    // Napoléon's bodyguard: the army's general at his rank.
    let napoleon = m
        .world
        .character_details
        .iter()
        .find(|(_, d)| d.forename == "names_name_names_frenchNapol\u{e9}on" && d.surname == "names_name_names_frenchBonaparte")
        .map(|(c, _)| *c)
        .expect("Napoleon");
    let f = m.world.forces.values().find(|f| f.commander == Some(napoleon)).expect("his army");
    let u = f.units.iter().find(|u| u.character == Some(napoleon)).expect("his unit");
    let rank = agents::rank(&m, napoleon);
    assert_eq!(m.battle_general_card(f, u).army_general_rank(), Some(rank));
    assert!((0..=9).contains(&rank));
}
