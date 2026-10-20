//! The open campaign map format's text files (DESIGN.md §3.5.1): a campaign folder of our own
//! (`campaigns/<name>/`) that a modder writes by hand, next to the original's `regions.esf`
//! importer. Each reader here fills the same [`RegionMap`] field the importer fills, so nothing
//! downstream depends on which source ran.
//!
//! Today this holds the black shroud outlines file, [`BLACK_SHROUD_FILE`] (optional):
//!
//! ```text
//! # Comments run from '#' to the end of the line; blank lines are skipped.
//! # One outline per line: its name, then 3 or more points, each "x z" in map units
//! # (the units of the region outlines). A name may repeat: every outline of that name is lifted.
//! shroud_map_1_1  -10.5 20   0 20   0 35.25   -10.5 35.25
//! ```
//!
//! The outlines keep file order. `unveil_black_shroud(name)` in a campaign script lifts the outlines
//! of that name ([`RegionMap::black_shroud_outlines`]); a map with any outline starts a new
//! campaign's human under the black shroud.

use crate::campaign_map::{GameFiles, RegionMap};

/// The black shroud outlines file's name in an open-format campaign folder.
pub const BLACK_SHROUD_FILE: &str = "black_shroud.txt";

/// The black shroud outlines, (name, outline in map units), in file order.
pub type BlackShroudOutlines = Vec<(String, Vec<(f32, f32)>)>;

/// Parses a [`BLACK_SHROUD_FILE`]'s text. An error names the first bad line (1-based): a point
/// that is not two finite numbers, or an outline of fewer than 3 points (it would lift nothing).
pub fn parse_black_shroud(text: &str) -> Result<BlackShroudOutlines, String> {
    let mut outlines = Vec::new();
    for (i, line) in text.lines().enumerate() {
        let line = line.split('#').next().unwrap_or_default();
        let mut words = line.split_whitespace();
        let Some(name) = words.next() else { continue };
        let numbers: Vec<f32> = words
            .map(|w| w.parse::<f32>().ok().filter(|v| v.is_finite()).ok_or_else(|| format!("line {}: {w:?} is not a finite number", i + 1)))
            .collect::<Result<_, _>>()?;
        let (pairs, rest) = numbers.as_chunks::<2>();
        if !rest.is_empty() {
            return Err(format!("line {}: outline {name:?} has an x without its z", i + 1));
        }
        let points: Vec<(f32, f32)> = pairs.iter().map(|&[x, z]| (x, z)).collect();
        if points.len() < 3 {
            return Err(format!("line {}: outline {name:?} has {} points, needs 3 or more", i + 1, points.len()));
        }
        outlines.push((name.to_owned(), points));
    }
    Ok(outlines)
}

/// Reads the [`BLACK_SHROUD_FILE`] of the open-format campaign folder `folder` (e.g.
/// `campaigns/my_map`) through the game's files (so a mod's copy wins, as for every game file). Not
/// there: no outlines, not an error. There but unreadable or malformed: the error is logged once and
/// the file is skipped (no outlines).
pub fn read_black_shroud(files: &GameFiles<'_>, folder: &str) -> BlackShroudOutlines {
    let path = format!("{}/{BLACK_SHROUD_FILE}", folder.trim_end_matches(['/', '\\']));
    if !files.vfs.contains(&path) {
        return Vec::new();
    }
    let parsed = files.read(&path).and_then(|bytes| String::from_utf8(bytes).map_err(|_| "not UTF-8 text".to_owned())).and_then(|text| parse_black_shroud(&text));
    parsed.unwrap_or_else(|e| {
        log::error!("Open campaign map {path}: {e}; no black shroud outlines");
        Vec::new()
    })
}

