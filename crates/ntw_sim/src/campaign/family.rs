//! The royal family of a faction (`FACTION` `FAMILY`) and what happens when a post holder dies:
//! the yearly family pass, the succession to the faction leader and the new minister for a
//! vacated post. Read from the exe (slot 0-G); spec: `analysis/fidelity/CHARACTERS_FIDELITY.md`
//! §5c. Specs in our words; no decompiled code.
//!
//! The family object (`0x0087D490` loads it) holds ten members of 0x84 bytes: 0 the leader, 1 the
//! spouse, 2..=5 the leader's children, 6..=9 relatives who can inherit when no child does.

use super::details::Portrait;
use super::ids::{CharacterId, FactionId};
use super::portraits::CulturePortraits;
use super::world::{CampaignModel, GovernmentType};
use crate::rng::CaRng;

/// Member slot of the leader.
pub const LEADER: usize = 0;
/// Member slot of the leader's spouse.
pub const SPOUSE: usize = 1;
/// First of the four children slots.
pub const FIRST_CHILD: usize = 2;
/// First of the four relative slots.
pub const FIRST_RELATIVE: usize = 6;

/// One `FAMILY::MONARCHY_INFO_CHARACTER` v2 (CONFIRMED layout and object offsets from the loader
/// `0x008C2390` and the saver `0x00890490`).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct FamilyMember {
    /// #0 `CAMPAIGN_LOCALISATION` strings (+0x00 / +0x0C): the name, e.g.
    /// `names_royalty_name_austriaFranz`. A relative slot counts as filled when one is not empty.
    pub names: Vec<String>,
    /// #1 bool (+0x19): male (CONFIRMED use: king / queen messages 0xB2 / 0xB4, the female-heir rule).
    pub male: bool,
    /// #2 bool (+0x1A): alive / present (spouse and children slots).
    pub exists: bool,
    /// #3 u8 (+0x1B): number of children (0..=4).
    pub children: u8,
    /// #4 i32 (+0x58): 0..=2, drawn when a member is made and when a child turns 16 (meaning
    /// UNKNOWN).
    pub trait_4: i32,
    /// #5 i32 (+0x5C): age in years.
    pub age: i32,
    /// #6 i32 (+0x2C): regnal number (Christian VII: 7), from the family's ordinal list.
    pub regnal: i32,
    /// #7..#10 i32 (+0x1C..+0x28): the member's age when each child was born (not cleared when a
    /// slot is cleared, as in the saves).
    pub child_ages: [i32; 4],
    /// #11 `PORTRAIT_DETAILS` (+0x30).
    pub portrait: Portrait,
    /// #12 utf16: religion key (+0x60, a record pointer at run time).
    pub religion: String,
    /// #13 u32 (+0x64): the owning faction's id (every filled slot of a faction has the same).
    pub owner: u32,
    /// #14 u32 (+0x68): married, the owner's id; 0 = unmarried.
    pub married_to: u32,
    /// #15 `CAMPAIGN_LOCALISATION`, present when #14 is not 0 (+0x6C): a name kept for the spouse.
    pub spouse_names: Vec<String>,
    /// +0x18 (not a field of the record): the child who inherits. The family's #10 u8 saves its
    /// index (`0x0087D490` sets the flag of child #10 when below 4).
    pub heir: bool,
}

impl FamilyMember {
    /// A relative slot is filled when a name string is not empty (`0x004F3500` on both strings).
    pub fn named(&self) -> bool {
        self.names.iter().any(|n| !n.is_empty())
    }

    /// Cleared as `0x008B6860` clears a slot: names, flags (male stays set), children, ages,
    /// owner, marriage and religion; regnal number 1; portrait empty with index -1. The children's
    /// birth ages stay.
    fn clear(&mut self) {
        let keep = self.child_ages;
        *self = FamilyMember {
            names: vec![String::new(), String::new()],
            male: true,
            regnal: 1,
            portrait: Portrait { index: -1, ..Default::default() },
            ..Default::default()
        };
        self.child_ages = keep;
    }
}

/// `FACTION` `FAMILY` v2: ten members, the heir index, the ordinal pairs.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Family {
    /// The ten members (see the module docs for the slots).
    pub members: Vec<FamilyMember>,
    /// `ORDINAL_PAIR[]` {name, count}: how many rulers had the name (+0x534 list, `0x008C18B0`).
    pub ordinals: Vec<(String, i32)>,
    /// Family +0x544: daughters may inherit and new relatives may be women. Set to false by the
    /// loader (CONFIRMED); where it is set otherwise is UNKNOWN (PROVISIONAL: false).
    pub female_heirs: bool,
}

impl Family {
    /// The index saved as `FAMILY` #10: the first child flagged as heir, else 4.
    pub fn heir_index(&self) -> u8 {
        (0..4).find(|&k| self.members.get(FIRST_CHILD + k).is_some_and(|m| m.heir)).unwrap_or(4) as u8
    }

    fn child_count(&self) -> usize {
        usize::from(self.members[LEADER].children).min(4)
    }

    /// A PLACEHOLDER name for a new member: the original draws one (`0x008AA5E0`: the faction's
    /// royal name list, else its generic names), which is not modelled; the new member takes the
    /// name of the first filled member of the same sex (leader, spouse, relatives, children), else
    /// the leader's.
    fn placeholder_names(&self, male: bool) -> Vec<String> {
        let order = [LEADER, SPOUSE, 6, 7, 8, 9, 2, 3, 4, 5];
        order
            .iter()
            .filter_map(|&i| self.members.get(i))
            .find(|m| m.named() && m.male == male)
            .or_else(|| self.members.get(LEADER))
            .map(|m| m.names.clone())
            .unwrap_or_default()
    }

