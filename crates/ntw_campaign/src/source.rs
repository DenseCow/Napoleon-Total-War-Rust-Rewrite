//! The campaign source seam (DESIGN.md §3.5.1, `analysis/modding/MODDING_AUDIT.md` §3.3): one entry
//! point, [`open`], gives the game a campaign as our own types, the [`CampaignModel`] (inside a
//! [`LoadedCampaign`]), its [`CampaignInfo`] and the [`MapData`] (the map display draws its
//! [`MapDisplay`]). Where the campaign comes from is a [`CampaignSource`]:
//! - [`OriginalSource`]: the original's importer (`campaigns/<key>/startpos.esf` + the pack tables,
//!   and the map folder `campaign_maps/<map>/`);
//! - later the open format (a folder with `campaign.toml` and images), as a second implementation.
//!
//! A save is not a source: it holds the model ([`crate::own_save`], or an original `.save` through
//! the importer) and names its campaign, whose source then gives the map.

use std::sync::Arc;

use ntw_data::GameDatabase;
use ntw_formats::campaign_map::{CampaignMap, FileSpan, GameFiles, SuperTexture, DISPLAY_TO_LOGIC, HEIGHT_SCALE};
use ntw_sim::campaign::features::CampaignFeatures;
use ntw_sim::campaign::{CampaignModel, Terrain};

use crate::header_map::TheatrePictures;
use crate::map_display::{GroundTexture, LineKind, MapDisplay, MapLine};
use crate::{CampaignInfo, LoadError, LoadedCampaign};

/// Reads one game file (a pack or loose `data\` path, `/` or `\`), `None` when it is not there.
pub type ReadFile<'a> = dyn Fn(&str) -> Option<Vec<u8>> + 'a;

/// Why a campaign could not be opened.
#[derive(Debug)]
pub enum SourceError {
    /// No source has a campaign with this key.
    NotFound(String),
    /// The campaign's data or the save could not be read.
    Load(LoadError),
    /// The campaign's map could not be read.
    Map(String),
}

impl std::fmt::Display for SourceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound(key) => write!(f, "no campaign {key}"),
            Self::Load(e) => write!(f, "{e}"),
            Self::Map(e) => write!(f, "map: {e}"),
        }
    }
}

impl std::error::Error for SourceError {}

impl From<LoadError> for SourceError {
    fn from(e: LoadError) -> Self {
        Self::Load(e)
    }
}

/// What the map display, the model and the save header need from a campaign's map.
pub struct MapData {
    /// What the map display draws.
    pub display: Arc<MapDisplay>,
    /// The movement grid.
    pub terrain: Terrain,
    /// The theatres' base and lookup pictures for the save header's territory maps, one per
    /// theatre of the campaign's header.
    pub theatre_pictures: Vec<TheatrePictures>,
}

impl MapData {
    /// Puts the map's parts of the campaign state into `model`: the movement grid, the region
    /// neighbours and the trade nodes' regions (CAMPAIGN_FIDELITY.md §Trade). Called on every load,
    /// whatever the model came from.
    pub fn attach(&self, model: &mut CampaignModel) {
        model.terrain = Some(self.terrain.clone());
        crate::trade::attach_map(model, &self.display.regions);
    }
}

/// A place campaigns come from.
pub trait CampaignSource {
    /// The campaign's key, e.g. `eur_napoleon`.
    fn key(&self) -> &str;
    /// The facts the front end shows (year, map, default faction, theatres), without building the
    /// model.
    fn info(&self) -> Result<CampaignInfo, SourceError>;
    /// The campaign at its start: turn 1, not started, no human chosen yet.
    fn new_campaign(&self, db: &GameDatabase) -> Result<LoadedCampaign, SourceError>;
    /// The campaign's map, for the facts in `info` (its map key and header theatres).
    fn map(&self, db: &GameDatabase, info: &CampaignInfo) -> Result<MapData, SourceError>;
    /// The campaign's own rule switches from its data, replacing the ones the rules get from the
    /// campaign key ([`crate::features::original`]). `None` (the original's importer): the key's.
    fn features(&self) -> Option<CampaignFeatures> {
        None
    }
}

/// The original's start position of campaign `key`, as a game path.
fn startpos_path(key: &str) -> String {
    format!("campaigns/{key}/startpos.esf")
}

/// The original's campaigns: their keys (the `campaigns/<key>/` folders with a start position),
/// sorted.
pub fn original_campaigns(files: &GameFiles<'_>) -> Vec<String> {
    let mut keys: Vec<String> = files
        .list("campaigns/")
        .iter()
        .filter(|p| p.ends_with("\\startpos.esf"))
        .filter_map(|p| p.split('\\').nth(1).map(str::to_owned))
        .collect();
    keys.dedup();
    keys
}