/// Fills `regions`' black shroud outlines from the open-format campaign folder `folder`
/// ([`read_black_shroud`]), replacing any it had: the open-format source's part of
/// [`RegionMap::black_shroud_outlines`], which the `regions.esf` importer fills for the original's maps.
pub fn attach_black_shroud(regions: &mut RegionMap, files: &GameFiles<'_>, folder: &str) {
    regions.black_shroud_outlines = read_black_shroud(files, folder);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pack::test_util::temp_dir;
    use crate::pack::{LayerKind, Vfs};

    /// A file with comments, blank lines, a repeated name and mixed spacing parses in file order.
    #[test]
    fn parses_outlines_in_file_order() {
        let text = "# outlines\n\nshroud_a -10.5 20  0 20 0 35.25 # trailing\n  shroud_b\t1 1 2 1 2 2 1 2\nshroud_a 5 5 6 5 6 6\n";
        let got = parse_black_shroud(text).expect("parses");
        assert_eq!(
            got,
            [
                ("shroud_a".to_owned(), vec![(-10.5, 20.0), (0.0, 20.0), (0.0, 35.25)]),
                ("shroud_b".to_owned(), vec![(1.0, 1.0), (2.0, 1.0), (2.0, 2.0), (1.0, 2.0)]),
                ("shroud_a".to_owned(), vec![(5.0, 5.0), (6.0, 5.0), (6.0, 6.0)]),
            ]
        );
        assert_eq!(parse_black_shroud("# nothing\n\n").expect("empty"), []);
    }

    /// Each malformed line is refused with its line number: an odd coordinate count, a word that is
    /// not a number, a non-finite number, fewer than 3 points.
    #[test]
    fn refuses_malformed_lines() {
        for (text, line) in [
            ("ok 0 0 1 0 1 1\nbad 0 0 1 0 1\n", "line 2"),
            ("bad 0 0 one 0 1 1\n", "line 1"),
            ("bad 0 0 inf 0 1 1\n", "line 1"),
            ("\n\nbad 0 0 1 1\n", "line 3"),
            ("bad\n", "line 1"),
        ] {
            let e = parse_black_shroud(text).expect_err(text);
            assert!(e.starts_with(line), "{text:?}: {e}");
        }
    }

    /// The open-format load fills the region map's field from the folder's file; a missing file
    /// leaves no outlines, and a malformed one is skipped (no outlines, no panic).
    #[test]
    fn the_open_format_load_fills_the_region_map() {
        let dir = temp_dir("open_map_black_shroud");
        let folder = |name: &str| {
            let d = dir.join("campaigns").join(name);
            std::fs::create_dir_all(&d).unwrap();
            d
        };
        std::fs::write(folder("good").join(BLACK_SHROUD_FILE), "shroud_1 0 0 4 0 4 4 0 4\n").unwrap();
        std::fs::write(folder("bad").join(BLACK_SHROUD_FILE), "shroud_1 0 0 4\n").unwrap();
        folder("none");
        let mut vfs = Vfs::new();
        vfs.mount_dir(&dir, LayerKind::Loose).unwrap();
        let files = GameFiles { vfs: &vfs };
        let mut regions = RegionMap {
            bounds_min: (0.0, 0.0),
            bounds_max: (8.0, 8.0),
            theatre: ((0.0, 0.0), (8.0, 8.0)),
            labels: Vec::new(),
            vertices: Vec::new(),
            regions: Vec::new(),
            trade_nodes: Vec::new(),
            ground_types: Vec::new(),
            black_shroud_outlines: vec![("stale".into(), Vec::new())],
        };
        attach_black_shroud(&mut regions, &files, "campaigns/good/");
        assert_eq!(regions.black_shroud_outlines, [("shroud_1".to_owned(), vec![(0.0, 0.0), (4.0, 0.0), (4.0, 4.0), (0.0, 4.0)])]);
        attach_black_shroud(&mut regions, &files, "campaigns/bad");
        assert!(regions.black_shroud_outlines.is_empty(), "malformed: skipped");
        assert!(read_black_shroud(&files, "campaigns/none").is_empty(), "missing: none");
    }
}