    /// Member `slot`'s portrait (`0x008EF630`, CONFIRMED): from the king / queen decks of his
    /// culture's set, by his age and sex ([`CulturePortraits::resolve_family_portrait`]). The exe
    /// takes the culture of the member's owner (+0x64), which is always the family's faction (every
    /// filled slot of a family has its id); `portraits` is that culture's set, `None` when the
    /// allocator has none (`0x009CB440` then changes nothing). The draws are on the deck's own LCG,
    /// never the campaign RNG.
    fn resolve_portrait(&mut self, slot: usize, portraits: Option<&mut CulturePortraits>) {
        if let Some(set) = portraits {
            let m = &mut self.members[slot];
            set.resolve_family_portrait(&mut m.portrait, m.age, m.male);
        }
    }

    /// A new adult member (`0x008B8860`): the slot is cleared (portrait number -1), then present,
    /// owned by `owner`, the faction's religion, `male`, a name unless `keep_name` (PLACEHOLDER),
    /// `age`, #4 drawn (`uniform_below(3)`), and last his portrait ([`Self::resolve_portrait`]).
    #[allow(clippy::too_many_arguments)]
    fn new_member(&mut self, rng: &mut CaRng, portraits: Option<&mut CulturePortraits>, slot: usize, owner: u32, male: bool, age: i32, keep_name: bool) {
        let names = if keep_name { self.members[slot].names.clone() } else { self.placeholder_names(male) };
        let religion = self.members[LEADER].religion.clone();
        let m = &mut self.members[slot];
        m.clear();
        m.exists = true;
        m.owner = owner;
        m.religion = religion;
        m.male = male;
        m.names = names;
        m.age = age;
        m.trait_4 = rng.uniform_below(3) as i32;
        self.resolve_portrait(slot, portraits);
    }

    /// A newborn (`0x008B8C80`): cleared, present, owned by `owner`, a boy when a 0..=100 draw is
    /// above 50, a name (PLACEHOLDER), age 0. No portrait yet (number -1): each caller resolves it
    /// once the child's age is set (the yearly pass at 0, the succession at his real age).
    fn birth(&mut self, rng: &mut CaRng, slot: usize, owner: u32) {
        let religion = self.members[LEADER].religion.clone();
        let male = rng.percent_0_100() > 50;
        let names = self.placeholder_names(male);
        let m = &mut self.members[slot];
        m.clear();
        m.exists = true;
        m.owner = owner;
        m.religion = religion;
        m.male = male;
        m.names = names;
    }
}

/// The chance of a child this year (`0x008B3AD0`): (135 − 3 × age) as a 16-bit value, divided by
/// the children so far + 1. Past 45 the 16-bit value wraps round to a large number, so older
/// members have a child every year until they have four (as the relatives in the saves do:
/// children at 46, 47, 48, 49).
fn fertility(m: &FamilyMember) -> u32 {
    u32::from((m.age.wrapping_mul(-3).wrapping_add(135)) as u16) / (u32::from(m.children) + 1)
}

/// A child's or relative's year (`0x008B45E0`): unmarried, he marries when 25 + a 0..=10 draw is
/// below his age (then #14 = the owner, #15 empty); married with fewer than four children, a
/// 0..=100 draw below [`fertility`] adds a child, remembering his age at the birth.
fn member_year(rng: &mut CaRng, m: &mut FamilyMember) {
    if m.married_to == 0 {
        let u = rng.int_range(0, 10);
        if u + 25 < m.age {
            m.married_to = m.owner;
            m.spouse_names = vec![String::new(), String::new()];
        }
    } else if m.children < 4 {
        let r = rng.percent_0_100();
        if r < fertility(m) {
            m.child_ages[usize::from(m.children)] = m.age;
            m.children += 1;
        }
    }
}

