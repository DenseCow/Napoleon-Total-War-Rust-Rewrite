//! The custom battle's army setup files (`<user folder>\army_setups\<name>.army_setup`, write
//! class `army_setup`). Evidence: `analysis/frontend/FRONTEND_PAGES.md` "Custom battle".
//!
//! Layout, CONFIRMED from the writer `0x0048CFC0` (the record is filled from the page's setup
//! table by `0x004553B0`; the field names are CONFIRMED there and in the loader `0x00461790`):
//!
//! ```text
//! u32   3                      version
//! i32   Era                    0 / 1 / 2
//! i32   ArmySize               the army size's funds
//! str8  Faction                u16 length + bytes
//! u8    IsHuman
//! str16 Name                   u16 length + UTF-16LE
//! u32   cards; per card:       ships first, then land units (the record's order)
//!       str16 Key, i32 Experience, u8 commander (IsAdmiral / IsGeneral), str16 ShipName
//! u32   limits; per limit:     the setup's array part
//!       i32 Max, i32 Actual, str8 Tag
//! ```
//!
//! The reader is the writer's mirror (INFERRED: the exe's reader `0x00454EA0` was not decompiled).

/// The file's version word (CONFIRMED, `0x0130CA0C`).
pub const VERSION: u32 = 3;

/// One unit card of a setup.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Card {
    /// `units` key.
    pub key: String,
    pub experience: i32,
    /// IsGeneral (land) / IsAdmiral (sea).
    pub commander: bool,
    /// ShipName (empty on land).
    pub ship_name: String,
}

/// One category limit (`{Max, Actual, Tag}`).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Limit {
    pub max: i32,
    pub actual: i32,
    pub tag: String,
}

/// An army setup file.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ArmySetupFile {
    pub era: i32,
    pub army_size: i32,
    pub faction: String,
    pub is_human: bool,
    pub name: String,
    pub cards: Vec<Card>,
    pub limits: Vec<Limit>,
}

fn str8(out: &mut Vec<u8>, s: &str) {
    let b = s.as_bytes();
    let n = b.len().min(u16::MAX as usize);
    out.extend_from_slice(&(n as u16).to_le_bytes());
    out.extend_from_slice(&b[..n]);
}

fn str16(out: &mut Vec<u8>, s: &str) {
    let w: Vec<u16> = s.encode_utf16().take(u16::MAX as usize).collect();
    out.extend_from_slice(&(w.len() as u16).to_le_bytes());
    for c in w {
        out.extend_from_slice(&c.to_le_bytes());
    }
}

struct Reader<'a> {
    b: &'a [u8],
    p: usize,
}

impl Reader<'_> {
    fn take(&mut self, n: usize) -> Result<&[u8], String> {
        let s = self.b.get(self.p..self.p + n).ok_or_else(|| format!("army setup: truncated at byte {}", self.p))?;
        self.p += n;
        Ok(s)
    }
    fn u8(&mut self) -> Result<u8, String> {
        Ok(self.take(1)?[0])
    }
    fn u16(&mut self) -> Result<u16, String> {
        let s = self.take(2)?;
        Ok(u16::from_le_bytes([s[0], s[1]]))
    }
    fn u32(&mut self) -> Result<u32, String> {
        let s = self.take(4)?;
        Ok(u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
    }
    fn i32(&mut self) -> Result<i32, String> {
        Ok(self.u32()? as i32)
    }
    fn str8(&mut self) -> Result<String, String> {
        let n = self.u16()? as usize;
        Ok(String::from_utf8_lossy(self.take(n)?).into_owned())
    }
    fn str16(&mut self) -> Result<String, String> {
        let n = self.u16()? as usize;
        let s = self.take(n * 2)?;
        let w: Vec<u16> = s.as_chunks::<2>().0.iter().map(|c| u16::from_le_bytes(*c)).collect();
        Ok(String::from_utf16_lossy(&w))
    }
}

