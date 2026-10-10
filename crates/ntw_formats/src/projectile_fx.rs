//! The two shipped DB tables that name the effect groups a projectile plays:
//! `db\projectiles_explosions_tables\projectiles_explosions` and
//! `db\projectile_impacts_tables\projectile_impacts`.
//!
//! Together with [`crate::effects`] (the group library) and `projectiles` (which points at these
//! tables) this is the whole data chain behind an explosion, an impact and a scorch, and it is all
//! shipped data. See `analysis/graphics/BATTLE_EFFECTS.md` §2.
//!
//! # What is CONFIRMED and what is not
//!
//! **CONFIRMED** (install tests `ntw_data --test effects_data` and
//! `ntw_formats --test effects_install`, which read the shipped bytes):
//! - the *string* columns of every row, in file order, and that the two group columns hold
//!   `SCRIPTED_EFFECT_GROUP` names of `effects\landbattle.xml`;
//! - that the third string column is a **fragment projectile** (a `projectiles` foreign key), not a
//!   group: a clean negative over all of its distinct values (the install test requires at least six
//!   and checks each);
//! - the two foreign keys that reach these tables: `projectiles.explosion` (column 5) names a
//!   `projectiles_explosions` key, and `projectiles`'s column 33 (`impact_ball`) names a
//!   `projectile_impacts` key;
//! - every 4-byte number that sits between the strings, kept in [`ExplosionRow::numbers`].
//!
//! **UNKNOWN** (and therefore not decoded here): the *names* of the numeric columns, and which
//! surface each of the impact group's columns is for. Three rounds of exact-fit schema search did
//! not land a full column layout, so the numeric columns are read as raw values and the impact
//! columns as an ordered list. Nothing in this module guesses at either.
//!
//! # How the rows are found without a schema
//!
//! A DB table has no per-row framing, so a row's extent can only be found from its content. Both
//! readers here do it structurally, from the shipped bytes, and then check the row count against
//! the header — which is what makes the segmentation evidence rather than a guess.
//!
//! **`projectiles_explosions`** (CONFIRMED, all 35 rows): the key, the fuse class and the shockwave
//! class are **adjacent** (0 bytes between them), then a **17-byte** numeric block, then the
//! fragment projectile. The run 0 / 0 / 17 is unique to a row, so rows are cut there; a row holds
//! five strings (no ground scorch) or six (with one).
//!
//! **`projectile_impacts`** (CONFIRMED, all 8 rows): every row ends with a **size class** — one of
//! `small` / `medium` / `large`, the same three words `projectiles.calibre` uses — so rows are cut
//! after each one.
use std::collections::BTreeMap;

use crate::db::{DbError, DbHeader};

/// Everything that can go wrong reading these two tables.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectileFxError {
    /// The table's header would not read.
    Header(DbError),
    /// A row did not have the string columns the shipped rows have.
    Shape {
        /// The row key.
        key: String,
        /// How many strings the row's byte range holds.
        found: usize,
        /// How many it must hold.
        want: &'static str,
    },
    /// The segmentation found a different number of rows than the header says.
    RowCount {
        /// From the header.
        expected: u32,
        /// From the content.
        found: usize,
    },
}

impl std::fmt::Display for ProjectileFxError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Header(e) => write!(f, "bad table header: {e}"),
            Self::Shape { key, found, want } => write!(f, "row {key} has {found} strings, expected {want}"),
            Self::RowCount { expected, found } => {
                write!(f, "header says {expected} rows, the content gives {found}")
            }
        }
    }
}

impl std::error::Error for ProjectileFxError {}

/// One length-prefixed UTF-16 string found in the bytes, with where it starts.
struct Found {
    at: usize,
    end: usize,
    text: String,
}