/// The yearly family pass (`0x008BC9C0`, run first in the year-end faction pass `0x008BC650` for a
/// faction that is not the rebels (`0x008CEEF0`), has +0x824 clear (`0x008BC6D0`) and whose family
/// +0x794 resolves). CONFIRMED order of the draws, all on the campaign RNG:
/// 1. the leader ages; each present child ages and has his year ([`member_year`]); a child turning
///    16 draws #4;
/// 2. an unmarried leader (or one whose spouse is gone) marries when 25 + a 0..=10 draw is below
///    his age: the spouse is the other sex, aged the leader's age + (queen) / − (king) a 0..=4
///    draw; a married leader's spouse ages, and with fewer than four children a 0..=100 draw below
///    [`fertility`] gives a child;
/// 3. each filled relative ages and has his year;
/// 4. death checks (the character death table, `0x009D5950`): the spouse, each present child, each
///    filled relative (the dead relative's slot is closed up);
/// 5. with fewer than four relatives, a 0..=100 draw at or below trunc(2^−n × 100) (n relatives)
///    adds one aged 16..=50 (a man unless daughters may inherit, then a 0..=100 draw above 50);
/// 6. in a republic the leader's own death check; if he dies, [`succession`].
pub fn family_year(model: &mut CampaignModel, faction: FactionId, hazards: &[f32; 101]) {
    let gov = model.world.factions.get(&faction).map(|f| f.government);
    let owner = faction.raw() as u32;
    let Some((fam, mut portraits, rng)) = model.family_parts(faction) else { return };
    if fam.members.len() < 10 {
        return;
    }
    fam.members[LEADER].age += 1;
    for k in 0..fam.child_count() {
        let c = &mut fam.members[FIRST_CHILD + k];
        if c.exists {
            c.age += 1;
            member_year(rng, c);
            if c.age == 16 {
                c.trait_4 = rng.uniform_below(3) as i32;
            }
        }
    }
    if fam.members[LEADER].married_to == 0 || !fam.members[SPOUSE].exists {
        let u = rng.int_range(0, 10);
        let leader = &fam.members[LEADER];
        if u + 25 < leader.age {
            let (lage, lmale) = (leader.age, leader.male);
            fam.members[LEADER].married_to = owner;
            let d = rng.uniform_below(5) as i32;
            let age = if lmale { lage - d } else { lage + d };
            fam.new_member(rng, portraits.as_deref_mut(), SPOUSE, owner, !lmale, age, false);
            fam.members[SPOUSE].married_to = owner;
            fam.members[SPOUSE].spouse_names = vec![String::new(), String::new()];
        }
    } else {
        fam.members[SPOUSE].age += 1;
        let n = fam.members[LEADER].children;
        if n < 4 {
            let fert = fertility(&fam.members[LEADER]);
            if rng.percent_0_100() < fert {
                fam.birth(rng, FIRST_CHILD + usize::from(n), owner);
                fam.members[LEADER].children += 1;
                fam.resolve_portrait(FIRST_CHILD + usize::from(n), portraits.as_deref_mut());
                fam.members[SPOUSE].children = fam.members[SPOUSE].children.saturating_add(1);
            }
        }
    }
    for j in 0..4 {
        let r = &mut fam.members[FIRST_RELATIVE + j];
        if r.named() {
            r.age += 1;
            member_year(rng, r);
        }
    }
    if fam.members[SPOUSE].exists && super::characters::dies_this_year(rng, hazards, fam.members[SPOUSE].age) {
        fam.members[SPOUSE].exists = false;
    }
    for k in 0..fam.child_count() {
        let c = &fam.members[FIRST_CHILD + k];
        if c.exists && super::characters::dies_this_year(rng, hazards, c.age) {
            fam.members[FIRST_CHILD + k].exists = false;
        }
    }
    let mut j = 0;
    while j < 4 {
        let r = &fam.members[FIRST_RELATIVE + j];
        if r.named() && super::characters::dies_this_year(rng, hazards, r.age) {
            close_up(fam, FIRST_RELATIVE + j);
            continue;
        }
        j += 1;
    }
    let n = (0..4).filter(|&j| fam.members[FIRST_RELATIVE + j].named()).count();
    if n < 4 {
        let r = rng.percent_0_100();
        let threshold = (2.0f32.powf(-(n as f32)) * 100.0) as u32;
        if r <= threshold {
            new_relative(fam, rng, portraits.as_deref_mut(), FIRST_RELATIVE + n, owner);
        }
    }
    let republic = !matches!(gov, Some(GovernmentType::AbsoluteMonarchy | GovernmentType::ConstitutionalMonarchy));
    if republic && super::characters::dies_this_year(rng, hazards, fam.members[LEADER].age) {
        succession(fam, rng, portraits, owner);
    }
}

/// A new relative aged 16..=50 (a man unless daughters may inherit, then a 0..=100 draw above 50).
fn new_relative(fam: &mut Family, rng: &mut CaRng, portraits: Option<&mut CulturePortraits>, slot: usize, owner: u32) {
    let age = rng.int_range(0, 34) + 16;
    let male = if fam.female_heirs { rng.percent_0_100() > 50 } else { true };
    fam.new_member(rng, portraits, slot, owner, male, age, false);
}

/// The regnal numeral shown after a monarch's name (`0x008DCF50`, CONFIRMED): "X" repeated
/// `n / 10` times, then the units of `n % 10` as `I` .. `IX` (string table `0x01357040`), so 3 →
/// "III", 14 → "XIV", 40 → "XXXX" (no L / C). The original counts the tens in a byte, so the tens
/// wrap at 256 (kept); a number below 1 gives "" (UNKNOWN in the original, whose table index would
/// be out of range; no shipped data reaches it).
pub fn regnal_numeral(n: i32) -> String {
    const UNITS: [&str; 10] = ["", "I", "II", "III", "IV", "V", "VI", "VII", "VIII", "IX"];
    if n < 1 {
        return String::new();
    }
    let mut s = "X".repeat(usize::from((n / 10) as u8));
    s.push_str(UNITS[(n % 10) as usize]);
    s
}

