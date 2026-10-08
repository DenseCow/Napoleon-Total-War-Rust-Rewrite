//! Scripted campaign missions: `FACTION/CAMPAIGN_MISSION_MANAGER` and its `CAMPAIGN_MISSION`
//! records.
//!
//! The layout below was recovered from the exe's own writer and reader
//! (`analysis/campaign/S1_MISSIONS_UI.md`; CONFIRMED unless a field says otherwise). No shipped
//! start position holds a mission; the user's newest `auto_save.save` holds one (France,
//! `eur_take_vienna`, capture city, 2000 money), which reads with this layout and writes back
//! identically (`tests/real_install.rs`).
//!
//!
//! - `CAMPAIGN_MISSION_MANAGER` v1 (writer `0x0099F9F0`, reader `0x0098A480`): bool, then the
//!   record array `MISSIONS` v0, one `CAMPAIGN_MISSION` record per item.
//! - `CAMPAIGN_MISSION` v2 (writer `0x0099F820`, reader `0x00989CF0`): u32, u32, i32, i32, bool,
//!   bool, ascii, utf16 (v2 only), then the records `CAMPAIGN_MISSION_OBJECTIVES`,
//!   `CAMPAIGN_MISSION_LOCALISATION_OVERRIDES` and `CAMPAIGN_MISSION_REWARDS`.
//! - `CAMPAIGN_MISSION_OBJECTIVES` v3 (writer `0x0099EF40`, reader `0x0098AA30`): u32 kind, u32
//!   settlement, u32 fort, u32 faction, utf16 building level key, utf16 unit key, u32 port (v2+),
//!   utf16 technology key (v2+), u32 character (v2+), u32 region (v3+), u32[] regions.
//! - `CAMPAIGN_MISSION_LOCALISATION_OVERRIDES` v1 (`0x0099EE40` / `0x00989FD0`): three utf16.
//! - `CAMPAIGN_MISSION_REWARDS` v3 (`0x0099F310` / `0x0098B310`): u32 money, u32 takeover
//!   faction, `GRANT_UNIT_DATA[]` {utf16, u32, u32} and `GRANT_AGENT_DATA[]` {utf16, u32}
//!   (v3; v2 holds one inline grant-unit triple, v1 none), u32 army experience, u32 navy
//!   experience, utf16 enable-recruitment unit key.
//!
//! The u32 target fields hold whatever the game had in memory; for the target kinds the mission
//! builders store **object pointers** there (CONFIRMED: `0x00A22280` calls virtuals on them), so
//! they are kept as raw numbers here.

use ntw_formats::esf::{EsfNode, EsfRecord, EsfRecordArray};

/// Record names and the versions the exe writes.
pub const MANAGER: &str = "CAMPAIGN_MISSION_MANAGER";
/// `CAMPAIGN_MISSION_MANAGER`'s record array of missions.
pub const MISSIONS: &str = "MISSIONS";
/// One mission.
pub const MISSION: &str = "CAMPAIGN_MISSION";
/// A mission's objective.
pub const OBJECTIVES: &str = "CAMPAIGN_MISSION_OBJECTIVES";
/// A mission's text overrides.
pub const LOCALISATION: &str = "CAMPAIGN_MISSION_LOCALISATION_OVERRIDES";
/// A mission's rewards.
pub const REWARDS: &str = "CAMPAIGN_MISSION_REWARDS";
const GRANT_UNIT: &str = "GRANT_UNIT_DATA";
const GRANT_AGENT: &str = "GRANT_AGENT_DATA";

const MANAGER_VERSION: u8 = 1;
const MISSIONS_VERSION: u8 = 0;
const MISSION_VERSION: u8 = 2;
const OBJECTIVES_VERSION: u8 = 3;
const LOCALISATION_VERSION: u8 = 1;
const REWARDS_VERSION: u8 = 3;
const GRANT_VERSION: u8 = 0;

/// What went wrong reading a mission record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MissionError {
    /// The record being read.
    pub record: &'static str,
    /// Which plain value (or child record) was wrong or missing.
    pub what: String,
}

impl std::fmt::Display for MissionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.record, self.what)
    }
}

impl std::error::Error for MissionError {}