/// Every length-prefixed, printable, non-empty UTF-16 string in `bytes`, in file order and
/// **non-overlapping**: the scan resumes after a string it found, so no string can start inside
/// another and every `at` is at or past the previous `end` (the gap arithmetic below relies on it;
/// an overlap used to underflow `s.at - prev_end` and panic on a malformed table).
fn find_strings(bytes: &[u8], from: usize, to: usize) -> Vec<Found> {
    let mut out = Vec::new();
    let mut i = from;
    while i + 2 <= to {
        let n = u16::from_le_bytes([bytes[i], bytes[i + 1]]) as usize;
        if (1..=96).contains(&n) && i + 2 + n * 2 <= to {
            let units: Vec<u16> = bytes[i + 2..i + 2 + n * 2]
                .as_chunks::<2>()
                .0
                .iter()
                .map(|c| u16::from_le_bytes(*c))
                .collect();
            if let Ok(s) = String::from_utf16(&units)
                && s.chars().all(|c| c.is_ascii_graphic() || c == ' ')
                && !s.trim().is_empty()
            {
                let end = i + 2 + n * 2;
                out.push(Found { at: i, end, text: s });
                i = end;
                continue;
            }
        }
        i += 1;
    }
    out
}

/// The 4-byte values in `bytes[from..to]`, read little-endian as `f32` from every offset in turn.
/// Which of them are real columns is UNKNOWN; they are kept as the evidence that they are there.
fn numbers_between(bytes: &[u8], from: usize, to: usize) -> Vec<f32> {
    let mut out = Vec::new();
    let gap = &bytes[from..to];
    let mut o = 0;
    while o + 4 <= gap.len() {
        out.push(f32::from_le_bytes([gap[o], gap[o + 1], gap[o + 2], gap[o + 3]]));
        o += 4;
    }
    out
}

/// One row of `projectiles_explosions`: what a projectile's detonation plays.
///
/// The string columns after the key are, in file order (CONFIRMED): the **fuse** class, the
/// **shockwave** class, the **fragment projectile** (a `projectiles` key, not a group), the **air
/// burst** group and — on the 10 rows that have one — the **ground scorch** group.
#[derive(Debug, Clone, PartialEq)]
pub struct ExplosionRow {
    /// The row key, e.g. `shell_12lb`. This is what `projectiles.explosion` points at.
    pub key: String,
    /// The fuse class (`fuse`, `fuse_grenade`, `fuse_shell`). A gameplay column, not played here.
    pub fuse: String,
    /// The shockwave class (`shockwave`, `carcass`, `quicklime`, `percussive`).
    pub shockwave: String,
    /// The **fragment projectile** this row's fragments are (`shell_fragment`,
    /// `carcass_fragment`, `shrapnel`, `quicklime_fragment`, `grenade_fragment`,
    /// `ship_explosion_fragment`, `ship_explosion_fragment_visual`). CONFIRMED a foreign key to
    /// `projectiles`, not an effect group: all eight are `projectiles` row keys, and none is a
    /// `SCRIPTED_EFFECT_GROUP` of `effects\landbattle.xml`.
    pub fragment_projectile: Option<String>,
    /// The group the air burst plays: `AirExplosion_sml/med/lrg`, `explode_cone`,
    /// `explode_grenade`, `explode_quicklime_med` or `ShipExplosion`.
    pub air: Option<String>,
    /// The group the scorch on the ground plays (`Cannon_Groundimpact_explosive` on the shipped
    /// explosive-shell rows; absent on the carcass, grenade and quicklime rows).
    pub ground: Option<String>,
    /// The unnamed 4-byte values between the string columns, in file order. **UNKNOWN** names;
    /// kept because they are the row's numeric columns (one of them grows with the pound count in
    /// the key over the shrapnel and shell series, so it is INFERRED to be a burst radius in
    /// metres, but it does **not** pick the `_sml`/`_med`/`_lrg` group).
    pub numbers: Vec<f32>,
}

/// Every row of `projectiles_explosions`, by key.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ExplosionTable {
    rows: BTreeMap<String, ExplosionRow>,
}

/// The byte gaps that precede an explosion row's fuse class, shockwave class and fragment group
/// (CONFIRMED on all 35 shipped rows): the fuse and shockwave class are **adjacent** to the key
/// (0 bytes), then a **17-byte** numeric block before the fragment group. The run 0 / 0 / 17 is
/// unique to a row, which is how rows are cut without a schema. The gap before the *key* itself
/// varies — 9 bytes when the previous row had no ground scorch, 0 when it had — so it is not used.
const ROW_INNER_GAPS: [usize; 3] = [0, 0, 17];