impl ArmySetupFile {
    /// The file's bytes.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(&VERSION.to_le_bytes());
        out.extend_from_slice(&self.era.to_le_bytes());
        out.extend_from_slice(&self.army_size.to_le_bytes());
        str8(&mut out, &self.faction);
        out.push(u8::from(self.is_human));
        str16(&mut out, &self.name);
        out.extend_from_slice(&(self.cards.len() as u32).to_le_bytes());
        for c in &self.cards {
            str16(&mut out, &c.key);
            out.extend_from_slice(&c.experience.to_le_bytes());
            out.push(u8::from(c.commander));
            str16(&mut out, &c.ship_name);
        }
        out.extend_from_slice(&(self.limits.len() as u32).to_le_bytes());
        for l in &self.limits {
            out.extend_from_slice(&l.max.to_le_bytes());
            out.extend_from_slice(&l.actual.to_le_bytes());
            str8(&mut out, &l.tag);
        }
        out
    }

    /// Reads a file (any other version is refused).
    pub fn read(b: &[u8]) -> Result<ArmySetupFile, String> {
        let mut r = Reader { b, p: 0 };
        let version = r.u32()?;
        if version != VERSION {
            return Err(format!("army setup: version {version}, expected {VERSION}"));
        }
        let mut f = ArmySetupFile { era: r.i32()?, army_size: r.i32()?, faction: r.str8()?, is_human: r.u8()? != 0, name: r.str16()?, ..Default::default() };
        let n = r.u32()? as usize;
        for _ in 0..n {
            f.cards.push(Card { key: r.str16()?, experience: r.i32()?, commander: r.u8()? != 0, ship_name: r.str16()? });
        }
        let n = r.u32()? as usize;
        for _ in 0..n {
            f.limits.push(Limit { max: r.i32()?, actual: r.i32()?, tag: r.str8()? });
        }
        Ok(f)
    }
}

/// The battle preferences file's magic and version words (CONFIRMED: the writer `0x0048D2C0`
/// writes 0xBA and 8; the reader `0x00455F00` refuses another magic and reads version 8 only).
pub const PREFS_MAGIC: u32 = 0xBA;
pub const PREFS_VERSION: u32 = 8;

/// One team of a battle preferences file: its player slots and, when it has any, its armies
/// (army setup records, the `.army_setup` layout from its version word on).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PrefsTeam {
    pub players: i32,
    pub armies: Vec<ArmySetupFile>,
}

/// A battle preferences file (`<user folder>\battle_preferences\<name>.battle_preferences`,
/// write class `battle_prefs`; the custom battle pages keep their last settings in the hidden
/// `.sp_default` / `.mp_default` ones). Layout CONFIRMED from the writer `0x0048D2C0`, the
/// reader `0x00455F00` and the original's `.mp_default.battle_preferences`:
///
/// ```text
/// u32 0xBA, u32 8
/// str8 Type, str8 File, str16 Name,
/// i32 era, i32 time_of_day, str8 weather,
/// i32 wind, i32 army_size, i32 time_limit, i32 AI_strength, i32 TotalPlayers,
/// str16 game_name, str16 password, u8 ranked, u8 IsNaval,
/// str16 era_string, str16 weather_string, str16 time_limit_string,
/// i32 allowed_funds, str16 Description, str8 Image, str8 Map, i32 spectators, str8 Key,
/// u8 IsHistoric,
/// 2 teams: i32 Players; when Players > 0: u32 armies + that many army setup records
/// ```
///
/// The record offsets of the numbers and flags are CONFIRMED by the table reader `0x00455F00`;
/// which text is which is read from the sample's values (INFERRED for game_name / password,
/// both "t" there, taken in the reader's order).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BattlePrefsFile {
    pub kind: String,
    pub file: String,
    pub name: String,
    pub era: i32,
    pub time_of_day: i32,
    pub weather: String,
    pub wind: i32,
    pub army_size: i32,
    pub time_limit: i32,
    pub ai_strength: i32,
    pub total_players: i32,
    pub game_name: String,
    pub password: String,
    pub ranked: bool,
    pub is_naval: bool,
    pub era_string: String,
    pub weather_string: String,
    pub time_limit_string: String,
    pub allowed_funds: i32,
    pub description: String,
    pub image: String,
    pub map: String,
    pub spectators: i32,
    pub key: String,
    pub is_historic: bool,
    pub teams: [PrefsTeam; 2],
}

