//! Character details tables and their attributes (character cards, the Lists rows, the government's ministers).

use super::*;

/// A character's details table as the exe builds it for character cards and panels
/// (0x009AD250, CONFIRMED key names; used by FactionDetails' Leader, the government ministers,
/// ...): Name, Age, AgentType, Title, Address, Flag ("<flag>/portrait_flags.tga"), SmallFlag
/// ("<flag>/small.tga"), ActionPoints, ActionPointsPerTurn, InfoImage and CardImage ("data/" +
/// the portrait pictures), IsGuerilla, IsNaval, CommanderType, ShowAsCharacter, Playable,
/// Location, FactionColour / UniformColour {r, g, b}, Attributes {PrimaryLevel,
/// PrimaryAttributePath, PrimaryAttributeName, [i] = {Value, PipPath, AttributeName}}, Traits,
/// Ancillaries, plus spying_data_level (read by Utilities.CreateCharacterCard).
/// PROVISIONAL: Title empty, Traits / Ancillaries / PostEffects as key lists only, spying_data_level
/// 3 (everything known). The attribute pictures are `agent_attributes`' ([`attribute_icon`]). The card
/// template also reads ShowAttributes, ShowFlag and TechImage (CONFIRMED reads); they are not given
/// (UNKNOWN values).
pub(super) fn character_details(lua: &Lua, inner: &Inner, ui: &CampaignUi, c: CharacterId) -> mlua::Result<Value> {
    let m = ui.model();
    let Some(ch) = m.world.characters.get(&c) else { return Ok(Value::Nil) };
    let d = m.world.character_details.get(&c);
    let faction_key = m.world.factions.get(&ch.faction).map(|f| f.key.clone()).unwrap_or_default();
    let t = lua.create_table()?;
    t.set("Address", character_value(ui, c))?;
    t.set("AgentType", ch.kind.esf_name())?;
    t.set("Title", "")?;
    if let Some(b) = d.and_then(|d| d.birth) {
        t.set("Age", (m.calendar.date.year as i32 - b.year as i32).max(0))?;
    }
    let rec = ui.link.db.faction(&faction_key);
    let flag = rec.map(|r| r.flag_path.clone()).unwrap_or_default();
    t.set("Flag", format!("{flag}/portrait_flags.tga"))?;
    t.set("SmallFlag", format!("{flag}/small.tga"))?;
    t.set("ActionPoints", ch.movement_points)?;
    t.set("ActionPointsPerTurn", ch.max_movement_points)?;
    if let Some(p) = d.map(|d| &d.portrait) {
        if !p.card.is_empty() {
            t.set("CardImage", format!("data/{}", p.card))?;
        }
        if !p.info.is_empty() {
            t.set("InfoImage", format!("data/{}", p.info))?;
        }
    }
    // IsGuerilla (CONFIRMED name; Agents.lua's buttons read it): INFERRED from the character type
    // `guerilla` (the spa start's guerillas).
    t.set("IsGuerilla", ch.kind == CharacterKind::Guerilla)?;
    let naval = matches!(ch.kind, CharacterKind::Admiral | CharacterKind::Captain);
    t.set("IsNaval", naval)?;
    t.set("CommanderType", if naval { 2 } else { 0 })?;
    // ShowAsCharacter / CommandedUnit / Soldiers (own Ghidra copy, 2026-10-07; CONFIRMED in
    // `0x009AD250`): the character is shown as himself unless he commands a unit and his agent
    // type is neither 0 (General) nor 1 (admiral) (`0x00F9C6C0` / `0x00F9C690` on the agent record
    // `+0x2C`); otherwise CommandedUnit is his unit's card with DisplayAsUnit true. Soldiers is his
    // force's soldier count (force vfunc `+0x74`, ours the units' men) whenever he has a force. So
    // the Lists rows (`template.row_template_army.lua:65-71`) give generals their portrait card and
    // colonels their unit's picture. PLACEHOLDER: a general or admiral with no portrait in the
    // model (pool hires, promoted generals; BACKLOG §0 "portraits of generated characters") keeps his unit card.
    let force = m.force_of(c).and_then(|f| m.world.forces.get(&f));
    let commanded = force.filter(|f| f.commander == Some(c) && !f.units.is_empty());
    let show_as_character = commanded.is_none()
        || (matches!(ch.kind, CharacterKind::General | CharacterKind::Admiral) && portrait_card(&m, c).is_some());
    t.set("ShowAsCharacter", show_as_character)?;
    if let Some(f) = &force {
        t.set("Soldiers", f.units.iter().map(|u| u.men).sum::<u32>())?;
    }
    t.set("Playable", rec.is_some_and(|r| r.category == "playable"))?;
    t.set("Location", ui.location_name(inner, (ch.position.0.to_f32(), ch.position.1.to_f32())))?;
    let colour = |c: [u8; 3]| -> mlua::Result<Table> {
        let ct = lua.create_table()?;
        ct.set("r", c[0])?;
        ct.set("g", c[1])?;
        ct.set("b", c[2])?;
        Ok(ct)
    };
    if let Some(r) = rec {
        t.set("FactionColour", colour(r.primary_colour())?)?;
        t.set("UniformColour", colour(r.secondary_colour())?)?;
    }
    t.set("Attributes", attributes_table(lua, inner, ui, Some(c), Pips::With)?)?;
    let traits = lua.create_table()?;
    for (i, tr) in d.map(|d| d.traits.as_slice()).unwrap_or_default().iter().enumerate() {
        traits.set(i + 1, tr.key.as_str())?;
    }
    t.set("Traits", traits)?;
    let anc = lua.create_table()?;
    for (i, a) in d.map(|d| d.ancillaries.as_slice()).unwrap_or_default().iter().enumerate() {
        anc.set(i + 1, a.as_str())?;
    }
    t.set("Ancillaries", anc)?;
    // What the player may know about this character (`utilities.lua`'s `spying_data_level`,
    // CONFIRMED values; see `spying_level_character` for the INFERRED mapping onto the model).
    // A foreign character under the shroud shows only what the player knows of him.
    t.set("spying_data_level", match m.faction_by_key(&ui.link.human).map(|h| h.id) {
        Some(human) => spying_level_character(&m, human, c),
        None => LEVEL_INVALID,
    })?;
    // knowledge_mask: the same answer as the unit rows carry (bit 1 the icon, 15 one's own).
    t.set("knowledge_mask", knowledge_mask(match m.faction_by_key(&ui.link.human).map(|h| h.id) {
        Some(human) => spying_level_character(&m, human, c),
        None => LEVEL_INVALID,
    }))?;
    // Post / PostName / PostEffects (CONFIRMED names): the government post the character holds,
    // its name for the faction's government (`ministerial_positions_by_gov_types`: faction, post,
    // government → string key, loc `ministerial_positions_strings_on_screen_<key>`; column use
    // INFERRED from the rows). PROVISIONAL: PostEffects empty.
    let post = m.world.faction_details.get(&ch.faction).and_then(|d| d.posts.iter().find(|p| p.holder == Some(c)).map(|p| p.key.clone()));
    if let Some(post) = post {
        let gov = m.world.factions.get(&ch.faction).map(|f| f.government_key.clone()).unwrap_or_default();
        let rows = ui.post_names.get_or_init(|| {
            small_table(inner, &tables::MINISTERIAL_POSITIONS_BY_GOV_TYPES)
        });
        let name = rows
            .iter()
            .find(|r| {
                r.first().and_then(|v| v.as_str()) == Some(faction_key.as_str())
                    && r.get(1).and_then(|v| v.as_str()) == Some(post.as_str())
                    && r.get(2).and_then(|v| v.as_str()) == Some(gov.as_str())
            })
            .and_then(|r| r.get(4).and_then(|v| v.as_str()))
            .and_then(|k| loc(inner, &format!("ministerial_positions_strings_on_screen_{k}")))
            .unwrap_or_else(|| post.clone());
        t.set("Post", post)?;
        t.set("PostName", name)?;
        t.set("PostEffects", lua.create_table()?)?;
    }
    // The commanded unit's card is built from the force as the model holds it: `unit_entry` and
    // the name lookups below only take more shared borrows of the model.
    if let Some(f) = commanded.filter(|_| !show_as_character) {
        t.set("CommandedUnit", unit_entry(lua, inner, ui, f, 0, true)?)?;
    }
    drop(m);
    t.set("Name", character_name(inner, ui, c).unwrap_or_else(|| character_type_name(inner, ui, c)))?;
    Ok(Value::Table(t))
}

