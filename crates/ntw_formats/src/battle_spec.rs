//! Historical battle specifications: the `.xml` files a `battles` DB record names
//! (`napoleon_historical_battles\arcole\arcole_battle.xml`, `mp_historical_battles\...`).
//!
//! Structure (CONFIRMED in all 29 shipped files, see `analysis/battle/BATTLE_FLOW.md` §1):
//! ```text
//! battle
//!   alliance id=N                       (one per side)
//!     [non_playable]                    (the whole alliance is AI-only, CONFIRMED 0x0050AFE0)
//!     army | reinforcement_army approach_angle=deg
//!       [non_playable] time_period faction   (non_playable: this army is AI-only, CONFIRMED 0x0050CAE0)
//!       deployment_area { centre x y, width metres, height metres, orientation radians }
//!       camera_start_position x y z, camera_target_position x y z
//!       campaign_ai_battle_hints aggression
//!       unit unit_category num_soldiers [script_name]
//!         [manually_deployed] unit_type type, position x y, orientation radians, width metres,
//!         [general general_category { name, experience | star_rating level, portrait }],
//!         unit_capabilities { special_ability*, shot_type*, bayonet_type }, unit_experience level
//!       ship ... (naval battles)
//!     victory_condition { kill_or_rout_enemy | sink_or_surrender_enemy }
//!     rout_position x y
//!   battle_description { battle_script, time_of_day, type, duration, timeout_winning_alliance_index }
//!   weather { prevailing_wind x y, lighting }, Climate { name }, Season { name }
//!   battle_map_definition { name }      (the terrain preset folder)
//!   playable_area dimension centre_x centre_y
//! ```
//! Positions are map metres in the same frame as the terrain preset's `deployment_areas.xml`
//! (INFERRED: units of each army stand inside the preset's areas). Orientations use the
//! deployment-area convention: 0 = facing +y, π/2 = facing +x (INFERRED, install test
//! `armies_face_each_other`). Camera positions are `(x, height, y)` (INFERRED: Arcole's camera
//! stands just behind the French line).
//!
//! The files declare UTF-8 but some names are Latin-1 (`Blücher`); they are read lossily.

use crate::battle_terrain::DeploymentArea;
use crate::xml::{self, XmlElement, XmlError};

/// A whole battle specification.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct BattleSpec {
    /// The alliances (sides) in file order; index = alliance id order.
    pub alliances: Vec<SpecAlliance>,
    /// `battle_description`.
    pub description: BattleDescription,
    /// `battle_map_definition/name`, e.g. `BattleTerrain/presets/HB_Arcole/`.
    pub map_definition: Option<String>,
    /// `playable_area`: (dimension, centre x, centre y) in metres.
    pub playable_area: Option<(f32, (f32, f32))>,
    /// `weather/prevailing_wind` (x, y).
    pub prevailing_wind: Option<(f32, f32)>,
    /// `Climate/name`, e.g. `lc_eu_central_humid`.
    pub climate: Option<String>,
    /// `Season/name`, e.g. `season_summer`.
    pub season: Option<String>,
    /// `skip_deployment` present (uncommented) at the top level.
    pub skip_deployment: bool,
}

/// `battle_description`.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct BattleDescription {
    /// `battle_script`, e.g. `Arcole_Battle`.
    pub battle_script: Option<String>,
    /// `time_of_day`, e.g. `midday`.
    pub time_of_day: Option<String>,
    /// `type`: `land_normal`, `normal` (naval files), ...
    pub kind: Option<String>,
    /// `duration`: seconds of battle time counted from the end of deployment, read as an `f32`
    /// (CONFIRMED, parser `0x00511210` and the time-out test `0x00582FB0`; see
    /// `ntw_sim::battle::victory`).
    pub duration: Option<f32>,
    /// `timeout_winning_alliance_index`: the alliance that wins when time runs out.
    pub timeout_winner: Option<usize>,
}

/// One alliance (side).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SpecAlliance {
    /// `alliance id`.
    pub id: u32,
    /// A `non_playable` child of the alliance: the whole alliance cannot be the player's
    /// (CONFIRMED: the alliance parser `0x0050AFE0` clears alliance `+0x38` for it).
    pub non_playable: bool,
    /// Armies present at the start.
    pub armies: Vec<SpecArmy>,
    /// `reinforcement_army` elements (they arrive later; when is UNKNOWN).
    pub reinforcements: Vec<SpecArmy>,
    /// `victory_condition` children, e.g. `kill_or_rout_enemy`.
    pub victory_conditions: Vec<String>,
    /// `rout_position` (x, y): where this alliance's routing units run to (INFERRED).
    pub rout_position: Option<(f32, f32)>,
}