impl ExplosionTable {
    /// The rows in key order, to merge several files of the table by key
    /// (`ntw_data`'s DB merge, the original's rule for every table).
    pub fn into_rows(self) -> Vec<ExplosionRow> {
        self.rows.into_values().collect()
    }

    /// A table from rows (a later row with the same key replaces an earlier one).
    pub fn from_rows(rows: impl IntoIterator<Item = ExplosionRow>) -> Self {
        Self { rows: rows.into_iter().map(|r| (r.key.clone(), r)).collect() }
    }

    /// Reads the table, cutting it into rows structurally at each 0 / 0 / 0 / 17 run.
    pub fn read(bytes: &[u8]) -> Result<Self, ProjectileFxError> {
        let header = DbHeader::read(bytes).map_err(ProjectileFxError::Header)?;
        let all = find_strings(bytes, header.data_offset, bytes.len());
        // The gap before each string, measured from where the previous one ended.
        let mut prev_end = header.data_offset;
        let gaps: Vec<usize> = all
            .iter()
            .map(|s| {
                let g = s.at - prev_end;
                prev_end = s.end;
                g
            })
            .collect();

        // A row's key is the string just before its 0 / 0 / 17 run.
        let starts: Vec<usize> = (1..all.len())
            .filter(|i| ROW_INNER_GAPS.iter().enumerate().all(|(k, want)| gaps.get(i + k) == Some(want)))
            .map(|i| i - 1)
            .collect();
        if starts.len() != header.row_count as usize {
            return Err(ProjectileFxError::RowCount { expected: header.row_count, found: starts.len() });
        }
        let mut rows = BTreeMap::new();
        for (n, &i) in starts.iter().enumerate() {
            let end = starts.get(n + 1).copied().unwrap_or(all.len());
            let row = &all[i..end];
            let key = row[0].text.clone();
            // key, fuse, shockwave, fragment, air, then the ground scorch on the rows that have
            // one. A row is five or six strings; anything else means the cut is wrong.
            let (air, ground) = match row.len() {
                5 => (Some(row[4].text.clone()), None),
                6 => (Some(row[4].text.clone()), Some(row[5].text.clone())),
                n => return Err(ProjectileFxError::Shape { key, found: n, want: "5 or 6" }),
            };
            // The unnamed numeric columns, kept in file order: the block after the shockwave
            // class, the one after the fragment group, and — on the rows with a ground scorch —
            // the one after the air burst.
            let mut numbers = numbers_between(bytes, row[2].end, row[3].at);
            numbers.extend(numbers_between(bytes, row[3].end, row[4].at));
            if row.len() > 5 {
                numbers.extend(numbers_between(bytes, row[4].end, row[5].at));
            }
            rows.insert(
                key.clone(),
                ExplosionRow {
                    key,
                    fuse: row[1].text.clone(),
                    shockwave: row[2].text.clone(),
                    fragment_projectile: Some(row[3].text.clone()),
                    air,
                    ground,
                    numbers,
                },
            );
        }
        Ok(Self { rows })
    }

    /// The row for `key`.
    pub fn get(&self, key: &str) -> Option<&ExplosionRow> {
        self.rows.get(key)
    }

    /// Every row, by key.
    pub fn iter(&self) -> impl Iterator<Item = &ExplosionRow> {
        self.rows.values()
    }

    /// How many rows were read.
    pub fn len(&self) -> usize {
        self.rows.len()
    }

    /// True when the table has no rows.
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }
}