/// An agent attribute's picture (`PipPath` / `PrimaryAttributePath`): the one mapping every card
/// and panel uses, `agent_attributes`' icon column (see `CharacterTables::attribute_icon` for the
/// evidence and tags).
///
/// A `PLACEHOLDER` row becomes the empty path here, at the UI boundary. INFERRED: the exe hands the
/// literal on (its resolver `0x00A06F20` finds no separator and no `<skin>/PLACEHOLDER` file, so it
/// keeps the string) and no file of that name exists, so the original draws nothing; the empty path
/// draws the same nothing without a texture-not-found warning.
fn attribute_icon(ui: &CampaignUi, key: &str) -> String {
    match ui.link.db.campaign.characters.attribute_icon(key) {
        "PLACEHOLDER" => String::new(),
        icon => icon.to_owned(),
    }
}

/// An attribute's on-screen name (`agent_attributes_onscreen_name_<key>`, else the key).
fn attribute_name(inner: &Inner, key: &str) -> String {
    loc(inner, &format!("agent_attributes_onscreen_name_{key}")).unwrap_or_else(|| key.to_owned())
}

/// Whether an [`attributes_table`] carries the per-attribute pips or only the `Primary*` fields.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Pips {
    /// `[i] = {Value, PipPath, AttributeName}` too (character details, agent-action target rows).
    With,
    /// Only `Primary*` (the recruitment rows and the commander's unit card read no pips).
    Without,
}