/// The mission kinds (the objective's first u32). CONFIRMED from the builders the scripts'
/// `trigger_custom_mission` type strings call (`0x00977DF0` → `0x00A21xxx` → `0x0098xxxx`) and
/// the default texts of `0x00A20CD0`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MissionKind {
    /// 0 `capture_city`: target in [`MissionObjectives::settlement`].
    CaptureCity,
    /// 1 `protectorate_region_capture`: targets in [`MissionObjectives::regions`].
    ProtectorateRegionCapture,
    /// 2 `build_building`: [`MissionObjectives::building_level`].
    BuildBuilding,
    /// 3 `recruit_unit`: [`MissionObjectives::unit`].
    RecruitUnit,
    /// 4 `forge_alliance`: [`MissionObjectives::faction`].
    ForgeAlliance,
    /// 5 `capture_fort`: [`MissionObjectives::fort`].
    CaptureFort,
    /// 6 `make_peace`: [`MissionObjectives::faction`].
    MakePeace,
    /// 7 `blockade_port`: [`MissionObjectives::port`].
    BlockadePort,
    /// 8 `infiltrate_garrison`: settlement, fort and port of the garrison's holder.
    InfiltrateGarrison,
    /// 9 `research_technology`: [`MissionObjectives::technology`].
    ResearchTechnology,
    /// 10 `assassinate_character`: [`MissionObjectives::character`].
    AssassinateCharacter,
    /// 11 `make_trade_agreement`: [`MissionObjectives::faction`].
    MakeTradeAgreement,
    /// 12 `end_rebellion`: [`MissionObjectives::region`].
    EndRebellion,
    /// 13 `restore_public_order`: [`MissionObjectives::region`].
    RestorePublicOrder,
    /// 14 `liberate_region`: [`MissionObjectives::region`].
    LiberateRegion,
    /// 15 `sabotage_enemy_building`: no target field.
    SabotageEnemyBuilding,
    /// 16, the value an objective starts with before a builder sets it (`0x0098AA30`).
    Unset,
    /// Any other stored value, kept as is.
    Other(u32),
}

impl MissionKind {
    /// The kind for a stored u32.
    pub fn from_raw(v: u32) -> Self {
        use MissionKind::*;
        match v {
            0 => CaptureCity,
            1 => ProtectorateRegionCapture,
            2 => BuildBuilding,
            3 => RecruitUnit,
            4 => ForgeAlliance,
            5 => CaptureFort,
            6 => MakePeace,
            7 => BlockadePort,
            8 => InfiltrateGarrison,
            9 => ResearchTechnology,
            10 => AssassinateCharacter,
            11 => MakeTradeAgreement,
            12 => EndRebellion,
            13 => RestorePublicOrder,
            14 => LiberateRegion,
            15 => SabotageEnemyBuilding,
            16 => Unset,
            v => Other(v),
        }
    }

    /// The stored u32.
    pub fn to_raw(self) -> u32 {
        use MissionKind::*;
        match self {
            CaptureCity => 0,
            ProtectorateRegionCapture => 1,
            BuildBuilding => 2,
            RecruitUnit => 3,
            ForgeAlliance => 4,
            CaptureFort => 5,
            MakePeace => 6,
            BlockadePort => 7,
            InfiltrateGarrison => 8,
            ResearchTechnology => 9,
            AssassinateCharacter => 10,
            MakeTradeAgreement => 11,
            EndRebellion => 12,
            RestorePublicOrder => 13,
            LiberateRegion => 14,
            SabotageEnemyBuilding => 15,
            Unset => 16,
            Other(v) => v,
        }
    }

    /// The type string `trigger_custom_mission` takes for this kind (CONFIRMED strings).
    pub fn script_name(self) -> Option<&'static str> {
        use MissionKind::*;
        Some(match self {
            CaptureCity => "capture_city",
            ProtectorateRegionCapture => "protectorate_region_capture",
            BuildBuilding => "build_building",
            RecruitUnit => "recruit_unit",
            ForgeAlliance => "forge_alliance",
            CaptureFort => "capture_fort",
            MakePeace => "make_peace",
            BlockadePort => "blockade_port",
            InfiltrateGarrison => "infiltrate_garrison",
            ResearchTechnology => "research_technology",
            AssassinateCharacter => "assassinate_character",
            MakeTradeAgreement => "make_trade_agreement",
            EndRebellion => "end_rebellion",
            RestorePublicOrder => "restore_public_order",
            LiberateRegion => "liberate_region",
            SabotageEnemyBuilding => "sabotage_enemy_building",
            Unset | Other(_) => return None,
        })
    }

    /// The kind for a `trigger_custom_mission` type string.
    pub fn from_script_name(s: &str) -> Option<Self> {
        (0..16).map(Self::from_raw).find(|k| k.script_name() == Some(s))
    }
}

