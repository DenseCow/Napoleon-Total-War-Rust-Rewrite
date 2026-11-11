//! The human player's campaign victory conditions in a save (SAVE_COMPAT.md §17).
//!
//! `CAMPAIGN_VICTORY_CONDITIONS` v5 (reader `0x00903DA0`, CONFIRMED layout): #0 `REGION_KEYS[]`
//! (regions that must be held), #1 bool met (+0x31, set when the victory fires), #2 u32 (+0x98,
//! default 24), #3 `DATE` deadline (+0x34, year 0 = none), #4 u32 regions to hold (+0x44), #5 bool
//! (+0x48, an extra test), #6 u32 type (+0x7C, 6 = none), #7 bool (+0x30), #8 bool failed (+0x32),
//! #9 u32 (+0x80), #10 `DATE` (+0x84).
//!
//! A save holds them three times for the human: in its `CAMPAIGN_SETUP` players entry, in its
//! `FACTION/CAMPAIGN_PLAYER_SETUP`, and in the `FACTION`'s own record (the playable factions only),
//! which is the one the game tests (CONFIRMED: the original's `auto_save` after three End Turns of
//! our NR-4 has France's faction-level record marked met). A start position holds the default
//! (type 5, no regions) everywhere; the original's front end copies the chosen option from the
//! start position's `CAMPAIGN_PREOPEN_MAP_INFO/VICTORY_CONDITION_OPTIONS` (per playable faction,
//! a `VICTORY_CONDITIONS_BLOCK[]` of three options) into those records (CONFIRMED shape: the
//! user's Coalition saves hold Britain's option in all three places, the other factions keep the
//! default).
//!
//! **Why it matters (CONFIRMED in the exe).** The test `0x0096FCE0` (called for the human, faction
//! +0x6E0, whenever the record's type is not 6) treats type 5 as the Peninsular campaign's rule: it
//! looks up the faction `spa_france` and, when that faction does not exist, counts the conditions
//! as met as long as every listed region is held, which an empty list always is. So the start
//! position's default makes the original declare a campaign victory ("Supreme Victory", then
//! "end the campaign or continue") as soon as it runs the test: the user's NR-B2 / NR-B3 at load.
//!
//! The front end gives a new campaign's human the first option (CONFIRMED: the original's own
//! turn-1 save of a new spa_napoleon campaign holds `spa_france`'s option 0 in exactly the three
//! records; for eur_napoleon the same rule is INFERRED).

use ntw_formats::esf::{EsfFile, EsfNode, EsfRecord};
use ntw_sim::campaign::outcome::VictoryConditions;

/// The record name.
pub const RECORD: &str = "CAMPAIGN_VICTORY_CONDITIONS";
/// The type value whose rule belongs to the Peninsular campaign (CONFIRMED `0x0096FCE0`).
pub const TYPE_PENINSULAR: u32 = 5;
/// "No conditions": the test is skipped (CONFIRMED caller `0x008F3DF0`).
pub const TYPE_NONE: u32 = 6;
/// The faction the type-5 rule looks up (CONFIRMED string in `0x0096FCE0`).
pub const PENINSULAR_FACTION: &str = "spa_france";
/// The option a new campaign gets (CONFIRMED on spa_napoleon, see the module notes).
pub const DEFAULT_OPTION: usize = 0;

/// The victory condition options of `faction` in a start position tree (`None` for a save, or a
/// faction without options).
pub fn options(startpos: &EsfRecord, faction: &str) -> Option<Vec<EsfRecord>> {
    let info = startpos.child("CAMPAIGN_PREOPEN_MAP_INFO")?;
    let opts = info.record_array("VICTORY_CONDITION_OPTIONS")?;
    let item = opts.items.iter().find(|it| it.first().and_then(EsfNode::as_str) == Some(faction))?;
    let blocks = item.iter().find_map(|n| match n {
        EsfNode::RecordArray(a) if a.name == "VICTORY_CONDITIONS_BLOCK" => Some(a),
        _ => None,
    })?;
    Some(blocks.items.iter().filter_map(|b| b.iter().find_map(|n| n.as_record().filter(|r| r.name == RECORD)).cloned()).collect())
}