/// One army (or reinforcement army).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SpecArmy {
    /// `faction` key, e.g. `ita_french_republic`.
    pub faction: String,
    /// `time_period`, e.g. `late`.
    pub time_period: Option<String>,
    /// A `non_playable` element inside the army (CONFIRMED: clears army `+0xCD`).
    pub non_playable: bool,
    /// `deployment_area`.
    pub deployment_area: Option<DeploymentArea>,
    /// `camera_start_position` (x, height, y).
    pub camera_start: Option<[f32; 3]>,
    /// `camera_target_position` (x, height, y).
    pub camera_target: Option<[f32; 3]>,
    /// `campaign_ai_battle_hints aggression`.
    pub ai_aggression: Option<i64>,
    /// `reinforcement_army approach_angle` in degrees.
    pub approach_angle: Option<f32>,
    /// Land units.
    pub units: Vec<SpecUnit>,
    /// Number of `ship` elements (naval battles; not read further yet).
    pub ships: usize,
}

/// One land unit.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SpecUnit {
    /// `unit_type type`: a `units` key, e.g. `Inf_Gren_French_Grenadiers`.
    pub unit_type: String,
    /// `unit_category`: infantry, cavalry, artillery, dragoons, ...
    pub category: String,
    /// `num_soldiers`.
    pub num_soldiers: Option<u32>,
    /// `script_name` (battle scripts address units by it).
    pub script_name: Option<String>,
    /// `position` (x, y) in map metres.
    pub position: (f32, f32),
    /// `orientation radians` (0 = +y, π/2 = +x).
    pub orientation: f32,
    /// `width metres`: the frontage.
    pub width: Option<f32>,
    /// `unit_experience level`.
    pub experience: Option<u32>,
    /// `manually_deployed`.
    pub manually_deployed: bool,
    /// `general`, if the unit is a general's unit.
    pub general: Option<SpecGeneral>,
    /// `unit_capabilities/special_ability`.
    pub special_abilities: Vec<String>,
    /// `unit_capabilities/shot_type`.
    pub shot_types: Vec<String>,
    /// `unit_capabilities/bayonet_type`.
    pub bayonet_type: Option<String>,
}

/// A unit's `general` element.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SpecGeneral {
    /// `general_category` attribute (e.g. `napoleon`).
    pub category: Option<String>,
    /// `name`: a plain name or a number (INFERRED: a names-table id, e.g. `1255960554`).
    pub name: String,
    /// `experience`.
    pub experience: Option<u32>,
    /// `star_rating level`.
    pub star_rating: Option<u32>,
    /// `portrait` image path.
    pub portrait: Option<String>,
}

impl BattleSpec {
    /// Reads a battle file from its bytes.
    pub fn parse(bytes: &[u8]) -> Result<Self, XmlError> {
        let root = xml::parse_bytes(bytes)?;
        if !root.name.eq_ignore_ascii_case("battle") {
            return Err(XmlError(format!("root is <{}>, not <battle>", root.name)));
        }
        let text = |e: Option<&XmlElement>| e.map(|e| e.text.clone()).filter(|t| !t.is_empty());
        let mut spec = BattleSpec::default();
        for a in root.children_named("alliance") {
            spec.alliances.push(alliance(a));
        }
        if let Some(d) = root.child("battle_description") {
            spec.description = BattleDescription {
                battle_script: text(d.child("battle_script")),
                time_of_day: text(d.child("time_of_day")),
                kind: text(d.child("type")),
                duration: text(d.child("duration")).and_then(|t| t.parse().ok()),
                timeout_winner: text(d.child("timeout_winning_alliance_index")).and_then(|t| t.parse().ok()),
            };
        }
        spec.map_definition = text(root.child("battle_map_definition").and_then(|m| m.child("name")));
        spec.playable_area = root.child("playable_area").and_then(|p| {
            Some((p.attr_f32("dimension")?, (p.attr_f32("centre_x").unwrap_or(0.0), p.attr_f32("centre_y").unwrap_or(0.0))))
        });
        spec.prevailing_wind = root.child("weather").and_then(|w| w.child("prevailing_wind")).and_then(xy);
        spec.climate = text(root.child("Climate").and_then(|c| c.child("name")));
        spec.season = text(root.child("Season").and_then(|c| c.child("name")));
        spec.skip_deployment = root.child("skip_deployment").is_some();
        Ok(spec)
    }