/// `CAMPAIGN_MISSION_OBJECTIVES`. The u32 targets are the in-memory values (object pointers for
/// the target kinds, 0 when unused); the keys are database keys (empty when unused).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MissionObjectives {
    /// What the mission asks for.
    pub kind: MissionKind,
    /// #1 settlement (`capture_city`, `infiltrate_garrison`); a heap address in the sample
    /// (0x237E9AA8), so the targets are saved as raw pointers (CONFIRMED).
    pub settlement: u32,
    /// #2 fort (`capture_fort`, `infiltrate_garrison`).
    pub fort: u32,
    /// #3 faction (`forge_alliance`, `make_peace`, `make_trade_agreement`).
    pub faction: u32,
    /// #4 `building_levels` key (`build_building`).
    pub building_level: String,
    /// #5 `units` key (`recruit_unit`).
    pub unit: String,
    /// #6 port (`blockade_port`, `infiltrate_garrison`); v2+.
    pub port: u32,
    /// #7 `technologies` key (`research_technology`); v2+.
    pub technology: String,
    /// #8 character (`assassinate_character`); v2+.
    pub character: u32,
    /// #9 region (`end_rebellion`, `restore_public_order`, `liberate_region`); v3+.
    pub region: u32,
    /// The u32 array: the regions of `protectorate_region_capture`.
    pub regions: Vec<u32>,
}

impl Default for MissionObjectives {
    fn default() -> Self {
        Self {
            kind: MissionKind::Unset,
            settlement: 0,
            fort: 0,
            faction: 0,
            building_level: String::new(),
            unit: String::new(),
            port: 0,
            technology: String::new(),
            character: 0,
            region: 0,
            regions: Vec::new(),
        }
    }
}

/// `CAMPAIGN_MISSION_LOCALISATION_OVERRIDES`: the script's own texts; an empty one makes the
/// game use its default text for the kind (`0x00A20CD0`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MissionTexts {
    /// The heading loc key.
    pub heading: String,
    /// The description loc key.
    pub description: String,
    /// The reward text loc key (built from the rewards when empty).
    pub reward: String,
}

/// One `GRANT_UNIT_DATA` item (`grant_unit:<unit>#<settlement>`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GrantUnit {
    /// The `units` key.
    pub unit: String,
    /// Two u32 the reward parser gets from the unit and settlement (`0x00A04C70`); meaning
    /// UNKNOWN.
    pub a: u32,
    /// See [`GrantUnit::a`].
    pub b: u32,
}

/// One `GRANT_AGENT_DATA` item (`grant_agent:<agent type>#<region>`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GrantAgent {
    /// The agent type.
    pub agent: String,
    /// The region (the in-memory value, INFERRED).
    pub region: u32,
}

/// `CAMPAIGN_MISSION_REWARDS`, parsed by the game from the script's reward string
/// (`0x00A16870`: comma-separated `money:`, `takeover_faction:`, `grant_unit:`,
/// `grant_experience_army:`, `grant_experience_navy:`, `enable_recruitment:`, `grant_agent:`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MissionRewards {
    /// `money:<n>`.
    pub money: u32,
    /// `takeover_faction:<faction>` (the in-memory value).
    pub takeover_faction: u32,
    /// `grant_unit:` entries.
    pub units: Vec<GrantUnit>,
    /// `grant_agent:` entries.
    pub agents: Vec<GrantAgent>,
    /// `grant_experience_army:<n>`.
    pub army_experience: u32,
    /// `grant_experience_navy:<n>`.
    pub navy_experience: u32,
    /// `enable_recruitment:<unit key>`.
    pub enable_recruitment: String,
}

/// One `CAMPAIGN_MISSION`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Mission {
    /// #0 (+0xDC): an int argument of `trigger_custom_mission` (INFERRED the turn limit, 0 =
    /// none; 0 in the one sample).
    pub turns: u32,
    /// #1 (+0xE0): 0 at creation (INFERRED the turns elapsed; 3 in the sample).
    pub elapsed: u32,
    /// #2/#3: the target's position (two i32 from the target's virtual `+0x3C`; units UNKNOWN).
    pub position: (i32, i32),
    /// #4: whether [`Mission::position`] is set (CONFIRMED: set with it by `0x00A22280`).
    pub has_position: bool,
    /// #5 (+0x100): 0 at creation; meaning UNKNOWN.
    pub flag: bool,
    /// #6 (ascii, +0xF4): the script's mission key (CONFIRMED: `eur_take_vienna` in the sample).
    pub script_key: String,
    /// #7 (utf16, +0xC4, v2): the objective label the text builder fills (CONFIRMED: "Capture
    /// city:" in the sample).
    pub display: String,
    /// What to do.
    pub objectives: MissionObjectives,
    /// The script's texts.
    pub texts: MissionTexts,
    /// What it pays.
    pub rewards: MissionRewards,
}

/// `CAMPAIGN_MISSION_MANAGER`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MissionManager {
    /// The leading bool (+0x16C of the owner; meaning UNKNOWN, false in all 16 files).
    pub flag: bool,
    /// The missions, in order.
    pub missions: Vec<Mission>,
}

