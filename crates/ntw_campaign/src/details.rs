//! Reads the detail records into [`ntw_sim::campaign::details`]: `CHARACTER_DETAILS`, the
//! `GOVERNMENT` posts and governorships (with the taxes), the capital and religion, and the full
//! `DIPLOMACY_RELATIONSHIP` records. Layouts: `analysis/campaign/CAMPAIGN_DATA.md` §3.
//!
//! Lenient by design: a detail that does not have the expected shape is skipped (the core loader
//! already checked everything the rules depend on), so an odd modded file still loads.

use ntw_formats::esf::{EsfNode, EsfRecord};
use ntw_sim::calendar::Date;
use ntw_sim::campaign::details::{
    AlliedWar, AttitudeFactor, CharacterDetails, CharacterTrait, FactionDetails, GovernmentPost, Governorship,
    GovernorshipTaxes, Portrait, RegularPayment, Relationship,
};
use ntw_sim::campaign::{CharacterId, FactionId, RegionId};

fn date_of(r: Option<&EsfRecord>) -> Option<Date> {
    let r = r?;
    Some(Date { year: r.get_u32(0)?, season: r.get_u32(1)?, month: r.get_u32(2)?, half: r.get_u32(3)? })
}

fn loc(r: Option<&EsfNode>) -> String {
    r.and_then(EsfNode::as_record).and_then(|l| l.get_str(0)).unwrap_or_default().to_string()
}

fn items<'a>(r: &'a EsfRecord, name: &str) -> impl Iterator<Item = &'a [EsfNode]> + 'a {
    r.record_array(name).into_iter().flat_map(|a| a.items.iter().map(Vec::as_slice))
}

fn s(n: Option<&EsfNode>) -> String {
    n.and_then(EsfNode::as_str).unwrap_or_default().to_string()
}

/// Two `CAMPAIGN_MODEL` children the characters area reads (CHARACTERS_FIDELITY.md §7, §8):
/// `HISTORICAL_CHARACTER_MANAGER/CREATED_CHARACTER_ARRAY[]` {utf16 key} (the historical characters
/// already made; kept sorted as the exe's list is), and `EPISODIC_RESTRICTIONS` #20 / #22 (the
/// `force_assassination_success_for_human` / `force_garrison_infiltration_success_for_human`
/// switches; CONFIRMED positions from the loader `0x009968A0`, both false in every vanilla save).
pub(crate) fn model_extras(model_rec: &EsfRecord) -> (Vec<String>, (bool, bool)) {
    let mut created: Vec<String> = model_rec
        .child("HISTORICAL_CHARACTER_MANAGER")
        .map(|m| items(m, "CREATED_CHARACTER_ARRAY").filter_map(|it| it.first()?.as_str().map(str::to_string)).collect())
        .unwrap_or_default();
    created.sort();
    let switches = model_rec
        .child("EPISODIC_RESTRICTIONS")
        .map(|e| (e.get_bool(20).unwrap_or(false), e.get_bool(22).unwrap_or(false)))
        .unwrap_or((false, false));
    (created, switches)
}

