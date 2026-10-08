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