// ---- reading ----

struct Values<'a> {
    record: &'static str,
    it: Box<dyn Iterator<Item = &'a EsfNode> + 'a>,
    n: usize,
}

impl<'a> Values<'a> {
    fn new(record: &'static str, rec: &'a EsfRecord) -> Self {
        Self { record, it: Box::new(rec.values()), n: 0 }
    }
    fn next(&mut self, expected: &str) -> Result<&'a EsfNode, MissionError> {
        let i = self.n;
        self.n += 1;
        self.it.next().ok_or_else(|| MissionError {
            record: self.record,
            what: format!("value #{i} ({expected}) missing"),
        })
    }
    fn bad(&self, expected: &str, found: &EsfNode) -> MissionError {
        MissionError {
            record: self.record,
            what: format!("value #{} should be {expected}, found {}", self.n - 1, found.type_name()),
        }
    }
    fn u32(&mut self) -> Result<u32, MissionError> {
        let n = self.next("u32")?;
        n.as_u32().ok_or_else(|| self.bad("u32", n))
    }
    fn i32(&mut self) -> Result<i32, MissionError> {
        let n = self.next("i32")?;
        n.as_i32().ok_or_else(|| self.bad("i32", n))
    }
    fn bool(&mut self) -> Result<bool, MissionError> {
        let n = self.next("bool")?;
        n.as_bool().ok_or_else(|| self.bad("bool", n))
    }
    fn utf16(&mut self) -> Result<String, MissionError> {
        match self.next("utf16")? {
            EsfNode::Utf16String(s) => Ok(s.clone()),
            n => Err(self.bad("utf16", n)),
        }
    }
    fn ascii(&mut self) -> Result<String, MissionError> {
        match self.next("ascii")? {
            EsfNode::AsciiString(s) => Ok(s.clone()),
            n => Err(self.bad("ascii", n)),
        }
    }
    fn u32_array(&mut self) -> Result<Vec<u32>, MissionError> {
        let n = self.next("u32 array")?;
        n.as_u32_array().map(<[u32]>::to_vec).ok_or_else(|| self.bad("u32 array", n))
    }
}

fn expect_name(rec: &EsfRecord, name: &'static str) -> Result<(), MissionError> {
    if rec.name == name {
        Ok(())
    } else {
        Err(MissionError { record: name, what: format!("record is named {}", rec.name) })
    }
}

fn child<'a>(rec: &'a EsfRecord, parent: &'static str, name: &str) -> Result<&'a EsfRecord, MissionError> {
    rec.child(name)
        .ok_or_else(|| MissionError { record: parent, what: format!("child record {name} missing") })
}

/// Reads a `CAMPAIGN_MISSION_MANAGER` record.
pub fn read_manager(rec: &EsfRecord) -> Result<MissionManager, MissionError> {
    expect_name(rec, MANAGER)?;
    let flag = Values::new(MANAGER, rec).bool()?;
    let array = rec
        .record_array(MISSIONS)
        .ok_or_else(|| MissionError { record: MANAGER, what: "record array MISSIONS missing".into() })?;
    let mut missions = Vec::with_capacity(array.items.len());
    for (i, item) in array.items.iter().enumerate() {
        let m = item.iter().find_map(EsfNode::as_record).ok_or_else(|| MissionError {
            record: MANAGER,
            what: format!("MISSIONS item {i} holds no record"),
        })?;
        missions.push(read_mission(m)?);
    }
    Ok(MissionManager { flag, missions })
}

/// Reads a `CAMPAIGN_MISSION` record (any version the exe reads: v1 has no display string).
pub fn read_mission(rec: &EsfRecord) -> Result<Mission, MissionError> {
    expect_name(rec, MISSION)?;
    let mut v = Values::new(MISSION, rec);
    let turns = v.u32()?;
    let elapsed = v.u32()?;
    let position = (v.i32()?, v.i32()?);
    let has_position = v.bool()?;
    let flag = v.bool()?;
    let script_key = v.ascii()?;
    let display = if rec.version > 1 { v.utf16()? } else { String::new() };
    Ok(Mission {
        turns,
        elapsed,
        position,
        has_position,
        flag,
        script_key,
        display,
        objectives: read_objectives(child(rec, MISSION, OBJECTIVES)?)?,
        texts: read_texts(child(rec, MISSION, LOCALISATION)?)?,
        rewards: read_rewards(child(rec, MISSION, REWARDS)?)?,
    })
}