/// `CHARACTER` → its details (`CHARACTER_DETAILS` v3 at #1, post id at #8).
pub(crate) fn character_details(ch: &EsfRecord) -> Option<CharacterDetails> {
    let d = ch.get(1)?.as_record().filter(|r| r.name == "CHARACTER_DETAILS")?;
    let rec = |i: usize| d.get(i).and_then(EsfNode::as_record);
    let traits = rec(0)
        .map(|t| {
            items(t, "TRAIT")
                .filter_map(|it| Some(CharacterTrait { key: it.first()?.as_str()?.to_string(), points: it.get(1)?.as_i32()? }))
                .collect()
        })
        .unwrap_or_default();
    let other = rec(3).map(|l| (s(l.get(0)), s(l.get(1)))).unwrap_or_default();
    let portrait = rec(8)
        .filter(|p| p.name == "PORTRAIT_DETAILS")
        .map(|p| Portrait { card: s(p.get(0)), alternative: s(p.get(1)), info: s(p.get(2)), index: p.get_i32(3).unwrap_or(0) })
        .unwrap_or_default();
    Some(CharacterDetails {
        forename: loc(d.get(1)),
        surname: loc(d.get(2)),
        other_names: other,
        unknown_4: s(d.get(4)),
        birth: date_of(rec(5)),
        date_2: date_of(rec(6)),
        portrait,
        faction_key: s(d.get(9)),
        traits,
        ancillaries: items(d, "AgentAncillaries").filter_map(|it| Some(it.first()?.as_str()?.to_string())).collect(),
        attributes: items(d, "AgentAttributes")
            .filter_map(|it| Some((it.first()?.as_str()?.to_string(), it.get(1)?.as_i32()?)))
            .collect(),
        abilities: items(d, "AgentAbilities")
            .filter_map(|it| Some((it.first()?.as_str()?.to_string(), it.get(1)?.as_i32()?, s(it.get(2)))))
            .collect(),
        attribute_bonuses: items(d, "AgentAttributeBonuses")
            .filter_map(|it| Some((it.first()?.as_str()?.to_string(), it.get(1)?.as_u32()?)))
            .collect(),
        onscreen_name: loc(d.get(16)),
        post: ch.get_u32(8).unwrap_or(0),
        returns_after_death: ch.get_bool(34).unwrap_or(false),
        duels_lost: ch.get_u32(36).unwrap_or(0),
        duels_won: ch.get_u32(37).unwrap_or(0),
        hidden: ch.get_bool(22).unwrap_or(false),
        turns_at_sea: ch.get_u32(28).unwrap_or(0),
        turns_in_enemy_lands: ch.get_u32(29).unwrap_or(0),
        turns_at_home: ch.get_u32(30).unwrap_or(0),
        no_action: ch.get_bool(14).unwrap_or(false),
        idle_turns: ch.get_u32(15).unwrap_or(0),
        fled: ch.get_bool(27).unwrap_or(false),
        wounded: false,
        historical_key: None,
    })
}