/// One row of `projectile_impacts`: the groups a ball class plays, per surface.
///
/// The row is a key, then a fixed number of effect-group columns (**UNKNOWN** which surface each
/// one is for, and how many of them there are: the shipped rows hold between 15 and 18, because
/// some are absent), then a **size class** — one of `small` / `medium` / `large`, the same three
/// words `projectiles.calibre` uses.
#[derive(Debug, Clone, PartialEq)]
pub struct ImpactRow {
    /// The row key: a ball class (`carcass`, `default_ball`, `musket_ball`, `naval_grape`,
    /// `incendiary`, `large_ball`, `quicklime`, `test`). This is what `projectiles`' column 33
    /// points at.
    pub key: String,
    /// The group columns in file order. **UNKNOWN** surface order. An empty column is not a string
    /// the reader can see, so it is skipped rather than kept as `None`: every entry is `Some` and
    /// the index of an entry is **not** its column number.
    pub groups: Vec<Option<String>>,
    /// `small` / `medium` / `large` (CONFIRMED: every shipped row ends with one of the three).
    pub size_class: Option<String>,
}

impl ImpactRow {
    /// True when any group column of the row is `name`.
    pub fn has_group(&self, name: &str) -> bool {
        self.groups.iter().any(|g| g.as_deref() == Some(name))
    }
}

/// Every row of `projectile_impacts`, by key.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ImpactTable {
    rows: BTreeMap<String, ImpactRow>,
}

/// The three size-class words every shipped `projectile_impacts` row ends with (CONFIRMED).
const SIZE_CLASSES: [&str; 3] = ["small", "medium", "large"];

impl ImpactTable {
    /// The rows in key order, to merge several files of the table by key
    /// (`ntw_data`'s DB merge, the original's rule for every table).
    pub fn into_rows(self) -> Vec<ImpactRow> {
        self.rows.into_values().collect()
    }

    /// A table from rows (a later row with the same key replaces an earlier one).
    pub fn from_rows(rows: impl IntoIterator<Item = ImpactRow>) -> Self {
        Self { rows: rows.into_iter().map(|r| (r.key.clone(), r)).collect() }
    }

    /// Reads the table, cutting it into rows after every size-class word.
    pub fn read(bytes: &[u8]) -> Result<Self, ProjectileFxError> {
        let header = DbHeader::read(bytes).map_err(ProjectileFxError::Header)?;
        let all = find_strings(bytes, header.data_offset, bytes.len());
        let mut rows: BTreeMap<String, ImpactRow> = BTreeMap::new();
        let mut start = header.data_offset;
        for s in &all {
            if !SIZE_CLASSES.contains(&s.text.as_str()) {
                continue;
            }
            let fields: Vec<&Found> =
                all.iter().filter(|f| f.at >= start && f.at < s.at).collect();
            let Some(key) = fields.first() else {
                start = s.end;
                continue;
            };
            rows.insert(
                key.text.clone(),
                ImpactRow {
                    key: key.text.clone(),
                    groups: fields[1..].iter().map(|f| Some(f.text.clone())).collect(),
                    size_class: Some(s.text.clone()),
                },
            );
            start = s.end;
        }
        if rows.len() != header.row_count as usize {
            return Err(ProjectileFxError::RowCount { expected: header.row_count, found: rows.len() });
        }
        Ok(Self { rows })
    }

    /// The row for a ball class.
    pub fn get(&self, key: &str) -> Option<&ImpactRow> {
        self.rows.get(key)
    }

    /// Every row, by key.
    pub fn iter(&self) -> impl Iterator<Item = &ImpactRow> {
        self.rows.values()
    }

    /// How many rows were read.
    pub fn len(&self) -> usize {
        self.rows.len()
    }

    /// True when the table has no rows.
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }
}

/// The bytes one `projectile_trails` row's ten floats take. The table is exactly regular: key,
/// blend mode, ten floats, next key, so this is the row length with both strings out.
pub const TRAIL_FLOAT_BYTES: usize = 40;