/// Reads `CAMPAIGN_MISSION_OBJECTIVES` (v1..v3, the fields each version has).
pub fn read_objectives(rec: &EsfRecord) -> Result<MissionObjectives, MissionError> {
    expect_name(rec, OBJECTIVES)?;
    let mut v = Values::new(OBJECTIVES, rec);
    let mut o = MissionObjectives {
        kind: MissionKind::from_raw(v.u32()?),
        settlement: v.u32()?,
        fort: v.u32()?,
        faction: v.u32()?,
        building_level: v.utf16()?,
        unit: v.utf16()?,
        ..MissionObjectives::default()
    };
    if rec.version > 1 {
        o.port = v.u32()?;
        o.technology = v.utf16()?;
        o.character = v.u32()?;
    }
    if rec.version > 2 {
        o.region = v.u32()?;
    }
    o.regions = v.u32_array()?;
    Ok(o)
}

/// Reads `CAMPAIGN_MISSION_LOCALISATION_OVERRIDES`.
pub fn read_texts(rec: &EsfRecord) -> Result<MissionTexts, MissionError> {
    expect_name(rec, LOCALISATION)?;
    let mut v = Values::new(LOCALISATION, rec);
    Ok(MissionTexts { heading: v.utf16()?, description: v.utf16()?, reward: v.utf16()? })
}

/// Reads `CAMPAIGN_MISSION_REWARDS` (v1: no unit or agent grants; v2: one inline unit grant;
/// v3: the two arrays).
pub fn read_rewards(rec: &EsfRecord) -> Result<MissionRewards, MissionError> {
    expect_name(rec, REWARDS)?;
    let mut v = Values::new(REWARDS, rec);
    let mut r = MissionRewards { money: v.u32()?, takeover_faction: v.u32()?, ..Default::default() };
    if rec.version == 2 {
        let unit = v.utf16()?;
        let (a, b) = (v.u32()?, v.u32()?);
        r.units.push(GrantUnit { unit, a, b });
    } else if rec.version > 2 {
        if let Some(arr) = rec.record_array(GRANT_UNIT) {
            for item in &arr.items {
                let mut iv = ItemValues::new(GRANT_UNIT, item);
                r.units.push(GrantUnit { unit: iv.utf16()?, a: iv.u32()?, b: iv.u32()? });
            }
        }
        if let Some(arr) = rec.record_array(GRANT_AGENT) {
            for item in &arr.items {
                let mut iv = ItemValues::new(GRANT_AGENT, item);
                r.agents.push(GrantAgent { agent: iv.utf16()?, region: iv.u32()? });
            }
        }
    }
    r.army_experience = v.u32()?;
    r.navy_experience = v.u32()?;
    r.enable_recruitment = v.utf16()?;
    Ok(r)
}

/// Plain values of one record-array item.
struct ItemValues<'a> {
    record: &'static str,
    item: &'a [EsfNode],
    n: usize,
}

impl<'a> ItemValues<'a> {
    fn new(record: &'static str, item: &'a [EsfNode]) -> Self {
        Self { record, item, n: 0 }
    }
    fn next(&mut self) -> Result<&'a EsfNode, MissionError> {
        let i = self.n;
        self.n += 1;
        self.item
            .get(i)
            .ok_or_else(|| MissionError { record: self.record, what: format!("item value #{i} missing") })
    }
    fn u32(&mut self) -> Result<u32, MissionError> {
        let n = self.next()?;
        n.as_u32().ok_or_else(|| MissionError {
            record: self.record,
            what: format!("item value #{} should be u32, found {}", self.n - 1, n.type_name()),
        })
    }
    fn utf16(&mut self) -> Result<String, MissionError> {
        match self.next()? {
            EsfNode::Utf16String(s) => Ok(s.clone()),
            n => Err(MissionError {
                record: self.record,
                what: format!("item value #{} should be utf16, found {}", self.n - 1, n.type_name()),
            }),
        }
    }
}

// ---- writing (the layout and versions the exe writes) ----

fn rec(name: &str, version: u8, children: Vec<EsfNode>) -> EsfNode {
    EsfNode::Record(Box::new(EsfRecord { name: name.into(), version, children }))
}

/// Writes a `CAMPAIGN_MISSION_MANAGER` v1 record.
pub fn write_manager(m: &MissionManager) -> EsfRecord {
    let mut arr = EsfRecordArray::new(MISSIONS, MISSIONS_VERSION);
    arr.items = m.missions.iter().map(|x| vec![EsfNode::Record(Box::new(write_mission(x)))]).collect();
    EsfRecord {
        name: MANAGER.into(),
        version: MANAGER_VERSION,
        children: vec![EsfNode::Bool(m.flag), EsfNode::RecordArray(Box::new(arr))],
    }
}