    /// The alliance the human player commands and its first playable army: the first alliance
    /// without an alliance-level `non_playable`, and its first army without an army-level one.
    /// The two flags are CONFIRMED reads (alliance `+0x38`, army `+0xCD`; the XML element is a
    /// name → children map, so an alliance-level `non_playable` anywhere among the alliance's
    /// children counts). That the player takes the first such army is PROVISIONAL (the front
    /// end's choice is not traced). Friedland flags **both** alliances (its first one has the
    /// element between its two French armies), so when no alliance is playable we fall back to
    /// the first army without an army-level flag (PROVISIONAL).
    pub fn player_army(&self) -> Option<(usize, usize)> {
        let pick = |only_playable: bool| {
            self.alliances
                .iter()
                .enumerate()
                .filter(|(_, a)| !only_playable || !a.non_playable)
                .find_map(|(ai, a)| a.armies.iter().position(|r| !r.non_playable).map(|ri| (ai, ri)))
        };
        pick(true).or_else(|| pick(false))
    }

    /// True if the battle has ships and no land units (a naval battle).
    pub fn is_naval(&self) -> bool {
        let all = || self.alliances.iter().flat_map(|a| a.armies.iter());
        all().any(|a| a.ships > 0) && all().all(|a| a.units.is_empty())
    }
}

fn xy(e: &XmlElement) -> Option<(f32, f32)> {
    Some((e.attr_f32("x")?, e.attr_f32("y")?))
}

fn xyz(e: &XmlElement) -> Option<[f32; 3]> {
    Some([e.attr_f32("x")?, e.attr_f32("y")?, e.attr_f32("z")?])
}

fn alliance(a: &XmlElement) -> SpecAlliance {
    let mut out = SpecAlliance { id: a.attr_i64("id").unwrap_or(0).max(0) as u32, ..Default::default() };
    for c in &a.children {
        match c.name.to_ascii_lowercase().as_str() {
            "non_playable" => out.non_playable = true,
            "army" => out.armies.push(army(c)),
            "reinforcement_army" => {
                let mut army = army(c);
                army.approach_angle = c.attr_f32("approach_angle");
                out.reinforcements.push(army);
            }
            "victory_condition" => out.victory_conditions.extend(c.children.iter().map(|v| v.name.clone())),
            "rout_position" => out.rout_position = xy(c),
            _ => {}
        }
    }
    out
}

fn army(a: &XmlElement) -> SpecArmy {
    let text = |n: &str| a.child(n).map(|e| e.text.clone()).filter(|t| !t.is_empty());
    SpecArmy {
        faction: text("faction").unwrap_or_default(),
        time_period: text("time_period"),
        non_playable: a.child("non_playable").is_some(),
        deployment_area: a.child("deployment_area").map(deployment_area),
        camera_start: a.child("camera_start_position").and_then(xyz),
        camera_target: a.child("camera_target_position").and_then(xyz),
        ai_aggression: a.child("campaign_ai_battle_hints").and_then(|h| h.attr_i64("aggression")),
        approach_angle: None,
        units: a.children_named("unit").map(unit).collect(),
        ships: a.children_named("ship").count(),
    }
}

fn deployment_area(d: &XmlElement) -> DeploymentArea {
    DeploymentArea {
        id: 0,
        centre: d.child("centre").and_then(xy).unwrap_or((0.0, 0.0)),
        width: d.child("width").and_then(|w| w.attr_f32("metres")).unwrap_or(0.0),
        height: d.child("height").and_then(|w| w.attr_f32("metres")).unwrap_or(0.0),
        orientation: d.child("orientation").and_then(|w| w.attr_f32("radians")).unwrap_or(0.0),
    }
}