/// The succession (`0x008B9500`), when the leader's post falls vacant (or a republic's family
/// head dies). CONFIRMED rules:
/// - the heir is the first child (of the leader's `children`) flagged as heir; a daughter when
///   daughters may not inherit hands it to the first son of the children; with no such child the
///   first relative inherits (one aged 16..=50 is made first when there is none) and the other
///   relatives move up;
/// - his regnal number comes from the ordinal list: one more than the last ruler of the name, or
///   the name is added with his current number;
/// - a married new ruler gets a spouse slot (a king a wife aged his age − a 0..=4 draw, at least
///   16; a queen a husband aged her age + the draw), keeping the #15 name when there is one, with
///   as many children as the ruler; an unmarried one an empty spouse slot;
/// - his children are made anew from his count (a newborn each, aged his age − his age at their
///   birth), the other child slots cleared, and the first child becomes the heir.
///
/// Returns whether the heir came from outside the children (the original then may name another
/// faction as a claimant: UNKNOWN effect, not modelled).
pub fn succession(fam: &mut Family, rng: &mut CaRng, mut portraits: Option<&mut CulturePortraits>, owner: u32) -> bool {
    let n = fam.child_count();
    let mut pick = None;
    for i in 0..n {
        let c = &fam.members[FIRST_CHILD + i];
        if c.heir {
            pick = if !c.male && !fam.female_heirs { (0..n).find(|&j| fam.members[FIRST_CHILD + j].male) } else { Some(i) };
            break;
        }
    }
    let outside = pick.is_none();
    match pick {
        Some(k) => fam.members[LEADER] = fam.members[FIRST_CHILD + k].clone(),
        None => {
            if !fam.members[FIRST_RELATIVE].named() {
                new_relative(fam, rng, portraits.as_deref_mut(), FIRST_RELATIVE, owner);
            }
            fam.members[LEADER] = fam.members[FIRST_RELATIVE].clone();
            close_up(fam, FIRST_RELATIVE);
        }
    }
    // Regnal number.
    let name = fam.members[LEADER].names.first().cloned().unwrap_or_default();
    if let Some(o) = fam.ordinals.iter_mut().find(|o| o.0 == name) {
        o.1 += 1;
        fam.members[LEADER].regnal = o.1;
    } else {
        fam.ordinals.push((name, fam.members[LEADER].regnal));
    }
    let leader = fam.members[LEADER].clone();
    if leader.married_to == 0 {
        fam.members[SPOUSE].clear();
    } else {
        let keep = leader.spouse_names.iter().any(|s| !s.is_empty());
        let d = rng.uniform_below(5) as i32;
        let (male, age) = if leader.male { (false, (leader.age - d).max(16)) } else { (true, leader.age + d) };
        if keep {
            fam.members[SPOUSE].names = leader.spouse_names.clone();
        }
        fam.new_member(rng, portraits.as_deref_mut(), SPOUSE, leader.married_to, male, age, keep);
        fam.members[SPOUSE].children = leader.children;
    }
    let count = usize::from(leader.children).min(4);
    for k in 0..4 {
        if k < count {
            fam.birth(rng, FIRST_CHILD + k, owner);
            fam.members[FIRST_CHILD + k].age = leader.age - leader.child_ages[k];
            fam.resolve_portrait(FIRST_CHILD + k, portraits.as_deref_mut());
        } else {
            fam.members[FIRST_CHILD + k].clear();
        }
    }
    fam.members[FIRST_CHILD].heir = true;
    outside
}

impl CampaignModel {
    /// What the family pass changes, borrowed apart: faction `faction`'s family, the portrait set
    /// of its culture (`0x008EF630`: the owner faction's subculture record → its culture; `None`
    /// when the allocator has no set for it) and the campaign RNG. `None` without a family.
    fn family_parts(&mut self, faction: FactionId) -> Option<(&mut Family, Option<&mut CulturePortraits>, &mut CaRng)> {
        let culture = self.rules.characters.culture(&self.world.factions.get(&faction)?.key);
        let fam = self.world.faction_details.get_mut(&faction)?.family.as_mut()?;
        let set = self.world.portraits.iter_mut().find(|s| s.culture == culture);
        Some((fam, set, &mut self.rng))
    }

    /// A post whose holder died gets a new holder at once (`0x008E05E0` → `0x008E4610` → the
    /// government class's slot +0x10: `0x008B7070` absolute monarchy, `0x008B7340` constitutional
    /// monarchy, `0x008B75C0` republic; CONFIRMED code; that the death of the holder is what calls
    /// it is INFERRED, the handler is reached through a table only). A new `minister`:
    /// - the leader's post of a monarchy: the family's successor ([`succession`]), named after him
    ///   and his age;
    /// - any other post, and a republic's leader: a new minister aged 21 + a 0..=30 draw (absolute
    ///   monarchy) or 25 + a 0..=30 draw (the other two).
    ///
    /// His portrait is a minister's, drawn at his age ([`Self::assign_portrait`]); a monarch then takes
    /// the family leader's (CONFIRMED).
    ///
    /// He stands off the map at (0, 0) as the original's ministers do. PROVISIONAL: his name for an
    /// ordinary post is drawn by the save writer (`ntw_campaign::charnames`), the original's name
    /// draws after the age draw are not on the model's RNG; the messages are not modelled.
    pub fn refill_post(&mut self, faction: FactionId, post: usize) {
        use super::details::CharacterDetails;
        use super::world::{Character, CharacterKind};
        let Some(gov) = self.world.factions.get(&faction).map(|f| f.government) else { return };
        let Some(details) = self.world.faction_details.get(&faction) else { return };
        // A faction out of the game gets no new holder: its destruction kills its characters until
        // its list is empty (`0x008AB6C0`, the loop on `0x008BC550`), which a new minister joining
        // it at each death would never end (INFERRED; where the exe stops the refill is not traced).
        if details.out_of_game == Some(true) {
            return;
        }
        let Some(p) = details.posts.get(post) else { return };
        if p.holder.is_some() {
            return;
        }
        let (post_id, leader_post) = (p.id, p.key == "faction_leader");
        let monarchy = matches!(gov, GovernmentType::AbsoluteMonarchy | GovernmentType::ConstitutionalMonarchy);
        let year = self.calendar.date.year as i32;
        let mut names = (String::new(), String::new());
        let mut numeral = String::new();
        let mut monarch_portrait = None;
        let age = if leader_post && monarchy && details.family.as_ref().is_some_and(|f| f.members.len() == 10) {
            let (fam, portraits, rng) = self.family_parts(faction).expect("checked");
            succession(fam, rng, portraits, faction.raw() as u32);
            let l = &fam.members[LEADER];
            monarch_portrait = Some(l.portrait.clone());
            names.0 = l.names.first().cloned().unwrap_or_default();
            // The new monarch-minister's name carries the successor's numeral (`0x008B7070` /
            // `0x008B7340` call `0x008DCF50` into character +0x33C, CONFIRMED).
            numeral = regnal_numeral(l.regnal);
            l.age
        } else {
            let base = if gov == GovernmentType::AbsoluteMonarchy && !leader_post { 21 } else { 25 };
            base + self.rng.int_range(0, 30)
        };
        let id = CharacterId(self.world.alloc_id() as i32);
        self.world.characters.insert(
            id,
            Character {
                id,
                faction,
                kind: CharacterKind::Minister,
                position: Default::default(),
                movement_points: 0,
                max_movement_points: 0,
                base_movement_points: 0,
                garrisoned_in: None,
            },
        );
        let birth = crate::calendar::Date { year: (year - age).max(0) as u32, ..self.calendar.date };
        self.world.character_details.insert(
            id,
            CharacterDetails {
                forename: names.0,
                surname: names.1,
                regnal_numeral: numeral,
                birth: Some(birth),
                post: post_id as u32,
                ..self.rules.characters.new_character_details(CharacterKind::Minister.esf_name())
            },
        );
        // The minister constructor `0x00990900` resolves a minister portrait at his age (an empty
        // spec to `0x00992E60`), so the minister deck advances for a monarch too; then `0x008B7070` /
        // `0x008B7340` copy the family leader's whole portrait block over it (`0x009D21B0` from
        // family member +0x30, CONFIRMED).
        self.assign_portrait(id, CharacterKind::Minister.esf_name());
        if let (Some(portrait), Some(d)) = (monarch_portrait, self.world.character_details.get_mut(&id)) {
            d.portrait = portrait;
        }
        if let Some(p) = self.world.faction_details.get_mut(&faction).and_then(|d| d.posts.get_mut(post)) {
            p.holder = Some(id);
        }
    }
}