/// Writes a `CAMPAIGN_MISSION` v2 record.
pub fn write_mission(m: &Mission) -> EsfRecord {
    let o = &m.objectives;
    let objectives = vec![
        EsfNode::U32(o.kind.to_raw()),
        EsfNode::U32(o.settlement),
        EsfNode::U32(o.fort),
        EsfNode::U32(o.faction),
        EsfNode::Utf16String(o.building_level.clone()),
        EsfNode::Utf16String(o.unit.clone()),
        EsfNode::U32(o.port),
        EsfNode::Utf16String(o.technology.clone()),
        EsfNode::U32(o.character),
        EsfNode::U32(o.region),
        EsfNode::U32Array(o.regions.clone()),
    ];
    let t = &m.texts;
    let texts = vec![
        EsfNode::Utf16String(t.heading.clone()),
        EsfNode::Utf16String(t.description.clone()),
        EsfNode::Utf16String(t.reward.clone()),
    ];
    let r = &m.rewards;
    let mut units = EsfRecordArray::new(GRANT_UNIT, GRANT_VERSION);
    units.items = r
        .units
        .iter()
        .map(|u| vec![EsfNode::Utf16String(u.unit.clone()), EsfNode::U32(u.a), EsfNode::U32(u.b)])
        .collect();
    let mut agents = EsfRecordArray::new(GRANT_AGENT, GRANT_VERSION);
    agents.items =
        r.agents.iter().map(|a| vec![EsfNode::Utf16String(a.agent.clone()), EsfNode::U32(a.region)]).collect();
    let rewards = vec![
        EsfNode::U32(r.money),
        EsfNode::U32(r.takeover_faction),
        EsfNode::RecordArray(Box::new(units)),
        EsfNode::RecordArray(Box::new(agents)),
        EsfNode::U32(r.army_experience),
        EsfNode::U32(r.navy_experience),
        EsfNode::Utf16String(r.enable_recruitment.clone()),
    ];
    EsfRecord {
        name: MISSION.into(),
        version: MISSION_VERSION,
        children: vec![
            EsfNode::U32(m.turns),
            EsfNode::U32(m.elapsed),
            EsfNode::I32(m.position.0),
            EsfNode::I32(m.position.1),
            EsfNode::Bool(m.has_position),
            EsfNode::Bool(m.flag),
            EsfNode::AsciiString(m.script_key.clone()),
            EsfNode::Utf16String(m.display.clone()),
            rec(OBJECTIVES, OBJECTIVES_VERSION, objectives),
            rec(LOCALISATION, LOCALISATION_VERSION, texts),
            rec(REWARDS, REWARDS_VERSION, rewards),
        ],
    }
}

/// Settlement id (`SETTLEMENT` #4) → the id of the region holding it (`REGION` #4), for every
/// `REGION` record under `root` (the mission targets name settlements by their ids).
pub fn settlement_regions(root: &EsfRecord) -> std::collections::BTreeMap<u32, ntw_sim::campaign::RegionId> {
    fn walk(r: &EsfRecord, out: &mut std::collections::BTreeMap<u32, ntw_sim::campaign::RegionId>) {
        if r.name == "REGION" {
            if let (Some(rid), Some(sid)) = (
                r.get(4).and_then(EsfNode::as_int),
                r.child("SETTLEMENT").and_then(|s| s.get(4)).and_then(EsfNode::as_int),
            ) {
                out.insert(sid as u32, ntw_sim::campaign::RegionId(rid as u32));
            }
            return;
        }
        for c in &r.children {
            match c {
                EsfNode::Record(b) => walk(b, out),
                EsfNode::RecordArray(a) => {
                    for n in a.items.iter().flatten() {
                        if let EsfNode::Record(b) = n {
                            walk(b, out);
                        }
                    }
                }
                _ => {}
            }
        }
    }
    let mut out = std::collections::BTreeMap::new();
    walk(root, &mut out);
    out
}