/// The front-end facts of campaign `key` from its source, reading files with `read`; `None` when
/// no source has it or its data is unreadable. For the UI, which reads game files through its own
/// file source.
pub fn campaign_info(read: &ReadFile<'_>, key: &str) -> Option<CampaignInfo> {
    crate::read_info(&read(&startpos_path(key))?).ok()
}

/// The original's importer: start position, pack tables and map folder.
pub struct OriginalSource<'a> {
    files: GameFiles<'a>,
    key: String,
}

impl<'a> OriginalSource<'a> {
    /// The original's campaign `key`, if the install has its start position.
    pub fn find(files: GameFiles<'a>, key: &str) -> Option<Self> {
        let path = ntw_formats::pack::normalize_path(&startpos_path(key));
        files.list(&format!("campaigns/{key}/")).iter().any(|p| p.eq_ignore_ascii_case(&path)).then(|| OriginalSource { files, key: key.to_owned() })
    }

    fn startpos(&self) -> Result<Vec<u8>, SourceError> {
        self.files.read(&startpos_path(&self.key)).map_err(|e| SourceError::Load(LoadError::Io { path: startpos_path(&self.key).into(), error: std::io::Error::other(e) }))
    }
}

impl CampaignSource for OriginalSource<'_> {
    fn key(&self) -> &str {
        &self.key
    }

    fn info(&self) -> Result<CampaignInfo, SourceError> {
        Ok(crate::read_info(&self.startpos()?)?)
    }

    fn new_campaign(&self, db: &GameDatabase) -> Result<LoadedCampaign, SourceError> {
        Ok(crate::read(&self.startpos()?, db)?)
    }

    fn map(&self, db: &GameDatabase, info: &CampaignInfo) -> Result<MapData, SourceError> {
        let map = CampaignMap::load(&self.files, &info.map_key).map_err(|e| SourceError::Map(format!("{}: {e}", info.map_key)))?;
        let terrain = Terrain(Arc::new(crate::pathing::build_grid(&map)));
        let display = Arc::new(original_display(&self.files, map));
        let theatre_pictures = info.header.maps.iter().filter_map(|m| TheatrePictures::load(&self.files, db, &info.map_key, &m.theatre)).collect();
        Ok(MapData { display, terrain, theatre_pictures })
    }
}

/// The map folder's movement-arrow model (CONFIRMED: every map has it, and its shape is the
/// movement arrow).
const ARROW_MODEL: &str = "display/arrows/arrows.rigid_model";
/// The map folder's river texture (`fx\campaignriver.fx`'s diffuse).
const RIVER_TEXTURE: &str = "display/rivers/textures/river_diffuse.dds";

/// The importer's translation of the original's decoded map folder into the display's own type:
/// spline folders become line kinds with their points in logic units, the supertexture becomes a
/// [`GroundTexture`] reading its tiles through the game's files (so a map mod's pack wins over the
/// loose file, as for every other map file), the map folder's river texture and arrow model are
/// read here. The pathfinding and sea-grid files stay with the importer (the movement grid).
pub fn original_display(files: &GameFiles<'_>, map: CampaignMap) -> MapDisplay {
    let base = format!("campaign_maps/{}", map.name);
    let ground = map.supertexture.and_then(|index| {
        let path = format!("{base}/display/supertexture/supertexture.stpd");
        match files.locate(&path) {
            Ok(span) => Some(Box::new(OriginalGround::new(index, span)) as Box<dyn GroundTexture>),
            Err(e) => {
                log::warn!("Campaign map supertexture: {e}");
                None
            }
        }
    });
    let lines = map
        .splines
        .into_iter()
        .filter_map(|(folder, s)| {
            let kind = match folder.as_str() {
                "borders" => LineKind::Border,
                "rivers" => LineKind::River,
                "roads" => LineKind::Road,
                "traderoutes" => LineKind::TradeRoute,
                _ => return None,
            };
            Some(MapLine { kind, points: s.points.iter().map(|p| (p[0] * DISPLAY_TO_LOGIC, p[2] * DISPLAY_TO_LOGIC)).collect() })
        })
        .collect();
    MapDisplay {
        river_texture: files.read(&format!("{base}/{RIVER_TEXTURE}")),
        arrow_model: files.read(&format!("{base}/{ARROW_MODEL}")),
        key: map.name,
        regions: map.regions,
        heightmap: map.heightmap,
        height_scale: HEIGHT_SCALE,
        ground,
        lines,
        coast: map.coast,
        trees: map.trees,
    }
}

/// The original's supertexture (`supertexture.stpi` index + `.stpd` tiles) as a
/// [`GroundTexture`]: each tile is read on request (seek and read its bytes only), inflated and
/// DXT5-decoded.
struct OriginalGround {
    index: SuperTexture,
    levels: Vec<(u32, u32)>,
    stpd: FileSpan,
}

