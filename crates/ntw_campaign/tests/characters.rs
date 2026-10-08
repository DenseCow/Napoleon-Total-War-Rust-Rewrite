//! Characters rules against the install (slot 0-G, `analysis/fidelity/CHARACTERS_FIDELITY.md`): the
//! trait and ancillary rules on real DB data, and the yearly natural-death pass. Skipped without an
//! install.

use std::path::PathBuf;

use ntw_data::GameDatabase;
use ntw_formats::campaign_map::GameFiles;
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
    let bytes = GameFiles { vfs: &vfs, data_dir: Some(&dir) }.read("campaigns/eur_napoleon/startpos.esf").expect("startpos");
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
    let bytes = GameFiles { vfs: &Vfs::open_install(&dir).unwrap(), data_dir: Some(&dir) }.read("campaigns/eur_napoleon/startpos.esf").unwrap();
    let src = EsfFile::from_bytes(&bytes).unwrap();
    let out = ntw_campaign::save::write_save(&src, &m, "france", 1).unwrap();
    let back = EsfFile::from_bytes(&out.to_bytes().unwrap()).unwrap();
    let r = ntw_campaign::save_check::check(&back);
    assert!(r.violations.is_empty(), "{:?}", r.violations);
    let l = ntw_campaign::read_esf(&back, &db).unwrap();
    let bd = &l.model.world.faction_details[&austria];
    assert_eq!(bd.leader(), Some(new_leader));
    assert_eq!(bd.family, d.family);
    assert_eq!(l.model.world.character_details[&new_leader].forename, heir.names[0]);
    assert_eq!(l.model.world.character_details[&refilled].post, d.posts[minister_post].id as u32);
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
    let bytes = GameFiles { vfs: &Vfs::open_install(&dir).unwrap(), data_dir: Some(&dir) }.read("campaigns/eur_napoleon/startpos.esf").unwrap();
    let src = EsfFile::from_bytes(&bytes).unwrap();
    let out = ntw_campaign::save::write_save(&src, &m, "france", 1).unwrap();
    let back = EsfFile::from_bytes(&out.to_bytes().unwrap()).unwrap();
    let r = ntw_campaign::save_check::check(&back);
    assert!(r.violations.is_empty(), "{:?}", r.violations);
}