/// The `Attributes` table every character card and panel reads (character details, recruitment
/// rows, agent-action target rows, the commander's unit card): `PrimaryAttributePath`,
/// `PrimaryLevel`, `PrimaryAttributeName`, then, with [`Pips::With`], `[i] = {Value, PipPath,
/// AttributeName}` for the character's own attribute list.
///
/// The primary attribute is the character type's main attribute (`agents::main_attribute`): the exe's
/// builder `0x009AD250` takes the character's main-attribute index (`0x00A198C0`) and fills the three
/// `Primary*` fields only when it is a valid index (< 14), CONFIRMED. `PrimaryLevel` =
/// `min(rank + 1, 9)` with the rank from `0x00A198D0` (`0x009AE759..0x009AE768`): the formula is
/// CONFIRMED; the rank it reads is our `agents::rank`, CONFIRMED for agents but PROVISIONAL for
/// generals, admirals, ministers and missionaries (their theatre / battle / army-make-up /
/// minister-post bonuses are not modelled, see `GetCharacterRank` in `CHARACTERS_FIDELITY.md`), so
/// their `PrimaryLevel` is PROVISIONAL too.
///
/// `character` is `None` for rows that are not a character (sabotage targets), and a character id
/// with no record has no type: PROVISIONAL empty `Primary*` values so the card's unguarded reads still
/// find strings (the exe always has the record, so it never takes this path).
///
/// PROVISIONAL: the exe's per-attribute part is all 14 attributes keyed by attribute key (its loop
/// `0x009AE87D..`, value from `0x009CB560`, not traced, so whether it is the base or the effective
/// level is UNKNOWN); ours is the character's own attribute list (base values) as an array.
pub(super) fn attributes_table(lua: &Lua, inner: &Inner, ui: &CampaignUi, character: Option<CharacterId>, pips: Pips) -> mlua::Result<Table> {
    let attrs = lua.create_table()?;
    let (primary, list) = match character {
        Some(c) => {
            let m = ui.model();
            let primary = m.world.characters.get(&c).map(|ch| {
                let level = (ntw_sim::campaign::agents::rank(&m, c) + 1).min(9);
                (ntw_sim::campaign::agents::main_attribute(ch.kind), level)
            });
            let list = if pips == Pips::With { character_attributes(&m, c) } else { Vec::new() };
            (primary, list)
        }
        None => (None, Vec::new()),
    };
    match primary {
        Some((key, level)) => {
            attrs.set("PrimaryLevel", level)?;
            attrs.set("PrimaryAttributePath", attribute_icon(ui, key))?;
            attrs.set("PrimaryAttributeName", attribute_name(inner, key))?;
        }
        None => {
            attrs.set("PrimaryLevel", 0)?;
            attrs.set("PrimaryAttributePath", "")?;
            attrs.set("PrimaryAttributeName", "")?;
        }
    }
    for (i, (key, value)) in list.iter().enumerate() {
        let a = lua.create_table()?;
        a.set("Value", *value)?;
        a.set("PipPath", attribute_icon(ui, key))?;
        a.set("AttributeName", attribute_name(inner, key))?;
        attrs.set(i + 1, a)?;
    }
    Ok(attrs)
}

/// A character's own attribute list (`character_details` `Attributes`' pips, the same list the
/// agents tab uses); the primary attribute is chosen by [`attributes_table`].
fn character_attributes(m: &CampaignModel, c: CharacterId) -> Vec<(String, i32)> {
    m.world.character_details.get(&c).map(|d| d.attributes.clone()).unwrap_or_default()
}