fn unit(u: &XmlElement) -> SpecUnit {
    let caps = u.child("unit_capabilities");
    let list = |n: &str| -> Vec<String> {
        caps.map(|c| c.children_named(n).map(|e| e.text.clone()).filter(|t| !t.is_empty()).collect()).unwrap_or_default()
    };
    SpecUnit {
        unit_type: u.child("unit_type").and_then(|t| t.attr("type")).unwrap_or_default().to_owned(),
        category: u.attr("unit_category").unwrap_or_default().to_owned(),
        num_soldiers: u.attr_i64("num_soldiers").and_then(|n| u32::try_from(n).ok()),
        script_name: u.attr("script_name").map(str::to_owned),
        position: u.child("position").and_then(xy).unwrap_or((0.0, 0.0)),
        orientation: u.child("orientation").and_then(|o| o.attr_f32("radians")).unwrap_or(0.0),
        width: u.child("width").and_then(|w| w.attr_f32("metres")),
        experience: u.child("unit_experience").and_then(|e| e.attr_i64("level")).and_then(|n| u32::try_from(n).ok()),
        manually_deployed: u.child("manually_deployed").is_some_and(|m| m.text.eq_ignore_ascii_case("true")),
        general: u.child("general").map(|g| SpecGeneral {
            category: g.attr("general_category").map(str::to_owned),
            name: g.child("name").map(|n| n.text.clone()).unwrap_or_default(),
            experience: g.child("experience").and_then(|e| e.text.parse().ok()),
            star_rating: g.child("star_rating").and_then(|s| s.attr_i64("level")).and_then(|n| u32::try_from(n).ok()),
            portrait: g.child("portrait").map(|p| p.text.clone()).filter(|t| !t.is_empty()),
        }),
        special_abilities: list("special_ability"),
        shot_types: list("shot_type"),
        bayonet_type: caps.and_then(|c| c.child("bayonet_type")).map(|b| b.text.clone()).filter(|t| !t.is_empty()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<battle>
  <alliance id="0">
    <army>
      <time_period>late</time_period>
      <faction>france</faction>
      <deployment_area><centre x="0" y="-10"/><width metres="1600.0"/><height metres="800"/><orientation radians="0"/></deployment_area>
      <camera_start_position x="-113.2" y="47" z="-746.9" />
      <unit unit_category="cavalry" num_soldiers="24">
        <unit_type type="Gen_Early_Napoleon"/>
        <position x="-108.80" y="-661.39"/>
        <orientation radians="0.42"/>
        <width metres="19.90"/>
        <general general_category="napoleon"><name>Napoleon Bonaparte</name><experience>5</experience></general>
        <unit_capabilities><special_ability>rally</special_ability><shot_type>grenade</shot_type></unit_capabilities>
        <unit_experience level="5"/>
      </unit>
    </army>
    <victory_condition><kill_or_rout_enemy></kill_or_rout_enemy></victory_condition>
    <rout_position x ="0.0" y = "2000.0"></rout_position>
  </alliance>
  <alliance id="1">
    <non_playable></non_playable>
    <army><faction>austria</faction></army>
    <reinforcement_army approach_angle="74.0"><faction>prussia</faction>
      <unit unit_category="dragoons" num_soldiers="60" script_name="Sohr"><manually_deployed>true</manually_deployed><unit_type type="X"/></unit>
    </reinforcement_army>
  </alliance>
  <battle_description><battle_script>Arcole_Battle</battle_script><type>land_normal</type><duration>2100</duration><timeout_winning_alliance_index>1</timeout_winning_alliance_index></battle_description>
  <battle_map_definition><name>BattleTerrain/presets/HB_Arcole/</name></battle_map_definition>
  <playable_area dimension = "1400" centre_x = "-200" centre_y = "-100"></playable_area>
</battle>"#;

    #[test]
    fn reads_the_structure() {
        let s = BattleSpec::parse(SAMPLE.as_bytes()).unwrap();
        assert_eq!(s.alliances.len(), 2);
        let a0 = &s.alliances[0].armies[0];
        assert_eq!(a0.faction, "france");
        assert_eq!(a0.deployment_area.as_ref().unwrap().height, 800.0);
        assert_eq!(a0.camera_start, Some([-113.2, 47.0, -746.9]));
        let u = &a0.units[0];
        assert_eq!((u.unit_type.as_str(), u.num_soldiers, u.width), ("Gen_Early_Napoleon", Some(24), Some(19.9)));
        assert_eq!(u.general.as_ref().unwrap().name, "Napoleon Bonaparte");
        assert_eq!(u.special_abilities, vec!["rally"]);
        assert_eq!(s.alliances[0].victory_conditions, vec!["kill_or_rout_enemy"]);
        assert_eq!(s.alliances[0].rout_position, Some((0.0, 2000.0)));
        assert!(s.alliances[1].non_playable && !s.alliances[1].armies[0].non_playable);
        assert_eq!(s.player_army(), Some((0, 0)));
        let r = &s.alliances[1].reinforcements[0];
        assert_eq!(r.approach_angle, Some(74.0));
        assert!(r.units[0].manually_deployed);
        assert_eq!(s.description.duration, Some(2100.0));
        assert_eq!(s.description.timeout_winner, Some(1));
        assert_eq!(s.map_definition.as_deref(), Some("BattleTerrain/presets/HB_Arcole/"));
        assert_eq!(s.playable_area, Some((1400.0, (-200.0, -100.0))));
        assert_eq!(s.player_army(), Some((0, 0)));
        assert!(!s.is_naval());
    }
}