/// Puts `conditions` into the human's three records (`env` = `CAMPAIGN_ENV`). Returns how many
/// records were replaced.
pub fn write_human(env: &mut EsfRecord, human: &str, conditions: &EsfRecord) -> usize {
    let mut n = 0;
    let mut put = |r: &mut EsfRecord| {
        for c in &mut r.children {
            if let EsfNode::Record(x) = c
                && x.name == RECORD
            {
                **x = conditions.clone();
                n += 1;
            }
        }
    };
    for c in &mut env.children {
        let EsfNode::Record(r) = c else { continue };
        match r.name.as_str() {
            "CAMPAIGN_SETUP" => {
                for p in players_mut(r) {
                    if p.get_str(2) == Some(human) {
                        put(p);
                    }
                }
            }
            "CAMPAIGN_MODEL" => {
                for m in r.children.iter_mut().filter_map(as_rec_mut).filter(|m| m.name == "WORLD") {
                    for a in m.children.iter_mut() {
                        let EsfNode::RecordArray(a) = a else { continue };
                        if a.name != "FACTION_ARRAY" {
                            continue;
                        }
                        for f in a.items.iter_mut().flat_map(|it| it.iter_mut()).filter_map(as_rec_mut) {
                            let mine = f.child("CAMPAIGN_PLAYER_SETUP").and_then(|p| p.get_str(2)) == Some(human);
                            if !mine {
                                continue;
                            }
                            put(f);
                            for p in f.children.iter_mut().filter_map(as_rec_mut).filter(|p| p.name == "CAMPAIGN_PLAYER_SETUP") {
                                put(p);
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }
    n
}

fn as_rec_mut(n: &mut EsfNode) -> Option<&mut EsfRecord> {
    match n {
        EsfNode::Record(r) => Some(r),
        _ => None,
    }
}

fn players_mut(setup: &mut EsfRecord) -> Vec<&mut EsfRecord> {
    let mut out = Vec::new();
    for c in setup.children.iter_mut().filter_map(as_rec_mut).filter(|r| r.name == "CAMPAIGN_PLAYERS_SETUP") {
        for a in c.children.iter_mut() {
            if let EsfNode::RecordArray(a) = a
                && a.name == "PLAYERS_ARRAY"
            {
                out.extend(a.items.iter_mut().flat_map(|it| it.iter_mut()).filter_map(as_rec_mut));
            }
        }
    }
    out
}

/// The type (#6) and region count of a conditions record.
pub fn kind(r: &EsfRecord) -> (u32, usize) {
    let regions = r.record_array("REGION_KEYS").map_or(0, |a| a.items.len());
    (r.get_u32(6).unwrap_or(TYPE_NONE), regions)
}

/// The human factions (faction-copy flag set) whose tested record (the faction-level one, else
/// the player-setup one) would count as met at once: type 5 in a world without `spa_france`, not
/// yet marked met. The original then declares a campaign victory (see the module notes).
pub fn met_at_once(esf: &EsfFile) -> Vec<String> {
    let Some(world) = esf.root.find_path("CAMPAIGN_ENV/CAMPAIGN_MODEL/WORLD") else { return Vec::new() };
    let factions: Vec<&EsfRecord> = world.record_array("FACTION_ARRAY").into_iter().flat_map(|a| a.records()).collect();
    let key = |f: &EsfRecord| f.child("CAMPAIGN_PLAYER_SETUP").and_then(|p| p.get_str(2)).unwrap_or("").to_string();
    let peninsular = factions.iter().any(|f| key(f) == PENINSULAR_FACTION);
    let mut out = Vec::new();
    for f in &factions {
        let Some(p) = f.child("CAMPAIGN_PLAYER_SETUP") else { continue };
        if !p.get_bool(3).unwrap_or(false) {
            continue;
        }
        let Some(c) = f.child(RECORD).or_else(|| p.child(RECORD)) else { continue };
        let (t, regions) = kind(c);
        if t == TYPE_PENINSULAR && !peninsular && regions == 0 && !c.get_bool(1).unwrap_or(false) {
            out.push(key(f));
        }
    }
    out
}

/// The region keys a conditions record names (`REGION_KEYS`, one string per item).
fn region_keys(r: &EsfRecord) -> impl Iterator<Item = &str> {
    r.record_array("REGION_KEYS").into_iter().flat_map(|a| a.items.iter()).filter_map(|it| it.first().and_then(EsfNode::as_str))
}

/// A faction's tested record (faction +0x724): the `FACTION`'s own, else its
/// `CAMPAIGN_PLAYER_SETUP`'s (the non-playable factions hold only that one).
fn tested(f: &EsfRecord) -> Option<&EsfRecord> {
    f.child(RECORD).or_else(|| f.child("CAMPAIGN_PLAYER_SETUP").and_then(|p| p.child(RECORD)))
}

/// [`tested`], writable.
fn tested_mut(f: &mut EsfRecord) -> Option<&mut EsfRecord> {
    let own = f.children.iter().position(|n| n.as_record().is_some_and(|r| r.name == RECORD));
    let i = match own {
        Some(i) => i,
        None => {
            let p = f.children.iter_mut().filter_map(as_rec_mut).find(|p| p.name == "CAMPAIGN_PLAYER_SETUP")?;
            return p.children.iter_mut().filter_map(as_rec_mut).find(|r| r.name == RECORD);
        }
    };
    as_rec_mut(&mut f.children[i])
}

/// The `DATE` record at child `i` of a conditions record.
fn date_at(r: &EsfRecord, i: usize) -> Result<ntw_sim::calendar::Date, String> {
    match r.children.get(i) {
        Some(EsfNode::Record(d)) if d.name == "DATE" => crate::fields::date(d, &format!("{RECORD}/#{i}")).map_err(|e| e.to_string()),
        _ => Err(format!("{RECORD} #{i} is not a DATE")),
    }
}

/// A conditions record as the model holds it ([`VictoryConditions`], fields #0..#10, layout in the
/// module notes). Region keys `region_ids` does not have are left out and counted in `unknown`.
pub fn parse(
    r: &EsfRecord,
    region_ids: &std::collections::HashMap<&str, ntw_sim::campaign::RegionId>,
    unknown: &mut usize,
) -> Result<VictoryConditions, String> {
    let field = |i: usize| -> Result<u32, String> { r.get_u32(i).ok_or_else(|| format!("{RECORD} #{i} is not a u32")) };
    let flag = |i: usize| -> Result<bool, String> { r.get_bool(i).ok_or_else(|| format!("{RECORD} #{i} is not a bool")) };
    let mut regions = Vec::new();
    for k in region_keys(r) {
        match region_ids.get(k) {
            Some(&id) => regions.push(id),
            None => *unknown += 1,
        }
    }
    Ok(VictoryConditions {
        regions,
        met: flag(1)?,
        unknown_2: field(2)?,
        deadline: date_at(r, 3)?,
        count: field(4)?,
        prestige_test: flag(5)?,
        kind: field(6)?,
        holding: flag(7)?,
        failed: flag(8)?,
        hold_turns: field(9)?,
        since: date_at(r, 10)?,
    })
}

/// The conditions [`crate::LoadedCampaign::set_human`] gives a new campaign's human, per faction
/// with options: its [`DEFAULT_OPTION`]'s record (a start position only; empty for a save).
pub type NewCampaignOptions = std::collections::BTreeMap<ntw_sim::campaign::FactionId, VictoryConditions>;

/// Fills [`World::victory_conditions`](ntw_sim::campaign::World::victory_conditions) from a start
/// position or save (`root` = the file's root record): for each faction, its tested record (the
/// `FACTION`'s own, else its `CAMPAIGN_PLAYER_SETUP`'s), as the exe's loader leaves faction +0x724.
/// Returns, for a start position, each faction's [`DEFAULT_OPTION`] record, which the front end copies
/// into the human's records ([`crate::LoadedCampaign::set_human`]; the module notes give the evidence
/// and its tag). A record that does not parse leaves its faction without conditions (no test); it and
/// region keys the world does not have are logged once per file.
pub fn fill(root: &EsfRecord, model: &mut ntw_sim::campaign::CampaignModel) -> NewCampaignOptions {
    let mut opts = NewCampaignOptions::new();
    let Some(world) = root.find_path("CAMPAIGN_ENV/CAMPAIGN_MODEL/WORLD") else { return opts };
    let region_ids: std::collections::HashMap<&str, ntw_sim::campaign::RegionId> = model.world.regions.values().map(|r| (r.key.as_str(), r.id)).collect();
    let mut out = std::collections::BTreeMap::new();
    let (mut unknown, mut broken) = (0usize, Vec::new());
    for f in world.record_array("FACTION_ARRAY").into_iter().flat_map(|a| a.records()).filter(|r| r.name == "FACTION") {
        let Some(key) = f.values().nth(1).and_then(EsfNode::as_str) else { continue };
        let Some(id) = model.faction_by_key(key).map(|x| x.id) else { continue };
        if let Some(r) = tested(f) {
            match parse(r, &region_ids, &mut unknown) {
                Ok(c) => {
                    out.insert(id, c);
                }
                Err(e) => broken.push(format!("{key}: {e}")),
            }
        }
        if let Some(opt) = options(root, key).and_then(|o| o.into_iter().nth(DEFAULT_OPTION)) {
            match parse(&opt, &region_ids, &mut unknown) {
                Ok(c) => {
                    opts.insert(id, c);
                }
                Err(e) => broken.push(format!("{key} option {DEFAULT_OPTION}: {e}")),
            }
        }
    }
    if !broken.is_empty() {
        log::warn!("victory conditions not read, those factions have none: {}", broken.join("; "));
    }
    if unknown > 0 {
        log::warn!("{unknown} victory condition region keys name no region of the world; left out");
    }
    model.world.victory_conditions = out;
    opts
}

/// Writes the fields the victory test changes (#1 met, #7 holding, #8 failed, #10 the hold's start)
/// from `c` into the faction's tested record (the one [`fill`] reads; `f` = the `FACTION`), as the
/// exe's writer saves faction +0x724. The other fields are data the test only reads.
pub fn write_state(f: &mut EsfRecord, c: &VictoryConditions) {
    let Some(r) = tested_mut(f) else { return };
    let mut set = |i: usize, v: EsfNode| {
        if let Some(slot) = r.children.get_mut(i)
            && slot.type_code() == v.type_code()
        {
            *slot = v;
        }
    };
    set(1, EsfNode::Bool(c.met));
    set(7, EsfNode::Bool(c.holding));
    set(8, EsfNode::Bool(c.failed));
    if let Some(EsfNode::Record(d)) = r.children.get_mut(10)
        && d.name == "DATE"
    {
        for (i, v) in [c.since.year, c.since.season, c.since.month, c.since.half].into_iter().enumerate() {
            if let Some(slot @ EsfNode::U32(_)) = d.children.get_mut(i) {
                *slot = EsfNode::U32(v);
            }
        }
    }
}
