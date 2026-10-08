//! The `models_building` DB table: each battlefield building's model and its fire lines.
//!
//! Row layout of the exe's reader `0x00DD2660` (CONFIRMED; it reads the whole shipped table, 538
//! rows): key, model path (`…_tech.cs2.parsed`), an int, then a count of entries read by
//! `0x00DF1930`: a name and ten 32-bit values. The entries are named
//! `EFLine_piece<NN>_destruct<NN>_line<NN>`; the values are an int (2 in the shipped rows), a start
//! point, an end point and an outward normal (x, height, z, building space; INFERRED from the
//! numbers: short wall-hugging segments with unit normals). INFERRED: the windows and loopholes the
//! garrison fires from (only buildings with them can be garrisoned, see BATTLE_FIDELITY.md §34).

use crate::bytes::Cursor;
use crate::db::{DbError, DbHeader};

/// One fire line of a building model.
#[derive(Debug, Clone, PartialEq)]
pub struct FireLine {
    /// Entry name, e.g. `EFLine_piece01_destruct01_line02`.
    pub name: String,
    /// The first value (2 in the shipped rows; meaning UNKNOWN).
    pub kind: u32,
    /// Start point (x, height, z), building space.
    pub start: [f32; 3],
    /// End point.
    pub end: [f32; 3],
    /// Outward normal.
    pub normal: [f32; 3],
}

impl FireLine {
    /// The `destruct<NN>` number in the name (1 = intact), if any.
    pub fn destruct_state(&self) -> Option<u32> {
        let lower = self.name.to_ascii_lowercase();
        let rest = &lower[lower.find("destruct")? + "destruct".len()..];
        rest.chars().take_while(char::is_ascii_digit).collect::<String>().parse().ok()
    }
}

/// One `models_building` row.
#[derive(Debug, Clone, PartialEq)]
pub struct ModelBuilding {
    /// Building key (as the battle maps place it).
    pub key: String,
    /// Model path.
    pub model: String,
    /// The int after the model (0 or 1; meaning UNKNOWN).
    pub flag: u32,
    /// Its fire lines.
    pub fire_lines: Vec<FireLine>,
}

impl ModelBuilding {
    /// The fire lines of the intact building (`destruct01`, or lines without a state).
    pub fn intact_fire_lines(&self) -> impl Iterator<Item = &FireLine> {
        self.fire_lines.iter().filter(|l| l.destruct_state().is_none_or(|s| s == 1))
    }
}

/// Reads the whole table.
pub fn read(bytes: &[u8]) -> Result<Vec<ModelBuilding>, DbError> {
    let header = DbHeader::read(bytes)?;
    let mut c = Cursor::new(bytes);
    c.take(header.data_offset)?;
    let mut rows = Vec::with_capacity(header.row_count as usize);
    for _ in 0..header.row_count {
        let key = c.utf16()?;
        let model = c.utf16()?;
        let flag = c.u32()?;
        let n = c.u32()?;
        let mut fire_lines = Vec::with_capacity(n.min(4096) as usize);
        for _ in 0..n {
            let name = c.utf16()?;
            let kind = c.u32()?;
            let mut v = [0f32; 9];
            for x in &mut v {
                *x = c.f32()?;
            }
            fire_lines.push(FireLine { name, kind, start: [v[0], v[1], v[2]], end: [v[3], v[4], v[5]], normal: [v[6], v[7], v[8]] });
        }
        rows.push(ModelBuilding { key, model, flag, fire_lines });
    }
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bytes::utf16_bytes;

    #[test]
    fn reads_rows_and_fire_lines() {
        let mut b = vec![1u8];
        b.extend(1u32.to_le_bytes());
        b.extend(utf16_bytes("farm"));
        b.extend(utf16_bytes("buildings\\farm\\farm_tech.cs2.parsed"));
        b.extend(1u32.to_le_bytes());
        b.extend(2u32.to_le_bytes());
        for (name, x) in [("EFLine_piece01_destruct01_line01", 1.0f32), ("EFLine_piece01_destruct02_line01", 2.0)] {
            b.extend(utf16_bytes(name));
            b.extend(2u32.to_le_bytes());
            for v in [x, 0.0, 0.0, x + 1.0, 0.0, 0.0, 0.0, 0.0, 1.0f32] {
                b.extend(v.to_le_bytes());
            }
        }
        let rows = read(&b).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].fire_lines.len(), 2);
        assert_eq!(rows[0].fire_lines[1].end, [3.0, 0.0, 0.0]);
        assert_eq!(rows[0].intact_fire_lines().count(), 1);
        assert_eq!(rows[0].fire_lines[1].destruct_state(), Some(2));
    }
}