/// One row of `projectile_trails`: the colour and the geometry of a projectile's trail.
///
/// This is a **different** thing from `projectiles`' column 32 (`trail`), which names the trail's
/// **effect group** in `effects\landbattle.xml` (`shrapnel_trail`, `carcass_trail`,
/// `congreve_rocket`, ...). The bridge between the two is `projectiles`' **column 6**
/// (`trail_texture`), and this table is what that column points at. CONFIRMED on the install by
/// `ntw_data/tests/effects_data.rs::the_trail_group_and_the_trail_table_are_joined_by_column_six`
/// and `ntw_formats/tests/effects_install.rs::the_projectile_trails_table_reads_whole`.
///
/// # What is named and what is not
///
/// - The **second string is the blend mode** (CONFIRMED): `alpha`, `add`, `none` — three words
///   that name nothing else in the table, and `alpha_shrapnel` is the only row that is not `alpha`.
/// - **Floats 4-7 are an 8-bit RGBA quadruple** (CONFIRMED, see [`TrailRow::colour`]). Three
///   consecutive equal values (255/255/255, 125/125/125) followed by a value in the same range
///   (128, 100), and all four zero on the `none` row. The order within the quadruple is INFERRED:
///   white at alpha 128 for a musket-ball or shell trail and grey at 100 for a rocket is the
///   sensible reading, whereas alpha-first would make every trail fully opaque and yellow-tinted.
/// - **The other six floats are UNKNOWN.** [`TrailRow::numbers`] keeps them in file order. No
///   naming is claimed for them: one `projectile_trails` row serves projectiles whose muzzle
///   velocity spans 4 to 250 m/s and whose effective range spans 50 to 750 m, so none of them can
///   be a per-shot duration or length, and the last one is the constant 50 on all four live rows.
#[derive(Debug, Clone, PartialEq)]
pub struct TrailRow {
    /// The row key: a trail texture name (`alpha`, `alpha_bullet`, `alpha_shrapnel`,
    /// `e3_rocket`, `none`). This is what `projectiles`' column 6 (`trail_texture`) points at.
    pub key: String,
    /// The blend mode: `alpha`, `add` or `none`.
    pub blend: String,
    /// The ten floats, in file order.
    pub floats: [f32; 10],
}

impl TrailRow {
    /// The trail colour as R, G, B and A in 0..=255 — floats 4 to 7.
    ///
    /// CONFIRMED as a quadruple by the shipped bytes: they are the only four values in the table
    /// that share an 8-bit range, the first three of them are equal on every live row, and all
    /// four are 0 on the `none` row.
    pub fn colour(&self) -> [u8; 4] {
        [
            self.floats[3] as u8,
            self.floats[4] as u8,
            self.floats[5] as u8,
            self.floats[6] as u8,
        ]
    }

    /// The six floats whose names are UNKNOWN: 1, 2, 3, 8, 9 and 10, in file order.
    pub fn numbers(&self) -> [f32; 6] {
        [
            self.floats[0],
            self.floats[1],
            self.floats[2],
            self.floats[7],
            self.floats[8],
            self.floats[9],
        ]
    }

    /// False on the `none` row, whose ten floats are all zero: this trail draws nothing.
    pub fn is_visible(&self) -> bool {
        self.floats.iter().any(|v| *v != 0.0)
    }
}

/// Every row of `projectile_trails`, by key.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TrailTable {
    rows: BTreeMap<String, TrailRow>,
}

impl TrailTable {
    /// The rows in key order, to merge several files of the table by key
    /// (`ntw_data`'s DB merge, the original's rule for every table).
    pub fn into_rows(self) -> Vec<TrailRow> {
        self.rows.into_values().collect()
    }

    /// A table from rows (a later row with the same key replaces an earlier one).
    pub fn from_rows(rows: impl IntoIterator<Item = TrailRow>) -> Self {
        Self { rows: rows.into_iter().map(|r| (r.key.clone(), r)).collect() }
    }