/// `FACTION` → its details, and the governorship taxes as `taxes_levels` keys (lower, upper).
pub(crate) fn faction_details(f: &EsfRecord) -> (FactionDetails, Option<(String, String)>) {
    let plain: Vec<&EsfNode> = f.values().collect();
    let mut d = FactionDetails { display_name: s(plain.get(2).copied()), ..Default::default() };
    d.exposed = items(f, "EXPOSED_CHARACTERS").filter_map(|it| it.first()?.as_i32()).map(CharacterId).collect();
    if let Some(g) = f.child("GOVERNMENT") {
        d.government_id = g.get_i32(0).unwrap_or(0);
        for it in items(g, "POSTS_ARRAY") {
            let Some(p) = it.first().and_then(EsfNode::as_record).filter(|r| r.name == "CHARACTER_POST") else { continue };
            let holder = p.get_u32(2).filter(|&h| h != 0).map(|h| CharacterId(h as i32));
            let governorship = p.child("GOVERNORSHIP").map(read_governorship);
            d.posts.push(GovernmentPost { id: p.get_i32(0).unwrap_or(0), key: s(p.get(1)), holder, governorship });
        }
    }
    // Capital: the i32 values right after the last FORT_UPGRADE_MANAGER (see FactionDetails).
    if let Some(last) = f.children.iter().rposition(|c| c.record_name() == Some("FORT_UPGRADE_MANAGER")) {
        let region = |i: usize| f.get(i).and_then(EsfNode::as_i32).filter(|&v| v != 0).map(|v| RegionId(v as u32));
        d.capital = region(last + 1);
        d.capital_2 = region(last + 2);
    }
    d.religion = f.values().filter_map(EsfNode::as_str).find(|v| v.starts_with("rel_") || v.starts_with("align_")).unwrap_or_default().to_string();
    // Trade income of the last ECONOMICS_DATA record (category 7 = #1[2], CAMPAIGN_FIDELITY.md).
    d.stored_trade_income = f
        .child("FACTION_ECONOMICS")
        .and_then(|e| e.get(0))
        .and_then(EsfNode::as_record_array)
        .and_then(|h| h.items.last())
        .and_then(|it| it.first())
        .and_then(EsfNode::as_record)
        .and_then(|d| d.get(1))
        .and_then(EsfNode::as_i32_array)
        .and_then(|a| a.get(2).copied());
    // The bool right before CHARACTER_ARRAY: the major-power flag (faction +0x524, CAMPAIGN_FIDELITY.md).
    if let Some(i) = f.children.iter().position(|c| c.record_name() == Some("CHARACTER_ARRAY")) {
        d.major = i.checked_sub(1).and_then(|p| f.children[p].as_bool());
    }
    d.family = f.child("FAMILY").and_then(read_family);
    // Technologies (EFFECTS_FIDELITY.md §3) and the campaign difficulty (slot 0-F).
    if let Some(tm) = f.child("FACTION_TECHNOLOGY_MANAGER") {
        d.technologies = items(tm, "techs").filter_map(|it| Some((it.first()?.as_str()?.to_string(), it.get(1)?.as_u32()?))).collect();
        // #2 f32 progress, #3 u32 the researching school slot (CAMPAIGN_FIDELITY.md §Research).
        d.research = items(tm, "techs")
            .filter_map(|it| {
                let key = it.first()?.as_str()?.to_string();
                let progress = it.get(2).and_then(EsfNode::as_f32).unwrap_or(0.0);
                let researcher = it.get(3).and_then(EsfNode::as_u32).unwrap_or(0);
                (progress != 0.0 || researcher != 0).then_some((key, ntw_sim::campaign::details::TechResearch { progress, researcher }))
            })
            .collect();
    }
    d.difficulty = f
        .child("CAMPAIGN_PLAYER_SETUP")
        .and_then(|s| s.child("CAMPAIGN_PLAYER_SETUP_INGAME_MODIFIABLES"))
        .and_then(|m| m.values().find_map(EsfNode::as_i32))
        .unwrap_or(0);
    // The recruitment pools (FACTION #75 `CHARACTER_RECRUITMENT_MANAGER`, slot 0-G).
    if let Some(m) = f.child("CHARACTER_RECRUITMENT_MANAGER") {
        let pool = |name: &str| {
            m.child(name)
                .map(|p| {
                    let ids = p.get(0).and_then(EsfNode::as_u32_array).map(|a| a.iter().map(|&x| CharacterId(x as i32)).collect()).unwrap_or_default();
                    (ids, p.get_u32(1).unwrap_or(0))
                })
                .unwrap_or_default()
        };
        d.general_pool = pool("GENERAL_RECRUITMENT");
        d.admiral_pool = pool("ADMIRAL_RECRUITMENT");
    }
    // The two saved effect containers (#54 base, #55 base + difficulty; EFFECTS_FIDELITY.md §1).
    let mut bonus = f.children.iter().filter_map(EsfNode::as_record).filter(|r| r.name == "CAMPAIGN_BONUS_VALUES").map(saved_bonuses);
    d.bonus_base = bonus.next().unwrap_or_default();
    d.bonus_with_difficulty = bonus.next().unwrap_or_default();
    let taxes = d.governorship().and_then(|g| {
        Some((
            GovernorshipTaxes::level_key(g.taxes.lower)?.to_string(),
            GovernorshipTaxes::level_key(g.taxes.upper)?.to_string(),
        ))
    });
    (d, taxes)
}