impl OriginalGround {
    fn new(index: SuperTexture, stpd: FileSpan) -> Self {
        let levels = index.levels.iter().map(|l| (l.tiles_x, l.tiles_y)).collect();
        Self { index, levels, stpd }
    }
}

impl GroundTexture for OriginalGround {
    fn tile_size(&self) -> u32 {
        self.index.tile_size
    }

    fn levels(&self) -> &[(u32, u32)] {
        &self.levels
    }

    fn window_rgba(&self, level: usize, tx0: u32, ty0: u32, nx: u32, ny: u32) -> Result<Vec<u8>, String> {
        let mut file = std::fs::File::open(&self.stpd.file).map_err(|e| format!("{}: {e}", self.stpd.file.display()))?;
        self.index
            .window_rgba(level, (tx0, ty0, nx, ny), |tile| {
                self.stpd.read_at(&mut file, u64::from(tile.offset), tile.size as usize).map_err(ntw_formats::campaign_map::CampaignMapError::Inner)
            })
            .map_err(|e| format!("supertexture level {level}, tiles {tx0},{ty0} +{nx}x{ny}: {e}"))
    }
}

/// The source of campaign `key` (today only the original's campaigns have one).
pub fn find<'a>(files: GameFiles<'a>, key: &str) -> Option<Box<dyn CampaignSource + 'a>> {
    OriginalSource::find(files, key).map(|s| Box::new(s) as Box<dyn CampaignSource + 'a>)
}

/// How a campaign starts.
#[derive(Debug, Clone, Copy)]
pub enum Start<'a> {
    /// A new campaign of this key, with the player's `campaign_unit_multiplier` preference (`None`:
    /// its default), which sets the new campaign's units per army / navy
    /// ([`ntw_sim::campaign::rules::ForceCaps::new_campaign`]).
    New(&'a str, Option<f32>),
    /// A save: one of ours ([`crate::own_save`]) or an original `.save` (read through the importer).
    Save(&'a [u8]),
}

/// A campaign ready to play: the model with its map parts attached, and the map.
pub struct OpenedCampaign {
    /// The model, its facts and the scripts' saved slots.
    pub loaded: LoadedCampaign,
    /// The map.
    pub map: MapData,
}

/// The one entry point: opens a new campaign or a save, with its campaign's map.
pub fn open(files: GameFiles<'_>, start: Start<'_>, db: &GameDatabase) -> Result<OpenedCampaign, SourceError> {
    let (source, mut loaded) = match start {
        Start::New(key, unit_multiplier) => {
            let source = find(files, key).ok_or_else(|| SourceError::NotFound(key.to_owned()))?;
            let mut loaded = source.new_campaign(db)?;
            // Not the start position's own 20 / 14: a new campaign sets its caps (`0x00872550`).
            loaded.model.force_caps = ntw_sim::campaign::rules::ForceCaps::new_campaign(&loaded.model.rules.limits, unit_multiplier);
            (source, loaded)
        }
        Start::Save(bytes) => {
            let loaded = crate::read(bytes, db)?;
            let key = &loaded.info.campaign_key;
            let source = find(files, key).ok_or_else(|| SourceError::NotFound(key.clone()))?;
            (source, loaded)
        }
    };
    if let Some(features) = source.features() {
        Arc::make_mut(&mut loaded.model.rules).features = features;
    }
    let map = source.map(db, &loaded.info)?;
    map.attach(&mut loaded.model);
    // The names new characters get when they are created (game data, like the rules).
    crate::names::attach(&mut loaded.model, files.vfs, db);
    Ok(OpenedCampaign { loaded, map })
}

#[cfg(test)]
mod tests {
    use ntw_formats::pack::Vfs;

    use super::*;
    use crate::own_save::{self, SaveData};

    /// A save names its campaign: when no source has that campaign, opening the save is an error
    /// naming it, not a campaign without a map; bytes that are no save are a load error.
    #[test]
    fn a_save_of_an_unknown_campaign_is_not_found() {
        let info = own_save::tests::info();
        let data = SaveData { human: "made_up_a".into(), model: own_save::tests::made_up_model(), rebel_faction: None, script_values: Vec::new(), restricted_units: Vec::new() };
        let bytes = own_save::write(&info, &data).expect("write");
        let vfs = Vfs::new();
        let files = GameFiles { vfs: &vfs };
        let db = GameDatabase::test_fixture();
        match open(files, Start::Save(&bytes), &db) {
            Err(SourceError::NotFound(k)) => assert_eq!(k, "made_up_campaign"),
            Err(e) => panic!("wrong error: {e}"),
            Ok(_) => panic!("opened a campaign no source has"),
        }
        assert!(find(files, "made_up_campaign").is_none());
        assert!(original_campaigns(&files).is_empty());
        assert!(matches!(open(files, Start::Save(b"not a save at all"), &db), Err(SourceError::Load(_))));
    }
}