    /// Reads the table. Its schema is `ssffffffffff` and the file is exactly regular — key, blend
    /// mode, ten floats, next key — so a row is cut at every pair of adjacent strings followed by
    /// exactly [`TRAIL_FLOAT_BYTES`], and the row count is checked against the header.
    pub fn read(bytes: &[u8]) -> Result<Self, ProjectileFxError> {
        let header = DbHeader::read(bytes).map_err(ProjectileFxError::Header)?;
        let all = find_strings(bytes, header.data_offset, bytes.len());
        let mut rows = BTreeMap::new();
        let mut i = 0usize;
        while i + 1 < all.len() {
            // The blend mode is adjacent to the key, and exactly ten floats follow it.
            if all[i + 1].at != all[i].end {
                i += 1;
                continue;
            }
            let end = all.get(i + 2).map(|s| s.at).unwrap_or(bytes.len());
            if end.checked_sub(all[i + 1].end) != Some(TRAIL_FLOAT_BYTES) {
                i += 1;
                continue;
            }
            let floats: [f32; 10] = std::array::from_fn(|k| {
                let o = all[i + 1].end + k * 4;
                f32::from_le_bytes([bytes[o], bytes[o + 1], bytes[o + 2], bytes[o + 3]])
            });
            let key = all[i].text.clone();
            rows.insert(
                key.clone(),
                TrailRow { key, blend: all[i + 1].text.clone(), floats },
            );
            i += 2;
        }
        if rows.len() != header.row_count as usize {
            return Err(ProjectileFxError::RowCount { expected: header.row_count, found: rows.len() });
        }
        Ok(Self { rows })
    }

    /// The row for a trail texture name.
    pub fn get(&self, key: &str) -> Option<&TrailRow> {
        self.rows.get(key)
    }

    /// Every row, by key.
    pub fn iter(&self) -> impl Iterator<Item = &TrailRow> {
        self.rows.values()
    }

    /// How many rows were read.
    pub fn len(&self) -> usize {
        self.rows.len()
    }

    /// True when the table has no rows.
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The header of a version-1 table with `rows` rows.
    fn header(rows: usize) -> Vec<u8> {
        let mut b = crate::db::VERSION_MARKER.to_vec();
        b.extend_from_slice(&1u32.to_le_bytes());
        b.push(1);
        b.extend_from_slice(&(rows as u32).to_le_bytes());
        b
    }

    fn s(b: &mut Vec<u8>, t: &str) {
        b.extend_from_slice(&(t.encode_utf16().count() as u16).to_le_bytes());
        t.encode_utf16().for_each(|u| b.extend_from_slice(&u.to_le_bytes()));
    }

    /// One row in the shipped byte shape: key, fuse, shockwave adjacent, a 17-byte numeric block,
    /// the fragment group, a 5-byte block, the air burst, a 9-byte block and then the ground
    /// scorch (or straight on to the next key).
    fn explosion_row(b: &mut Vec<u8>, key: &str, air: &str, ground: Option<&str>) {
        s(b, key);
        s(b, "fuse_shell");
        s(b, "shockwave");
        // Four aligned floats then one byte: the 17-byte block the shipped rows have before the
        // fragment group.
        for v in [3.0f32, 0.0, 20.0, 10.0] {
            b.extend_from_slice(&v.to_le_bytes());
        }
        b.push(0x11);
        s(b, "shell_fragment");
        b.extend_from_slice(&0.0f32.to_le_bytes());
        b.push(0x44);
        s(b, air);
        for v in [0.0f32, 1.0] {
            b.extend_from_slice(&v.to_le_bytes());
        }
        b.push(0x55);
        if let Some(g) = ground {
            s(b, g);
        }
    }

    #[test]
    fn explosion_rows_are_cut_at_the_zero_zero_seventeen_run() {
        let mut bytes = header(2);
        explosion_row(&mut bytes, "shell_12lb", "AirExplosion_med", Some("Cannon_Groundimpact_explosive"));
        explosion_row(&mut bytes, "carcass_12lb", "explode_cone", None);
        let t = ExplosionTable::read(&bytes).expect("read");
        assert_eq!(t.len(), 2);
        let shell = t.get("shell_12lb").expect("shell_12lb");
        assert_eq!((shell.fuse.as_str(), shell.shockwave.as_str()), ("fuse_shell", "shockwave"));
        assert_eq!(shell.fragment_projectile.as_deref(), Some("shell_fragment"));
        assert_eq!(shell.air.as_deref(), Some("AirExplosion_med"));
        assert_eq!(shell.ground.as_deref(), Some("Cannon_Groundimpact_explosive"));
        // The 17-byte block's numbers are kept, in order.
        assert_eq!(&shell.numbers[..4], &[3.0, 0.0, 20.0, 10.0]);
        // The carcass row has no ground group: five strings, and its own air burst.
        let carcass = t.get("carcass_12lb").expect("carcass_12lb");
        assert_eq!(carcass.ground, None);
        assert_eq!(carcass.air.as_deref(), Some("explode_cone"));
    }