/// A mission as the campaign model keeps it, with its saved target ids mapped to the loaded
/// objects (CONFIRMED rule: the original's faction post-load fix-up `0x008E07F0` → `0x00A18670`
/// maps every objective target (settlement, fort, faction, port, character, region and the
/// region list, `0x00A186A0`) and every reward target (takeover faction, the two `grant_unit`
/// values, the `grant_agent` regions, `0x00A18720`) through the global id → object map
/// `0x0105AC60`, then rebuilds the texts `0x00A20CD0`). Ids are the objects' saved ids: a
/// settlement's is `SETTLEMENT` #4, a region's `REGION` #4, a faction's and a character's their
/// record ids (CONFIRMED values of the same pointer-like form). Forts and ports stay raw (not
/// modelled).
pub fn to_model(
    m: &Mission,
    world: &ntw_sim::campaign::World,
    settlement_region: &std::collections::BTreeMap<u32, ntw_sim::campaign::RegionId>,
) -> ntw_sim::campaign::details::CampaignMission {
    use ntw_sim::campaign::details::MissionTarget;
    use ntw_sim::campaign::{CharacterId, FactionId, RegionId};
    fn target<T>(raw: u32, find: impl Fn(u32) -> Option<T>) -> MissionTarget<T> {
        match raw {
            0 => MissionTarget::None,
            r => find(r).map_or(MissionTarget::Unresolved(r), MissionTarget::Found),
        }
    }
    let region = |r: u32| world.regions.contains_key(&RegionId(r)).then_some(RegionId(r));
    let faction = |r: u32| world.factions.contains_key(&FactionId(r as i32)).then_some(FactionId(r as i32));
    let character = |r: u32| world.characters.contains_key(&CharacterId(r as i32)).then_some(CharacterId(r as i32));
    let o = &m.objectives;
    fn opt<T>(t: MissionTarget<T>) -> Option<MissionTarget<T>> {
        if matches!(t, MissionTarget::None) { None } else { Some(t) }
    }
    ntw_sim::campaign::details::CampaignMission {
        script_key: m.script_key.clone(),
        kind: o.kind.to_raw(),
        turns: m.turns,
        elapsed: m.elapsed,
        settlement: opt(target(o.settlement, |r| settlement_region.get(&r).copied())),
        fort: o.fort,
        faction: opt(target(o.faction, faction)),
        port: o.port,
        character: opt(target(o.character, character)),
        region: opt(target(o.region, region)),
        regions: o.regions.iter().map(|&r| target(r, region)).collect(),
        building_level: o.building_level.clone(),
        unit: o.unit.clone(),
        technology: o.technology.clone(),
        reward_money: m.rewards.money,
        reward_takeover: opt(target(m.rewards.takeover_faction, faction)),
        reward_agents: m.rewards.agents.iter().map(|a| (a.agent.clone(), target(a.region, region))).collect(),
        reward_units: m.rewards.units.iter().map(|u| (u.unit.clone(), u.a, u.b)).collect(),
        reward_experience: (m.rewards.army_experience, m.rewards.navy_experience),
        reward_recruitment: m.rewards.enable_recruitment.clone(),
    }
}