impl ArmySetupFile {
    fn read_from(r: &mut Reader) -> Result<ArmySetupFile, String> {
        let start = r.p;
        let rest = &r.b[start..];
        let f = ArmySetupFile::read(rest)?;
        r.p += f.to_bytes().len();
        Ok(f)
    }
}

impl BattlePrefsFile {
    /// The file's bytes.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut o = Vec::new();
        let i = |o: &mut Vec<u8>, v: i32| o.extend_from_slice(&v.to_le_bytes());
        i(&mut o, PREFS_MAGIC as i32);
        i(&mut o, PREFS_VERSION as i32);
        str8(&mut o, &self.kind);
        str8(&mut o, &self.file);
        str16(&mut o, &self.name);
        i(&mut o, self.era);
        i(&mut o, self.time_of_day);
        str8(&mut o, &self.weather);
        for v in [self.wind, self.army_size, self.time_limit, self.ai_strength, self.total_players] {
            i(&mut o, v);
        }
        str16(&mut o, &self.game_name);
        str16(&mut o, &self.password);
        o.push(u8::from(self.ranked));
        o.push(u8::from(self.is_naval));
        str16(&mut o, &self.era_string);
        str16(&mut o, &self.weather_string);
        str16(&mut o, &self.time_limit_string);
        i(&mut o, self.allowed_funds);
        str16(&mut o, &self.description);
        str8(&mut o, &self.image);
        str8(&mut o, &self.map);
        i(&mut o, self.spectators);
        str8(&mut o, &self.key);
        o.push(u8::from(self.is_historic));
        for t in &self.teams {
            i(&mut o, t.players);
            if t.players > 0 {
                i(&mut o, t.armies.len() as i32);
                for a in &t.armies {
                    o.extend_from_slice(&a.to_bytes());
                }
            }
        }
        o
    }

    /// Reads a file (another magic or version is refused, as the exe does).
    pub fn read(b: &[u8]) -> Result<BattlePrefsFile, String> {
        let mut r = Reader { b, p: 0 };
        let (magic, version) = (r.u32()?, r.u32()?);
        if magic != PREFS_MAGIC || version != PREFS_VERSION {
            return Err(format!("battle preferences: magic {magic:#x} version {version}"));
        }
        let mut f = BattlePrefsFile { kind: r.str8()?, file: r.str8()?, name: r.str16()?, era: r.i32()?, time_of_day: r.i32()?, weather: r.str8()?, ..Default::default() };
        (f.wind, f.army_size, f.time_limit, f.ai_strength, f.total_players) = (r.i32()?, r.i32()?, r.i32()?, r.i32()?, r.i32()?);
        (f.game_name, f.password) = (r.str16()?, r.str16()?);
        (f.ranked, f.is_naval) = (r.u8()? != 0, r.u8()? != 0);
        (f.era_string, f.weather_string, f.time_limit_string) = (r.str16()?, r.str16()?, r.str16()?);
        f.allowed_funds = r.i32()?;
        f.description = r.str16()?;
        (f.image, f.map) = (r.str8()?, r.str8()?);
        f.spectators = r.i32()?;
        f.key = r.str8()?;
        f.is_historic = r.u8()? != 0;
        for t in &mut f.teams {
            t.players = r.i32()?;
            if t.players > 0 {
                let n = r.u32()? as usize;
                for _ in 0..n {
                    t.armies.push(ArmySetupFile::read_from(&mut r)?);
                }
            }
        }
        Ok(f)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The layout of the original's `.mp_default.battle_preferences` (its bytes rebuilt from the
    /// values it holds; the user's file is not copied into the repository).
    #[test]
    fn battle_prefs_layout_matches_the_original_sample() {
        let f = BattlePrefsFile {
            kind: "classic".into(),
            file: "BattleTerrain/Presets/NAP_MP_Valley_[SA]/".into(),
            name: "Aosta Valley".into(),
            era: 2,
            weather: "dry".into(),
            time_limit: 60,
            ai_strength: 1,
            total_players: 2,
            game_name: "t".into(),
            password: "t".into(),
            weather_string: "Dry".into(),
            time_limit_string: "60 Minutes".into(),
            allowed_funds: 5000,
            description: "Positioning and flanking is crucial here, as the high rock formations limit mobility.".into(),
            image: "data/BattleTerrain/Presets/NAP_MP_Valley_[SA]/screenshot_small.tga".into(),
            map: "data/BattleTerrain/Presets/NAP_MP_Valley_[SA]/preview_map.tga".into(),
            key: "NAP_MP_Valley_[SA]".into(),
            teams: [PrefsTeam { players: 4, armies: Vec::new() }, PrefsTeam { players: 4, armies: Vec::new() }],
            ..Default::default()
        };
        let b = f.to_bytes();
        // The sample is 509 bytes; its head and tail as dumped.
        assert_eq!(b.len(), 509);
        assert_eq!(&b[..16], &[0xba, 0, 0, 0, 8, 0, 0, 0, 7, 0, b'c', b'l', b'a', b's', b's', b'i']);
        assert_eq!(&b[0x6b..0x77], &[0x3c, 0, 0, 0, 1, 0, 0, 0, 2, 0, 0, 0]);
        assert_eq!(&b[0xa1..0xa5], &[0x88, 0x13, 0, 0]);
        assert_eq!(&b[0x1ec..], &[0, 4, 0, 0, 0, 0, 0, 0, 0, 4, 0, 0, 0, 0, 0, 0, 0]);
        assert_eq!(BattlePrefsFile::read(&b).unwrap(), f);
        // With armies.
        let mut g = f.clone();
        g.teams[0].armies.push(ArmySetupFile { era: 2, army_size: 5000, faction: "france".into(), cards: vec![Card { key: "Gen_Generals_Staff".into(), ..Default::default() }], ..Default::default() });
        assert_eq!(BattlePrefsFile::read(&g.to_bytes()).unwrap(), g);
        // An original file read from the user's folder, if there is one (read only).
        let p = std::env::var_os("APPDATA").map(|a| std::path::PathBuf::from(a).join(r"The Creative Assembly\Napoleon\battle_preferences\.mp_default.battle_preferences"));
        if let Some(orig) = p.as_ref().and_then(|p| std::fs::read(p).ok()) {
            let o = BattlePrefsFile::read(&orig).unwrap();
            assert_eq!(o.to_bytes(), orig, "byte-exact round trip of the original's file");
        }
    }

    #[test]
    fn army_setup_round_trip_and_layout() {
        let f = ArmySetupFile {
            era: 2,
            army_size: 10000,
            faction: "austria".into(),
            is_human: true,
            name: "Généraux".into(),
            cards: vec![
                Card { key: "Gen_Generals_Staff".into(), experience: 1, commander: true, ship_name: String::new() },
                Card { key: "Inf_Line_Austrian_German_Fusiliers".into(), experience: 0, commander: false, ship_name: String::new() },
            ],
            limits: vec![Limit { max: 7, actual: 1, tag: "inf".into() }],
        };
        let b = f.to_bytes();
        assert_eq!(&b[..12], &[3, 0, 0, 0, 2, 0, 0, 0, 0x10, 0x27, 0, 0]);
        assert_eq!(&b[12..21], b"\x07\x00austria");
        assert_eq!(b[21], 1);
        assert_eq!(&b[22..26], &[8, 0, b'G', 0]);
        assert_eq!(ArmySetupFile::read(&b).unwrap(), f);
        assert!(ArmySetupFile::read(&b[..b.len() - 1]).is_err());
        let mut v4 = b.clone();
        v4[0] = 4;
        assert!(ArmySetupFile::read(&v4).is_err());
    }
}