/// Removes relative `slot`: the relatives after it move up one and the last slot is cleared (it
/// keeps its children's birth ages, as `0x008B6860` leaves them).
fn close_up(fam: &mut Family, slot: usize) {
    fam.members.remove(slot);
    let mut last = fam.members[fam.members.len() - 1].clone();
    last.clear();
    fam.members.push(last);
}

impl CampaignModel {
    /// The spare ministers of a faction: its `minister` characters without a post, in id order.
    /// The government keeps five (`0x008BF280` tops the list up to 5 with new ministers); every
    /// faction of the vanilla saves has exactly 5 (CONFIRMED, 23 + 4 factions).
    pub fn spare_ministers(&self, faction: FactionId) -> Vec<CharacterId> {
        self.world
            .characters
            .values()
            .filter(|c| c.faction == faction && c.kind == super::world::CharacterKind::Minister)
            .filter(|c| self.world.character_details.get(&c.id).is_none_or(|d| d.post == 0))
            .map(|c| c.id)
            .collect()
    }

    /// The post a character holds: (faction, post index).
    fn post_of(&self, c: CharacterId) -> Option<(FactionId, usize)> {
        self.world.faction_details.iter().find_map(|(f, d)| d.posts.iter().position(|p| p.holder == Some(c)).map(|i| (*f, i)))
    }

    /// Puts `c` into post `post` of `faction` (`0x009CB1E0`).
    fn seat(&mut self, faction: FactionId, post: usize, c: CharacterId) {
        let Some(p) = self.world.faction_details.get_mut(&faction).and_then(|d| d.posts.get_mut(post)) else { return };
        p.holder = Some(c);
        let id = p.id as u32;
        self.world.character_details.entry(c).or_default().post = id;
    }

    /// Dismissing a minister (`0x008EB9D0`, CONFIRMED): he leaves the game; the government type
    /// then fills the post: an absolute monarchy (its slot +4 is false) seats a spare minister
    /// picked by an int_range(0, spares − 1) draw (`0x008E44F0`, the list not topped up), the other
    /// two make a new minister ([`Self::refill_post`]). A constitutional monarchy also counts the
    /// dismissal (its slot +0x20 adds 1 to a counter, +0x14; what reads it is UNKNOWN, not modelled).
    /// PROVISIONAL: the faction leader cannot be dismissed (the interface's rule is not traced), and
    /// with no spare the post gets a new minister (the original would read an empty list).
    pub(crate) fn dismiss_minister(&mut self, minister: CharacterId) -> Result<Vec<super::CampaignEvent>, super::CommandError> {
        use super::CommandError as E;
        let (faction, post) = self.post_of(minister).ok_or(E::Unsupported("that character holds no post"))?;
        if !self.may_act(faction) {
            return Err(E::NotYourTurn(faction));
        }
        if self.world.faction_details[&faction].posts[post].key == "faction_leader" {
            return Err(E::Unsupported("the faction leader cannot be dismissed"));
        }
        let gov = self.world.factions.get(&faction).map(|f| f.government);
        self.world.remove_character(minister);
        self.world.character_details.remove(&minister);
        if let Some(p) = self.world.faction_details.get_mut(&faction).and_then(|d| d.posts.get_mut(post)) {
            p.holder = None;
        }
        let spares = self.spare_ministers(faction);
        if gov == Some(GovernmentType::AbsoluteMonarchy) && !spares.is_empty() {
            let i = self.rng.int_range(0, spares.len() as i32 - 1) as usize;
            self.seat(faction, post, spares[i]);
        } else {
            self.refill_post(faction, post);
        }
        Ok(Vec::new())
    }