fn read_governorship(g: &EsfRecord) -> Governorship {
    let t = g.child("GOVERNORSHIP_TAXES");
    let u8_at = |i: usize| match t.and_then(|t| t.get(i)) {
        Some(EsfNode::U8(v)) => *v,
        _ => 0,
    };
    Governorship {
        taxes: GovernorshipTaxes {
            lower: t.and_then(|t| t.get_u32(0)).unwrap_or(2),
            upper: t.and_then(|t| t.get_u32(1)).unwrap_or(2),
            lower_rate: u8_at(2),
            upper_rate: u8_at(3),
        },
        theatre_id: g.get_i32(1).unwrap_or(0),
        regions: g.get(2).and_then(EsfNode::as_u32_array).map(|a| a.iter().map(|&r| RegionId(r)).collect()).unwrap_or_default(),
        faction: FactionId(g.get_u32(3).unwrap_or(0) as i32),
        flags: (g.get_bool(4).unwrap_or(false), g.get_bool(5).unwrap_or(false)),
    }
}

/// `DIPLOMACY_RELATIONSHIP` v14 → (target, relationship). Field meanings:
/// `analysis/campaign/S1_LEFTOVERS.md` §1 (the exe's reader `0x00AE9480` / writer `0x00AFC340`).
pub(crate) fn relationship(r: &EsfRecord) -> Option<(FactionId, Relationship)> {
    let target = FactionId(r.get_i32(0)?);
    // Every plain field is read by its index with whatever integer type it has, so a value of
    // another width (an odd modded file) still loads.
    let int = |i: usize| r.get(i).and_then(EsfNode::as_int).unwrap_or(0);
    let i32_at = |i: usize| int(i) as i32;
    let u32_at = |i: usize| int(i) as u32;
    let bool_at = |i: usize| r.get_bool(i).unwrap_or(false);
    let array = |i: usize| r.get(i).and_then(EsfNode::as_record_array).map(|a| a.items.as_slice()).unwrap_or_default();
    let item_int = |it: &[EsfNode], k: usize| it.get(k).and_then(EsfNode::as_int).unwrap_or(0);
    let item_bool = |it: &[EsfNode], k: usize| it.get(k).and_then(EsfNode::as_bool).unwrap_or(false);
    let attitudes = array(1)
        .iter()
        .map(|it| AttitudeFactor {
            drift: item_int(it, 0) as i32,
            value: item_int(it, 1) as i32,
            limit: item_int(it, 2) as i32,
            limited: item_bool(it, 3),
            cap: item_int(it, 4) as i32,
            capped: item_bool(it, 5),
        })
        .collect();
    let payments = array(14)
        .iter()
        .map(|it| RegularPayment { amount: item_int(it, 0) as i32, turns: item_int(it, 1) as u32 })
        .collect();
    let allied_in_war_against = array(17)
        .iter()
        .map(|it| AlliedWar { enemy: FactionId(item_int(it, 0) as i32), saved_access_turns: item_int(it, 1) as i32 })
        .collect();
    let mut diplomacy_options = [0u32; 14];
    if let Some(a) = r.get(18).and_then(EsfNode::as_u32_array) {
        for (o, v) in diplomacy_options.iter_mut().zip(a) {
            *o = *v;
        }
    }
    let war_ally = Some(i32_at(5)).filter(|&f| f != 0).map(FactionId);
    Some((
        target,
        Relationship {
            attitudes,
            trade_agreement: bool_at(2),
            military_access_turns: i32_at(3),
            war_ally,
            alliance_commitment_turns: u32_at(6),
            war_momentum: i32_at(7),
            protectorate_tribute: i32_at(8),
            protectorate_income: i32_at(9),
            war_region_balance: i32_at(10),
            war_wealth_balance: i32_at(11),
            war_turns: u32_at(12),
            turns_since_battle: u32_at(13),
            payments,
            friendship_turns: u32_at(15),
            unknown_16: u32_at(16),
            allied_in_war_against,
            diplomacy_options,
            military_access_streak: u32_at(19),
            previous_stance: s(r.get(20)),
            unknown_21: bool_at(21),
            unknown_22: bool_at(22),
            start_attitude: i32_at(23),
            military_access_granted: i32_at(24),
            military_access_elapsed: u32_at(25),
            access_cancel_grievance: u32_at(26),
            trade_embargo_turns: u32_at(27),
            // Version < 14 files have no #28; the exe's default is true (CONFIRMED in its reader).
            allows_region_return: r.get_bool(28).unwrap_or(true),
        },
    ))
}