    /// The row count is checked against the header, and a row's string count against the shipped
    /// shape, so a wrong cut is an error rather than a misread row.
    #[test]
    fn a_wrong_cut_is_an_error_not_a_guess() {
        // A table with two rows but a header that says one.
        let mut bytes = header(1);
        explosion_row(&mut bytes, "shell_12lb", "AirExplosion_med", None);
        explosion_row(&mut bytes, "carcass_12lb", "explode_cone", None);
        let wrong_count = ExplosionTable::read(&bytes);
        assert!(
            matches!(wrong_count, Err(ProjectileFxError::RowCount { .. }) | Err(ProjectileFxError::Shape { .. })),
            "a header that disagrees with the rows is an error: {wrong_count:?}"
        );

        // A row with three strings instead of five or six.
        let mut bytes = header(1);
        s(&mut bytes, "shell_12lb");
        s(&mut bytes, "fuse_shell");
        s(&mut bytes, "shockwave");
        for v in [3.0f32, 0.0, 20.0, 10.0] {
            bytes.extend_from_slice(&v.to_le_bytes());
        }
        bytes.push(0x11);
        s(&mut bytes, "shell_fragment");
        assert!(matches!(ExplosionTable::read(&bytes), Err(ProjectileFxError::Shape { found: 4, .. })));

        // Nothing that looks like a row at all.
        let bytes = header(1);
        assert!(matches!(ExplosionTable::read(&bytes), Err(ProjectileFxError::RowCount { expected: 1, found: 0 })));
    }

    /// Two impact rows in the shipped shape: key, two group columns, size class.
    fn impacts(rows: &[(&str, &[&str], &str)]) -> Vec<u8> {
        let mut b = vec![1u8];
        b.extend_from_slice(&(rows.len() as u32).to_le_bytes());
        let s = |b: &mut Vec<u8>, t: &str| {
            b.extend_from_slice(&(t.encode_utf16().count() as u16).to_le_bytes());
            t.encode_utf16().for_each(|u| b.extend_from_slice(&u.to_le_bytes()));
        };
        for (key, groups, size) in rows {
            s(&mut b, key);
            for g in *groups {
                s(&mut b, g);
            }
            s(&mut b, size);
        }
        b
    }

    /// A malformed or modded table is an error, never a panic. The scan used to report strings that
    /// start inside other strings, which could make `s.at - prev_end` underflow. Deterministic noise
    /// with real string fragments spliced in, through all three readers.
    #[test]
    fn noise_is_an_error_not_a_panic() {
        let mut seed = 0x1234_5678u32;
        let mut next = || {
            seed = seed.wrapping_mul(214_013).wrapping_add(2_531_011);
            (seed >> 16) as u8
        };
        for round in 0..400 {
            let mut bytes = header(3);
            for _ in 0..(round % 97) {
                match next() % 4 {
                    0 => s(&mut bytes, "shell_12lb"),
                    1 => s(&mut bytes, "small"),
                    _ => bytes.push(next()),
                }
                // A length prefix that claims a string running into the next one.
                if next() % 13 == 0 {
                    bytes.extend_from_slice(&[3, 0, b'a', 0]);
                }
            }
            let _ = ExplosionTable::read(&bytes);
            let _ = ImpactTable::read(&bytes);
            let _ = TrailTable::read(&bytes);
        }
        // And the strings the scan reports never overlap.
        // `!` is 33, so at the `!` the bytes also read as a 33-unit string of the `a`s after it: the
        // old byte-by-byte scan reported that as a second string starting inside the first.
        let long = format!("x!{}", "a".repeat(38));
        let mut bytes = Vec::new();
        s(&mut bytes, &long);
        s(&mut bytes, "medium");
        let found = find_strings(&bytes, 0, bytes.len());
        assert!(found.windows(2).all(|w| w[1].at >= w[0].end), "overlapping strings");
        assert_eq!(found.iter().map(|f| f.text.as_str()).collect::<Vec<_>>(), vec![long.as_str(), "medium"]);
    }