    /// Appointing (`0x008F3760`, CONFIRMED): two post holders swap posts; in an absolute monarchy
    /// a spare minister may also replace a post holder, who then leaves the game (`0x008E44F0`, the
    /// list not topped up). The other governments refuse a spare (`0x008B3500`).
    /// PROVISIONAL: the faction leader's post is not part of it (the interface's rule is not traced).
    pub(crate) fn appoint_minister(&mut self, a: CharacterId, b: CharacterId) -> Result<Vec<super::CampaignEvent>, super::CommandError> {
        use super::CommandError as E;
        let fa = self.world.characters.get(&a).ok_or(E::UnknownCharacter(a))?.faction;
        let fb = self.world.characters.get(&b).ok_or(E::UnknownCharacter(b))?.faction;
        if fa != fb || a == b {
            return Err(E::WrongFaction);
        }
        if !self.may_act(fa) {
            return Err(E::NotYourTurn(fa));
        }
        let (pa, pb) = (self.post_of(a), self.post_of(b));
        let leader = |p: Option<(FactionId, usize)>| p.is_some_and(|(f, i)| self.world.faction_details[&f].posts[i].key == "faction_leader");
        if leader(pa) || leader(pb) {
            return Err(E::Unsupported("the faction leader's post cannot be changed"));
        }
        match (pa, pb) {
            (Some((_, i)), Some((_, j))) => {
                self.seat(fa, i, b);
                self.seat(fa, j, a);
            }
            (Some((_, i)), None) | (None, Some((_, i))) => {
                if self.world.factions.get(&fa).map(|f| f.government) != Some(GovernmentType::AbsoluteMonarchy) {
                    return Err(E::Unsupported("only an absolute monarchy appoints spare ministers"));
                }
                let (holder, spare) = if pa.is_some() { (a, b) } else { (b, a) };
                if !self.spare_ministers(fa).contains(&spare) {
                    return Err(E::Unsupported("not a spare minister"));
                }
                self.world.remove_character(holder);
                self.world.character_details.remove(&holder);
                self.seat(fa, i, spare);
            }
            (None, None) => return Err(E::Unsupported("neither holds a post")),
        }
        Ok(Vec::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn member(name: &str, male: bool, age: i32) -> FamilyMember {
        FamilyMember { names: vec![name.into()], male, exists: true, age, regnal: 1, owner: 7, ..Default::default() }
    }

    fn family() -> Family {
        let mut f = Family { members: Vec::new(), ordinals: vec![("Karl".into(), 2)], female_heirs: false };
        for _ in 0..10 {
            let mut m = FamilyMember::default();
            m.clear();
            f.members.push(m);
        }
        f.members[LEADER] = member("Karl", true, 60);
        f.members[LEADER].children = 2;
        f.members[LEADER].child_ages = [30, 32, 0, 0];
        f.members[FIRST_CHILD] = member("Maria", false, 30);
        f.members[FIRST_CHILD].heir = true;
        f.members[FIRST_CHILD + 1] = member("Karl", true, 28);
        f.members[FIRST_RELATIVE] = member("Otto", true, 40);
        f.members[FIRST_RELATIVE + 1] = member("Hans", true, 35);
        f
    }

    /// `0x008DCF50`: tens as repeated "X", units from the I..IX table (bug 2026-10-08: the
    /// negotiation screen read "George" for the original's "George III").
    #[test]
    fn regnal_numerals_follow_the_exe_table() {
        for (n, s) in [(0, ""), (1, "I"), (3, "III"), (4, "IV"), (9, "IX"), (10, "X"), (14, "XIV"), (19, "XIX"), (40, "XXXX")] {
            assert_eq!(regnal_numeral(n), s, "{n}");
        }
    }

    #[test]
    fn fertility_wraps_past_45() {
        let mut m = member("A", true, 30);
        assert_eq!(fertility(&m), 45);
        m.children = 2;
        assert_eq!(fertility(&m), 15);
        m.age = 50;
        // (135 - 150) as u16 = 65521, / 3.
        assert_eq!(fertility(&m), 65521 / 3);
    }

    #[test]
    fn a_daughter_heir_hands_on_to_the_first_son() {
        let mut f = family();
        let mut rng = CaRng::new(5);
        assert!(!succession(&mut f, &mut rng, None, 7));
        let l = &f.members[LEADER];
        assert_eq!((l.names[0].as_str(), l.age), ("Karl", 28));
        // Karl again: the third of the name.
        assert_eq!((l.regnal, f.ordinals[0].1), (3, 3));
        // His children are made from his own count (none), the first child slot is the heir.
        assert!(f.members[FIRST_CHILD..FIRST_CHILD + 4].iter().all(|c| !c.exists));
        assert!(f.members[FIRST_CHILD].heir);
        // Daughters may inherit: Maria.
        let mut f = family();
        f.female_heirs = true;
        succession(&mut f, &mut rng, None, 7);
        assert_eq!(f.members[LEADER].names[0], "Maria");
    }

    #[test]
    fn without_a_flagged_child_the_first_relative_inherits() {
        let mut f = family();
        f.members[FIRST_CHILD].heir = false;
        let mut rng = CaRng::new(5);
        assert!(succession(&mut f, &mut rng, None, 7));
        assert_eq!(f.members[LEADER].names[0], "Otto");
        // Hans moves up, the last slot is empty; Otto is a new name in the ordinal list.
        assert_eq!(f.members[FIRST_RELATIVE].names[0], "Hans");
        assert!(!f.members[9].named());
        assert!(f.ordinals.iter().any(|o| o.0 == "Otto" && o.1 == 1));
        // No relative left at all: one aged 16..=50 is made first.
        let mut f = family();
        f.members[FIRST_CHILD].heir = false;
        f.members[FIRST_RELATIVE].clear();
        f.members[FIRST_RELATIVE + 1].clear();
        succession(&mut f, &mut rng, None, 7);
        let l = &f.members[LEADER];
        assert!(l.named() && l.male && (16..=50).contains(&l.age));
    }

    #[test]
    fn a_married_successor_gets_a_spouse_and_his_children() {
        let mut f = family();
        f.members[FIRST_CHILD].heir = false;
        let otto = &mut f.members[FIRST_RELATIVE];
        otto.married_to = 7;
        otto.children = 2;
        otto.child_ages = [25, 30, 0, 0];
        let mut rng = CaRng::new(9);
        succession(&mut f, &mut rng, None, 7);
        let s = &f.members[SPOUSE];
        assert!(s.exists && !s.male && (36..=40).contains(&s.age) && s.children == 2);
        assert_eq!((f.members[FIRST_CHILD].age, f.members[FIRST_CHILD + 1].age), (15, 10));
        assert!(f.members[FIRST_CHILD].exists && f.members[FIRST_CHILD + 1].exists && !f.members[FIRST_CHILD + 2].exists);
    }

    #[test]
    fn a_relative_marries_and_has_children() {
        let mut rng = CaRng::new(3);
        let mut m = member("Otto", true, 40);
        for _ in 0..20 {
            member_year(&mut rng, &mut m);
            m.age += 1;
        }
        assert_eq!(m.married_to, 7);
        assert!(m.children > 0 && m.child_ages[0] >= 40);
    }

    /// A european set with king / queen decks: Info young, Info old (the Cards decks are never drawn).
    fn royal_set() -> CulturePortraits {
        use super::super::portraits::{PortraitCategory, PortraitDeck};
        let deck = |order: Vec<u32>| PortraitDeck { count: order.len() as u32, cursor: 0, order, seed: 3 };
        let decks = |young: Vec<u32>, old: Vec<u32>| vec![deck(young), deck(old), deck(Vec::new()), deck(Vec::new())];
        CulturePortraits {
            culture: "european".into(),
            paths: vec![("king".into(), "european".into()), ("queen".into(), "european".into()), ("minister".into(), "european".into())],
            categories: vec![
                PortraitCategory { key: "minister".into(), decks: decks(vec![8, 9], vec![10, 11]) },
                PortraitCategory { key: "king".into(), decks: decks(vec![4, 1, 6, 2], vec![7, 2]) },
                PortraitCategory { key: "queen".into(), decks: decks(vec![3, 0, 5, 1], vec![5, 6]) },
            ],
        }
    }

    fn cursor(set: &CulturePortraits, key: &str, deck: usize) -> u32 {
        set.categories.iter().find(|c| c.key == key).unwrap().decks[deck].cursor
    }

    fn index_and_card(p: &Portrait) -> (i32, String) {
        (p.index, p.card.clone())
    }

    /// `0x00A055E0`: a man from the king decks, a woman from the queen decks, the old deck at 45 or
    /// older; a number already held is kept; an empty deck leaves -1 and no pictures.
    #[test]
    fn a_family_portrait_comes_from_the_king_or_queen_decks_by_age() {
        let mut set = royal_set();
        let mut p = Portrait::default();
        assert!(set.resolve_family_portrait(&mut p, 50, true));
        assert_eq!((p.index, p.card.as_str()), (7, "ui/portraits/european/Cards/king/old/007.tga"));
        assert_eq!(p.info, "ui/portraits/european/Info/king/old/007.jpg");
        let mut q = Portrait::default();
        assert!(set.resolve_family_portrait(&mut q, 44, false));
        assert_eq!((q.index, q.card.as_str()), (3, "ui/portraits/european/Cards/queen/young/003.tga"));
        // A kept number at a new age: only the pictures change.
        assert!(set.resolve_family_portrait(&mut q, 45, false));
        assert_eq!((q.index, q.card.as_str()), (3, "ui/portraits/european/Cards/queen/old/003.tga"));
        assert_eq!((cursor(&set, "queen", 0), cursor(&set, "queen", 1)), (1, 0));
        // A custom picture name does not stop the resolve (no check in `0x00A055E0`).
        let mut r = Portrait { alternative: "custom".into(), ..Default::default() };
        assert!(set.resolve_family_portrait(&mut r, 20, true));
        assert_eq!(r.index, 4);
        // An empty deck.
        set.categories[1].decks[0] = Default::default();
        let mut e = Portrait::default();
        assert!(!set.resolve_family_portrait(&mut e, 20, true));
        assert_eq!(e, Portrait::default());
    }

    /// The succession's new spouse (`0x008B8860`) and children (`0x008B9500`: resolved after the
    /// child's age is set) get their portraits; the campaign RNG stream is the same with or
    /// without portraits (the decks have their own LCG).
    #[test]
    fn a_successors_new_spouse_and_children_get_family_portraits() {
        let mut f = family();
        f.members[FIRST_CHILD].heir = false;
        let otto = &mut f.members[FIRST_RELATIVE];
        otto.married_to = 7;
        otto.children = 2;
        otto.child_ages = [25, 30, 0, 0];
        let mut set = royal_set();
        let (mut with, mut without) = (f.clone(), f);
        let (mut rng_a, mut rng_b) = (CaRng::new(9), CaRng::new(9));
        succession(&mut with, &mut rng_a, Some(&mut set), 7);
        succession(&mut without, &mut rng_b, None, 7);
        assert_eq!(rng_a, rng_b, "no campaign-RNG draw");
        assert_eq!(index_and_card(&with.members[SPOUSE].portrait), (3, "ui/portraits/european/Cards/queen/young/003.tga".to_string()));
        // Children aged 15 and 10: each from his sex's young deck, in slot order.
        let (mut kings, mut queens) = ([4, 1, 6, 2].into_iter(), [0, 5, 1].into_iter());
        for k in 0..2 {
            let c = &with.members[FIRST_CHILD + k];
            let (folder, n) = if c.male { ("king", kings.next().unwrap()) } else { ("queen", queens.next().unwrap()) };
            assert_eq!(index_and_card(&c.portrait), (n, format!("ui/portraits/european/Cards/{folder}/young/{n:03}.tga")));
        }
        // Without a set nothing is drawn: number -1, no pictures, everything else the same.
        assert!(without.members[SPOUSE..FIRST_CHILD + 2].iter().all(|m| m.portrait == Portrait::default()));
        for m in with.members.iter_mut().chain(without.members.iter_mut()) {
            m.portrait = Portrait::default();
        }
        assert_eq!(with, without);
    }

    /// A model with France (european, absolute monarchy, id 7), its family and the royal set.
    fn royal_model() -> CampaignModel {
        use super::super::details::{FactionDetails, GovernmentPost};
        use super::super::world::{Faction, World};
        use crate::calendar::{Calendar, Date};
        let mut w = World::default();
        let faction = Faction {
            id: FactionId(7),
            key: "france".into(),
            treasury: 0,
            government: GovernmentType::AbsoluteMonarchy,
            government_key: String::new(),
            tax_lower: String::new(),
            tax_upper: String::new(),
            diplomacy: Default::default(),
        };
        w.factions.insert(FactionId(7), faction);
        let posts = vec![
            GovernmentPost { id: 1, key: "faction_leader".into(), holder: None, governorship: None },
            GovernmentPost { id: 2, key: "finance".into(), holder: None, governorship: None },
        ];
        w.faction_details.insert(FactionId(7), FactionDetails { family: Some(family()), posts, ..Default::default() });
        w.portraits = vec![royal_set()];
        let date = Date { year: 1805, season: 0, month: 4, half: 0 };
        let mut m = CampaignModel::new(Calendar::new(date, 2), CaRng::new(1), w);
        let rules = std::sync::Arc::make_mut(&mut m.rules);
        rules.characters.faction_subculture.insert("france".into(), "sc_european".into());
        rules.characters.subculture_culture.insert("sc_european".into(), "european".into());
        m
    }

    /// The yearly pass's newborn is resolved at age 0 (`0x008BC9C0` at `0x008BCB31`).
    #[test]
    fn a_newborn_gets_a_young_family_portrait() {
        let mut m = royal_model();
        let fam = m.world.faction_details.get_mut(&FactionId(7)).unwrap().family.as_mut().unwrap();
        fam.members[LEADER].age = 30;
        fam.members[LEADER].children = 0;
        fam.members[LEADER].married_to = 7;
        fam.members[SPOUSE] = member("Louise", false, 28);
        for _ in 0..40 {
            family_year(&mut m, FactionId(7), &[0.0; 101]);
            if m.world.faction_details[&FactionId(7)].family.as_ref().unwrap().members[LEADER].children > 0 {
                break;
            }
        }
        let fam = m.world.faction_details[&FactionId(7)].family.as_ref().unwrap();
        let c = &fam.members[FIRST_CHILD];
        assert!(c.exists && c.age == 0, "a child this year");
        let (folder, n) = if c.male { ("king", 4) } else { ("queen", 3) };
        assert_eq!(index_and_card(&c.portrait), (n, format!("ui/portraits/european/Cards/{folder}/young/{n:03}.tga")));
    }

    /// A new minister draws a minister portrait at his age (`0x00990900` → `0x00992E60`); a
    /// monarch-minister draws one too (the minister deck advances), then takes the family
    /// leader's whole portrait (`0x009D21B0`).
    #[test]
    fn new_ministers_get_minister_portraits_and_the_monarch_the_familys() {
        let mut m = royal_model();
        m.refill_post(FactionId(7), 1);
        let id = m.world.faction_details[&FactionId(7)].posts[1].holder.expect("a finance minister");
        let d = &m.world.character_details[&id];
        let age = 1805 - d.birth.unwrap().year as i32;
        assert!((21..=51).contains(&age));
        let (deck, folder, n) = if age >= 45 { (1, "old", 10) } else { (0, "young", 8) };
        assert_eq!(index_and_card(&d.portrait), (n, format!("ui/portraits/european/Cards/minister/{folder}/{n:03}.tga")));
        assert_eq!(cursor(&m.world.portraits[0], "minister", deck), 1);
        // The heir (Karl, the second child, as Maria may not inherit) has a portrait of his own.
        let held = Portrait { card: "c".into(), alternative: "a".into(), info: "i".into(), index: 12 };
        m.world.faction_details.get_mut(&FactionId(7)).unwrap().family.as_mut().unwrap().members[FIRST_CHILD + 1].portrait = held.clone();
        let before = m.world.portraits[0].categories[0].decks.iter().map(|d| d.cursor).sum::<u32>();
        m.refill_post(FactionId(7), 0);
        let fam = m.world.faction_details[&FactionId(7)].family.as_ref().unwrap();
        let id = m.world.faction_details[&FactionId(7)].posts[0].holder.expect("a monarch");
        assert_eq!(fam.members[LEADER].names[0], "Karl");
        assert_eq!(m.world.character_details[&id].portrait, held);
        let after = m.world.portraits[0].categories[0].decks.iter().map(|d| d.cursor).sum::<u32>();
        assert_eq!(after, before + 1, "the monarch drew from the minister deck too");
    }
}