/// `CAMPAIGN_BONUS_VALUES` → its entries (`CAMPAIGN_BONUS_VALUE_BLOCK[]` items each holding one
/// `CAMPAIGN_BONUS_VALUE` {u32 type, i32 bonus, f32 value, utf16 qualifier (optional)}; reader `0x00DF14C0`).
fn saved_bonuses(r: &EsfRecord) -> Vec<ntw_sim::campaign::effects::SavedBonus> {
    items(r, "CAMPAIGN_BONUS_VALUE_BLOCK")
        .filter_map(|it| {
            let v = it.first()?.as_record()?;
            Some(ntw_sim::campaign::effects::SavedBonus {
                kind: v.get_u32(0)?,
                bonus: v.get_i32(1)?,
                value: v.get_f32(2)?,
                qualifier: v.get_str(3).unwrap_or_default().to_string(),
            })
        })
        .collect()
}

/// The strings of a `CAMPAIGN_LOCALISATION` record (one or two).
fn loc_strings(r: Option<&EsfNode>) -> Vec<String> {
    r.and_then(EsfNode::as_record).map(|l| l.children.iter().filter_map(EsfNode::as_str).map(str::to_string).collect()).unwrap_or_default()
}

/// `FACTION` `FAMILY` v2: ten `FAMILY::MONARCHY_INFO_CHARACTER`, a u8 (the heir child's index),
/// `ORDINAL_PAIR[]` {loc, i32} (CONFIRMED: loader `0x0087D490`, member loader `0x008C2390`).
pub(crate) fn read_family(fam: &EsfRecord) -> Option<ntw_sim::campaign::family::Family> {
    use ntw_sim::campaign::family::{Family, FamilyMember, FIRST_CHILD};
    let members: Vec<FamilyMember> = fam
        .children_named("FAMILY::MONARCHY_INFO_CHARACTER")
        .map(|r| {
            let int = |i: usize| r.get(i).and_then(EsfNode::as_int).unwrap_or(0);
            let married_to = int(14) as u32;
            FamilyMember {
                names: loc_strings(r.get(0)),
                male: int(1) != 0,
                exists: int(2) != 0,
                children: int(3) as u8,
                trait_4: int(4) as i32,
                age: int(5) as i32,
                regnal: int(6) as i32,
                child_ages: [int(7) as i32, int(8) as i32, int(9) as i32, int(10) as i32],
                portrait: r
                    .get(11)
                    .and_then(EsfNode::as_record)
                    .map(|p| Portrait { card: s(p.get(0)), alternative: s(p.get(1)), info: s(p.get(2)), index: p.get_i32(3).unwrap_or(0) })
                    .unwrap_or_default(),
                religion: s(r.get(12)),
                owner: int(13) as u32,
                married_to,
                spouse_names: if married_to != 0 { loc_strings(r.get(15)) } else { Vec::new() },
                heir: false,
            }
        })
        .collect();
    if members.len() != 10 {
        return None;
    }
    let mut out = Family { members, ordinals: Vec::new(), female_heirs: false };
    let heir = fam.values().find_map(|v| match v {
        EsfNode::U8(x) => Some(*x),
        _ => None,
    });
    if let Some(h) = heir.filter(|&h| h < 4) {
        out.members[FIRST_CHILD + usize::from(h)].heir = true;
    }
    out.ordinals = items(fam, "ORDINAL_PAIR")
        .filter_map(|it| Some((loc_strings(it.first()).into_iter().next().unwrap_or_default(), it.get(1)?.as_i32()?)))
        .collect();
    Some(out)
}