/// Every `CAMPAIGN_MISSION_MANAGER` under `root` (they sit in `FACTION` records), with the key of
/// the enclosing `FACTION` (its plain value #1; empty when there is none).
pub fn find_managers(root: &EsfRecord) -> Vec<(String, &EsfRecord)> {
    fn walk<'a>(r: &'a EsfRecord, faction: &str, out: &mut Vec<(String, &'a EsfRecord)>) {
        let key = if r.name == "FACTION" {
            r.values().nth(1).and_then(EsfNode::as_str).unwrap_or("").to_string()
        } else {
            faction.to_string()
        };
        for c in &r.children {
            match c {
                EsfNode::Record(b) if b.name == MANAGER => out.push((key.clone(), b)),
                EsfNode::Record(b) => walk(b, &key, out),
                EsfNode::RecordArray(a) => {
                    for it in &a.items {
                        for n in it {
                            if let EsfNode::Record(b) = n {
                                walk(b, &key, out);
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    }
    let mut out = Vec::new();
    walk(root, "", &mut out);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use ntw_formats::esf::EsfFile;

    fn sample() -> Mission {
        Mission {
            turns: 12,
            elapsed: 3,
            position: (-222_517_056, 104_857_600),
            has_position: true,
            flag: false,
            script_key: "fra_capture_vienna".into(),
            display: "Vienna".into(),
            objectives: MissionObjectives {
                kind: MissionKind::CaptureCity,
                settlement: 0x1234_5678,
                regions: vec![7, 9],
                building_level: "barracks".into(),
                technology: "mil_drill".into(),
                region: 42,
                ..Default::default()
            },
            texts: MissionTexts {
                heading: "fra_vienna_heading".into(),
                description: "fra_vienna_text".into(),
                reward: String::new(),
            },
            rewards: MissionRewards {
                money: 2000,
                takeover_faction: 0,
                units: vec![
                    GrantUnit { unit: "fra_inf_line".into(), a: 1, b: 2 },
                    GrantUnit { unit: "fra_cav_hussars".into(), a: 3, b: 4 },
                ],
                agents: vec![GrantAgent { agent: "spy".into(), region: 77 }],
                army_experience: 1,
                navy_experience: 0,
                enable_recruitment: "fra_inf_guard".into(),
            },
        }
    }

    #[test]
    fn mission_round_trips_through_records() {
        let m = sample();
        assert_eq!(read_mission(&write_mission(&m)).unwrap(), m);
        let mgr = MissionManager { flag: true, missions: vec![m.clone(), Mission::default()] };
        assert_eq!(read_manager(&write_manager(&mgr)).unwrap(), mgr);
    }

    #[test]
    fn manager_round_trips_through_esf_bytes() {
        let mgr = MissionManager { flag: false, missions: vec![sample()] };
        let mut faction = EsfRecord::new("FACTION", 18);
        faction.children.push(EsfNode::I32(5));
        faction.children.push(EsfNode::Utf16String("france".into()));
        faction.children.push(EsfNode::Record(Box::new(write_manager(&mgr))));
        let mut root = EsfRecord::new("CAMPAIGN_SAVE_GAME", 0);
        root.children.push(EsfNode::Record(Box::new(faction)));
        let bytes = EsfFile::new(root).to_bytes().unwrap();
        let back = EsfFile::from_bytes(&bytes).unwrap();
        let found = find_managers(&back.root);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].0, "france");
        assert_eq!(read_manager(found[0].1).unwrap(), mgr);
        // writing what was read gives the same record again
        assert_eq!(&write_manager(&read_manager(found[0].1).unwrap()), found[0].1);
    }

    #[test]
    fn written_layout_matches_the_exe_writer() {
        let r = write_mission(&sample());
        assert_eq!((r.name.as_str(), r.version), (MISSION, 2));
        let types: Vec<&str> = r.values().map(EsfNode::type_name).collect();
        let expect: Vec<&str> = [
            EsfNode::U32(0),
            EsfNode::U32(0),
            EsfNode::I32(0),
            EsfNode::I32(0),
            EsfNode::Bool(false),
            EsfNode::Bool(false),
            EsfNode::AsciiString(String::new()),
            EsfNode::Utf16String(String::new()),
        ]
        .iter()
        .map(EsfNode::type_name)
        .collect();
        assert_eq!(types, expect);
        let o = r.child(OBJECTIVES).unwrap();
        assert_eq!(o.version, 3);
        assert_eq!(o.values().count(), 11);
        let rw = r.child(REWARDS).unwrap();
        assert_eq!(rw.version, 3);
        assert!(rw.record_array(GRANT_UNIT).is_some() && rw.record_array(GRANT_AGENT).is_some());
        assert_eq!(r.child(LOCALISATION).unwrap().values().count(), 3);
    }

    #[test]
    fn older_versions_read() {
        // OBJECTIVES v1: no port / technology / character / region.
        let mut o = EsfRecord::new(OBJECTIVES, 1);
        o.children = vec![
            EsfNode::U32(3),
            EsfNode::U32(0),
            EsfNode::U32(0),
            EsfNode::U32(0),
            EsfNode::Utf16String(String::new()),
            EsfNode::Utf16String("fra_inf_line".into()),
            EsfNode::U32Array(vec![]),
        ];
        let got = read_objectives(&o).unwrap();
        assert_eq!(got.kind, MissionKind::RecruitUnit);
        assert_eq!(got.unit, "fra_inf_line");
        // REWARDS v2: one inline unit grant.
        let mut r = EsfRecord::new(REWARDS, 2);
        r.children = vec![
            EsfNode::U32(500),
            EsfNode::U32(0),
            EsfNode::Utf16String("fra_art_foot".into()),
            EsfNode::U32(1),
            EsfNode::U32(2),
            EsfNode::U32(0),
            EsfNode::U32(0),
            EsfNode::Utf16String(String::new()),
        ];
        let got = read_rewards(&r).unwrap();
        assert_eq!(got.money, 500);
        assert_eq!(got.units, vec![GrantUnit { unit: "fra_art_foot".into(), a: 1, b: 2 }]);
        // MISSION v1: no display string.
        let mut m = write_mission(&sample());
        m.version = 1;
        m.children.remove(7);
        let got = read_mission(&m).unwrap();
        assert_eq!(got.display, "");
        assert_eq!(got.turns, 12);
    }

    #[test]
    fn bad_input_is_an_error_not_a_panic() {
        let mut m = write_mission(&sample());
        m.children[0] = EsfNode::Bool(true);
        assert!(read_mission(&m).is_err());
        let empty = EsfRecord::new(MANAGER, 1);
        assert!(read_manager(&empty).is_err());
        assert!(read_mission(&EsfRecord::new("OTHER", 2)).is_err());
    }

    #[test]
    fn kinds_and_script_names() {
        for v in 0..18 {
            assert_eq!(MissionKind::from_raw(v).to_raw(), v);
        }
        assert_eq!(MissionKind::from_script_name("liberate_region"), Some(MissionKind::LiberateRegion));
        assert_eq!(MissionKind::from_script_name("capture_city").map(MissionKind::to_raw), Some(0));
        assert_eq!(MissionKind::from_script_name("nonsense"), None);
    }
}