    #[test]
    fn impact_rows_end_at_the_size_class() {
        let bytes = impacts(&[
            ("musket_ball", &["Musket_impact_hard", "blood_gen"], "small"),
            ("default_ball", &["Cannon_Groundimpact_gen_med"], "medium"),
        ]);
        let t = ImpactTable::read(&bytes).unwrap();
        assert_eq!(t.len(), 2);
        let m = t.get("musket_ball").unwrap();
        assert_eq!(m.size_class.as_deref(), Some("small"));
        assert_eq!(m.groups, vec![Some("Musket_impact_hard".into()), Some("blood_gen".into())]);
        assert!(m.has_group("blood_gen") && !m.has_group("blood_spray"));
        assert_eq!(t.get("default_ball").unwrap().size_class.as_deref(), Some("medium"));
    }

    /// A `projectile_trails` table in the shipped shape: key, blend mode, ten floats, and the next
    /// row. The values are the shipped ones, so the colour and the visible flag are read off them.
    fn trails(rows: &[(&str, &str, [f32; 10])]) -> Vec<u8> {
        let mut b = vec![0u8];
        b.extend_from_slice(&(rows.len() as u32).to_le_bytes());
        let s = |b: &mut Vec<u8>, t: &str| {
            b.extend_from_slice(&(t.encode_utf16().count() as u16).to_le_bytes());
            t.encode_utf16().for_each(|u| b.extend_from_slice(&u.to_le_bytes()));
        };
        for (key, blend, floats) in rows {
            s(&mut b, key);
            s(&mut b, blend);
            for v in *floats {
                b.extend_from_slice(&v.to_le_bytes());
            }
        }
        b
    }

    #[test]
    fn trail_rows_are_cut_at_ten_floats_and_the_colour_reads() {
        let bytes = trails(&[
            ("alpha", "alpha", [0.2, 0.1, 50.0, 255.0, 255.0, 255.0, 128.0, 300.0, 30.0, 50.0]),
            ("alpha_shrapnel", "add", [0.05, 0.05, 25.0, 255.0, 255.0, 255.0, 128.0, 300.0, 25.0, 50.0]),
            ("e3_rocket", "alpha", [2.0, 2.0, 500.0, 125.0, 125.0, 125.0, 100.0, 1000.0, 100.0, 50.0]),
            ("none", "none", [0.0; 10]),
        ]);
        let t = TrailTable::read(&bytes).unwrap();
        assert_eq!(t.len(), 4);
        // The blend mode is the second string, and floats 4-7 are an 8-bit RGBA quadruple.
        assert_eq!(t.get("alpha").unwrap().blend, "alpha");
        assert_eq!(t.get("alpha_shrapnel").unwrap().blend, "add");
        assert_eq!(t.get("alpha").unwrap().colour(), [255, 255, 255, 128]);
        assert_eq!(t.get("e3_rocket").unwrap().colour(), [125, 125, 125, 100]);
        // The all-zero `none` row draws nothing.
        assert!(t.get("alpha").unwrap().is_visible());
        assert!(!t.get("none").unwrap().is_visible());
        assert_eq!(t.get("none").unwrap().colour(), [0, 0, 0, 0]);
        // The six unnamed floats come back in file order, 1 2 3 8 9 10.
        assert_eq!(t.get("alpha").unwrap().numbers(), [0.2, 0.1, 50.0, 300.0, 30.0, 50.0]);
        // A row whose float block is the wrong size is not cut, so the count check catches it.
        let mut wrong = trails(&[("alpha", "alpha", [0.2, 0.1, 50.0, 255.0, 255.0, 255.0, 128.0, 300.0, 30.0, 50.0])]);
        wrong.truncate(wrong.len() - 4);
        assert!(matches!(TrailTable::read(&wrong), Err(ProjectileFxError::RowCount { .. })));
    }
}