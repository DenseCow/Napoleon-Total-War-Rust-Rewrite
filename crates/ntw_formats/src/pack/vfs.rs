//! The virtual file system: packs and loose-file folders layered into one read-only view.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::{PackEntry, PackError, PackFile, PackGraph, PackType, normalize_path};

/// What role a mounted layer plays. Shown by the `--list-mods` report; together with the pack's
/// header type it gives the layer's [`Layer::rank`]. See `analysis/mods/MOD_LOADING.md`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LayerKind {
    /// A pack with header type 0 (`boot.pack`).
    Boot,
    /// A pack with header type 1 (`data.pack`, `local_en.pack`, ...).
    Release,
    /// A pack with header type 2 (`local_en_patch.pack`).
    Patch,
    /// A pack with header type 4 (`media.pack`). Loaded from `data\` automatically.
    Movie,
    /// Loose files in the install's `data\` folder (the original's "non_pack" files), or in
    /// a folder added with `add_working_directory`.
    Loose,
    /// A pack activated by a `mod "x.pack";` line in `user.script.txt` (or `import_all_mods`).
    ScriptMod,
    /// Our easy-mod layer: a pack or folder in the `mods\` folder (not in the original).
    ModsFolder,
    /// A pack mounted with [`Vfs::mount`] whose header type is unknown.
    Other,
}

impl LayerKind {
    /// The kind a pack gets from its header type when mounted with [`Vfs::mount`].
    pub fn from_pack_type(t: PackType) -> Self {
        match t {
            PackType::Boot => Self::Boot,
            PackType::Release => Self::Release,
            PackType::Patch => Self::Patch,
            PackType::Movie => Self::Movie,
            PackType::Mod => Self::ScriptMod,
            PackType::Other(_) => Self::Other,
        }
    }

    /// True for layers that come from mods (script mods and the mods folder).
    pub fn is_mod(self) -> bool {
        matches!(self, Self::ScriptMod | Self::ModsFolder)
    }

    /// True for the install's own packs (boot, release, patch, movie): a file of theirs that
    /// does not decode is an error, not a skipped mod file.
    pub fn is_install_pack(self) -> bool {
        matches!(self, Self::Boot | Self::Release | Self::Patch | Self::Movie)
    }

    /// A short lowercase name for reports, e.g. `"release"`.
    pub fn name(self) -> &'static str {
        match self {
            Self::Boot => "boot",
            Self::Release => "release",
            Self::Patch => "patch",
            Self::Movie => "movie",
            Self::Loose => "loose",
            Self::ScriptMod => "mod",
            Self::ModsFolder => "mods-folder",
            Self::Other => "other",
        }
    }
}

/// [`Layer::rank`] of loose files: below every pack. The original opens a path from a pack if any
/// pack has it and only then looks on disk (`VFS_OpenFileForRead` 0x0106A890, CONFIRMED); its folder
/// listings tag loose files with a marker whose pack type is -1 (0x0108E290, CONFIRMED).
pub const LOOSE_RANK: i32 = -1;

/// [`Layer::rank`] of our `mods\` folder: above every pack type of the original (ours).
pub const MODS_FOLDER_RANK: i32 = i32::MAX;

/// A folder of loose files mounted as one layer. The file list is taken once, at mount time.
#[derive(Debug)]
pub struct LooseDir {
    root: PathBuf,
    /// (normalized path, path relative to `root` as found on disk), sorted by normalized path.
    files: Vec<(String, PathBuf)>,
}

impl LooseDir {
    /// Scans `root` recursively. `.pack` files are skipped: they are archives, not content.
    pub fn scan(root: impl AsRef<Path>) -> std::io::Result<Self> {
        let root = root.as_ref().to_path_buf();
        let mut files = Vec::new();
        let mut stack = vec![PathBuf::new()];
        while let Some(rel_dir) = stack.pop() {
            for item in std::fs::read_dir(root.join(&rel_dir))? {
                let item = item?;
                let rel = rel_dir.join(item.file_name());
                let ty = item.file_type()?;
                if ty.is_dir() {
                    stack.push(rel);
                } else if ty.is_file() {
                    let is_pack = rel.extension().is_some_and(|e| e.eq_ignore_ascii_case("pack"));
                    if !is_pack {
                        files.push((normalize_path(&rel.to_string_lossy()), rel));
                    }
                }
            }
        }
        // Sorted by normalized path: lookups binary-search it, and reports are deterministic.
        files.sort();
        files.dedup_by(|a, b| a.0 == b.0);
        Ok(Self { root, files })
    }

    /// The folder this layer reads from.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The normalized paths of all files in this folder.
    pub fn paths(&self) -> impl Iterator<Item = &str> {
        self.files.iter().map(|(n, _)| n.as_str())
    }

    /// Number of files.
    pub fn len(&self) -> usize {
        self.files.len()
    }

    /// True if the folder has no files.
    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }

    /// Position of a normalized path in `files`.
    fn position(&self, norm: &str) -> Option<usize> {
        self.files.binary_search_by(|(n, _)| n.as_str().cmp(norm)).ok()
    }
}

/// Where a layer's files come from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayerSource {
    /// Index into [`Vfs::packs`].
    Pack(usize),
    /// Index into [`Vfs::loose_dirs`].
    Dir(usize),
}

/// One mounted layer: a pack or a loose folder, with its role.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Layer {
    /// The layer's role.
    pub kind: LayerKind,
    /// The pack or folder behind it.
    pub source: LayerSource,
    /// Priority class: a pack's header type (boot 0, release 1, patch 2, mod 3, movie 4; a pack
    /// named by a `mod` line keeps its own type), [`LOOSE_RANK`] for loose files,
    /// [`MODS_FOLDER_RANK`] for our `mods\` folder. A higher rank wins; on equal ranks the layer
    /// mounted first keeps the file (see [`Vfs::beats`]).
    pub rank: i32,
}

/// Which layer supplies a visible file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Hit {
    Pack { layer: u32, pack: u32, entry: u32 },
    Loose { layer: u32, dir: u32, file: u32 },
}

impl Hit {
    fn layer(self) -> usize {
        match self {
            Hit::Pack { layer, .. } | Hit::Loose { layer, .. } => layer as usize,
        }
    }
}

/// One visible path: the winning copy, and when the path was first indexed (the original lists
/// a folder's pack files in that order, `VFS_AddFileToDirectoryNode` 0x0105BD40, CONFIRMED).
#[derive(Debug, Clone, Copy)]
struct Slot {
    hit: Hit,
    first: u32,
}

/// One file of a DB table folder, as [`Vfs::table_files`] lists it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FolderFile {
    /// Normalized game path.
    pub path: String,
    /// The layer this copy comes from (index into [`Vfs::layers`]).
    pub layer: usize,
    /// The file name starts with `bob_` (case-sensitive, as in the original).
    pub bob: bool,
}

/// A read-only view over several packs and loose folders.
///
/// # Which copy wins (CONFIRMED, `analysis/mods/MOD_LOADING.md` §2)
/// When a layer brings a path that is already indexed, its copy replaces the old one only if
/// it [`beats`](Self::beats) the old layer: when both are packs of the precedence graph
/// ([`PackGraph`], `set_pack_file_precedence` / `set_pack_file_dependency`) the later node wins;
/// otherwise the higher [`Layer::rank`] (the pack's header type) wins. **On equal ranks the layer
/// mounted first keeps the file** (`VFS_ShouldPackOverride` 0x0108ED00). So `local_en_patch.pack` (patch, 2) beats
/// `local_en.pack` (release, 1) in any order, a mod pack (3) beats both, the first of two mod
/// packs wins, and loose files only fill paths that no pack has.
///
/// There are no write methods. Packs and folders are opened read-only.
///
/// ```no_run
/// use ntw_formats::pack::Vfs;
/// let vfs = Vfs::open_install(r"C:\Games\Napoleon Total War\data").unwrap();
/// let units = vfs.read("db/units_tables/units").unwrap();
/// ```
#[derive(Debug, Default)]
pub struct Vfs {
    packs: Vec<PackFile>,
    dirs: Vec<Arc<LooseDir>>,
    layers: Vec<Layer>,
    /// The `set_pack_file_precedence` / `set_pack_file_dependency` graph.
    graph: Arc<PackGraph>,
    /// Per layer: its pack's position in the graph's node order (`None`: not a node, or a folder).
    /// Set at mount time, so the priority rule does no lookup.
    graph_pos: Vec<Option<usize>>,
    /// Normalized path -> winning copy. Built once, when layers are mounted.
    index: HashMap<String, Slot>,
    /// `db\<folder>\` -> the paths directly in it, in the order they were first indexed (so
    /// [`table_files`](Self::table_files) does not scan the whole index for each table). Built
    /// with `index`, at mount time.
    db_folders: HashMap<String, Vec<String>>,
}

/// The file name of a path, lowercase (used for sorting and matching pack names).
pub(crate) fn lower_file_name(path: &Path) -> String {
    path.file_name().map(|n| n.to_string_lossy().to_ascii_lowercase()).unwrap_or_default()
}

/// For `local_en.pack` / `local_en_patch.pack` returns `Some("en")`; otherwise `None`.
pub(crate) fn language_of(file_name: &str) -> Option<String> {
    let rest = file_name.to_ascii_lowercase().strip_prefix("local_")?.to_owned();
    let lang: String = rest.chars().take_while(|c| c.is_ascii_alphabetic()).collect();
    (!lang.is_empty()).then_some(lang)
}

/// [`Layer::rank`] of a pack: its header type.
fn pack_rank(t: PackType) -> i32 {
    match t {
        PackType::Boot => 0,
        PackType::Release => 1,
        PackType::Patch => 2,
        PackType::Mod => 3,
        PackType::Movie => 4,
        PackType::Other(n) => i32::try_from(n).unwrap_or(i32::MAX - 1),
    }
}

/// The one priority rule ([`Vfs::beats`]), on borrowed parts so mounting can call it while it
/// updates the index.
fn layer_beats(layers: &[Layer], graph_pos: &[Option<usize>], new: usize, old: usize) -> bool {
    // Both packs are graph nodes: the graph decides, even for one pack against itself
    // (VFS_ShouldPackOverride 0x0108ED00 -> VFS_ComparePackPrecedenceGraph 0x0108EBB0).
    if let Some(replaces) = PackGraph::replaces(graph_pos[old], graph_pos[new]) {
        return replaces;
    }
    new != old && layers[new].rank > layers[old].rank
}

impl Vfs {
    /// An empty VFS with nothing mounted.
    pub fn new() -> Self {
        Self::default()
    }

    /// Builds a VFS from already-open packs, each mounted with the kind and rank of its header
    /// type, in the given order.
    pub fn from_packs(packs: Vec<PackFile>) -> Self {
        let mut vfs = Self::new();
        for p in packs {
            vfs.mount(p);
        }
        vfs
    }

    /// Sets the `set_pack_file_precedence` / `set_pack_file_dependency` graph. Call before
    /// mounting: the index is built as layers are mounted.
    pub fn set_pack_graph(&mut self, graph: Arc<PackGraph>) {
        debug_assert!(self.layers.is_empty(), "set the pack graph before mounting");
        self.graph = graph;
    }

    /// True if a copy from layer `new` replaces a copy from layer `old`: the single priority rule
    /// of the original (`VFS_ShouldPackOverride` 0x0108ED00, CONFIRMED), used for the file index
    /// and for DB rows ([`db_row_replaces`](Self::db_row_replaces)). When both layers are packs
    /// that are nodes of the [`PackGraph`], the later node wins (and a pack replaces itself);
    /// else the higher [`Layer::rank`] wins; equal ranks: `false` (the copy already there stays).
    pub fn beats(&self, new: usize, old: usize) -> bool {
        layer_beats(&self.layers, &self.graph_pos, new, old)
    }

    /// Mounts one more pack; its kind and rank come from its header type. Its files replace
    /// indexed ones only from layers it [`beats`](Self::beats).
    pub fn mount(&mut self, pack: PackFile) {
        let kind = LayerKind::from_pack_type(pack.pack_type());
        self.mount_as(pack, kind);
    }

    /// Mounts a pack with an explicit role. Its rank is its header type, except in our `mods\`
    /// folder ([`MODS_FOLDER_RANK`]).
    pub fn mount_as(&mut self, pack: PackFile, kind: LayerKind) {
        let rank = if kind == LayerKind::ModsFolder { MODS_FOLDER_RANK } else { pack_rank(pack.pack_type()) };
        let layer = self.layers.len() as u32;
        let pack_index = self.packs.len() as u32;
        self.layers.push(Layer { kind, source: LayerSource::Pack(pack_index as usize), rank });
        let node = if self.graph.is_empty() { None } else { self.graph.position(&lower_file_name(pack.path())) };
        self.graph_pos.push(node);
        let paths: Vec<String> = pack.entries().iter().map(|e| normalize_path(&e.path)).collect();
        self.packs.push(pack);
        for (i, norm) in paths.into_iter().enumerate() {
            self.insert(norm, Hit::Pack { layer, pack: pack_index, entry: i as u32 });
        }
    }

    /// Scans a folder and mounts its loose files.
    pub fn mount_dir(&mut self, root: impl AsRef<Path>, kind: LayerKind) -> Result<(), PackError> {
        self.mount_loose(LooseDir::scan(root)?, kind);
        Ok(())
    }

    /// Mounts an already scanned loose folder.
    pub fn mount_loose(&mut self, dir: LooseDir, kind: LayerKind) {
        self.mount_loose_shared(Arc::new(dir), kind);
    }

    /// Mounts a scanned loose folder that other VFSs share (the cached scan of `data\`, so
    /// every [`open_install`](Self::open_install) does not walk the folder again). Rank
    /// [`LOOSE_RANK`], or [`MODS_FOLDER_RANK`] for a folder in our `mods\` folder.
    pub fn mount_loose_shared(&mut self, dir: Arc<LooseDir>, kind: LayerKind) {
        let rank = if kind == LayerKind::ModsFolder { MODS_FOLDER_RANK } else { LOOSE_RANK };
        let layer = self.layers.len() as u32;
        let dir_index = self.dirs.len() as u32;
        self.layers.push(Layer { kind, source: LayerSource::Dir(dir_index as usize), rank });
        self.graph_pos.push(None);
        self.dirs.push(Arc::clone(&dir));
        for (i, (norm, _)) in dir.files.iter().enumerate() {
            self.insert(norm.clone(), Hit::Loose { layer, dir: dir_index, file: i as u32 });
        }
    }

    /// Indexes one copy of a path (mount time only).
    fn insert(&mut self, norm: String, hit: Hit) {
        let next = self.index.len() as u32;
        let Self { layers, graph_pos, index, db_folders, .. } = self;
        match index.entry(norm) {
            std::collections::hash_map::Entry::Vacant(v) => {
                if v.key().starts_with("db\\")
                    && let Some(cut) = v.key().rfind('\\')
                {
                    db_folders.entry(v.key()[..=cut].to_owned()).or_default().push(v.key().clone());
                }
                v.insert(Slot { hit, first: next });
            }
            std::collections::hash_map::Entry::Occupied(mut o) => {
                if layer_beats(layers, graph_pos, hit.layer(), o.get().hit.layer()) {
                    o.get_mut().hit = hit;
                }
            }
        }
    }

    /// The mounted packs, in mount order.
    pub fn packs(&self) -> &[PackFile] {
        &self.packs
    }

    /// The mounted loose folders, in mount order.
    pub fn loose_dirs(&self) -> &[Arc<LooseDir>] {
        &self.dirs
    }

    /// All layers in mount order. Which one wins a path is decided by [`beats`](Self::beats).
    pub fn layers(&self) -> &[Layer] {
        &self.layers
    }

    /// Layer indexes from the highest priority to the lowest (rank, then mount order); the
    /// precedence pairs are not applied here (reports only).
    pub fn layers_by_priority(&self) -> Vec<usize> {
        let mut order: Vec<usize> = (0..self.layers.len()).collect();
        order.sort_by(|&a, &b| self.layers[b].rank.cmp(&self.layers[a].rank).then(a.cmp(&b)));
        order
    }

    /// A human-readable location for a layer (the pack file or folder path).
    pub fn layer_path(&self, layer: &Layer) -> &Path {
        match layer.source {
            LayerSource::Pack(i) => self.packs[i].path(),
            LayerSource::Dir(i) => self.dirs[i].root(),
        }
    }

    /// Number of files a layer contains (visible or not).
    pub fn layer_len(&self, layer: &Layer) -> usize {
        match layer.source {
            LayerSource::Pack(i) => self.packs[i].entries().len(),
            LayerSource::Dir(i) => self.dirs[i].len(),
        }
    }

    /// The normalized paths a layer contains (visible or not).
    pub fn layer_paths(&self, layer: &Layer) -> Vec<String> {
        match layer.source {
            LayerSource::Pack(i) => self.packs[i].entries().iter().map(|e| normalize_path(&e.path)).collect(),
            LayerSource::Dir(i) => self.dirs[i].paths().map(str::to_owned).collect(),
        }
    }

    /// Index (into [`layers`](Self::layers)) of the layer that supplies `path`, if any.
    pub fn origin_index(&self, path: &str) -> Option<usize> {
        self.index.get(&normalize_path(path)).map(|s| s.hit.layer())
    }

    /// The layer that supplies `path`, if any.
    pub fn origin(&self, path: &str) -> Option<&Layer> {
        self.origin_index(path).map(|i| &self.layers[i])
    }

    /// Number of distinct paths visible through the VFS.
    pub fn len(&self) -> usize {
        self.index.len()
    }

    /// True if no files are visible.
    pub fn is_empty(&self) -> bool {
        self.index.is_empty()
    }

    /// The winning pack and entry for a path (case-insensitive, `/` or `\`).
    /// `None` if the path is missing **or** the winning copy is a loose file
    /// (use [`origin`](Self::origin) / [`read`](Self::read) for those).
    pub fn find(&self, path: &str) -> Option<(&PackFile, &PackEntry)> {
        match self.index.get(&normalize_path(path))?.hit {
            Hit::Pack { pack, entry, .. } => {
                let pack = &self.packs[pack as usize];
                Some((pack, &pack.entries()[entry as usize]))
            }
            Hit::Loose { .. } => None,
        }
    }

    /// The disk path of the winning loose file for `path`, if the winner is a loose file.
    pub fn loose_path(&self, path: &str) -> Option<PathBuf> {
        match self.index.get(&normalize_path(path))?.hit {
            Hit::Loose { dir, file, .. } => Some(self.loose_file_path(dir as usize, file as usize)),
            Hit::Pack { .. } => None,
        }
    }

    fn loose_file_path(&self, dir: usize, file: usize) -> PathBuf {
        let d = &self.dirs[dir];
        d.root.join(&d.files[file].1)
    }

    /// True if some mounted layer contains `path`.
    pub fn contains(&self, path: &str) -> bool {
        self.index.contains_key(&normalize_path(path))
    }

    /// Reads a file's contents from the copy that wins.
    pub fn read(&self, path: &str) -> Result<Vec<u8>, PackError> {
        match self.index.get(&normalize_path(path)).map(|s| s.hit) {
            Some(hit) => self.read_hit(hit),
            None => Err(PackError::NotFound(path.to_owned())),
        }
    }

    fn read_hit(&self, hit: Hit) -> Result<Vec<u8>, PackError> {
        match hit {
            Hit::Pack { pack, entry, .. } => {
                let pack = &self.packs[pack as usize];
                pack.read_entry(&pack.entries()[entry as usize])
            }
            Hit::Loose { dir, file, .. } => Ok(std::fs::read(self.loose_file_path(dir as usize, file as usize))?),
        }
    }

    /// Reads `len` bytes of a file starting `start` bytes in (clamped to the file), from the
    /// copy that wins. Used to stream large files such as movies.
    pub fn read_range(&self, path: &str, start: u64, len: usize) -> Result<Vec<u8>, PackError> {
        use std::io::{Read, Seek, SeekFrom};
        match self.index.get(&normalize_path(path)).map(|s| s.hit) {
            Some(Hit::Pack { pack, entry, .. }) => {
                let pack = &self.packs[pack as usize];
                pack.read_entry_range(&pack.entries()[entry as usize], start, len)
            }
            Some(Hit::Loose { dir, file, .. }) => {
                let mut f = std::fs::File::open(self.loose_file_path(dir as usize, file as usize))?;
                let size = f.metadata()?.len();
                let start = start.min(size);
                let len = (len as u64).min(size - start) as usize;
                f.seek(SeekFrom::Start(start))?;
                let mut buf = vec![0u8; len];
                f.read_exact(&mut buf)?;
                Ok(buf)
            }
            None => Err(PackError::NotFound(path.to_owned())),
        }
    }

    /// The size in bytes of the winning copy of `path`.
    pub fn file_size(&self, path: &str) -> Result<u64, PackError> {
        match self.index.get(&normalize_path(path)).map(|s| s.hit) {
            Some(Hit::Pack { pack, entry, .. }) => Ok(self.packs[pack as usize].entries()[entry as usize].size as u64),
            Some(Hit::Loose { dir, file, .. }) => Ok(std::fs::metadata(self.loose_file_path(dir as usize, file as usize))?.len()),
            None => Err(PackError::NotFound(path.to_owned())),
        }
    }

    /// All visible paths that start with `prefix` (normalized), sorted.
    /// The paths are returned normalized (lowercase, `\`). Example: `list("db/units_tables/")`.
    pub fn list(&self, prefix: &str) -> Vec<&str> {
        let prefix = normalize_path(prefix);
        let mut out: Vec<&str> =
            self.index.keys().filter(|k| k.starts_with(&prefix)).map(String::as_str).collect();
        out.sort_unstable();
        out
    }

    /// The files directly in `folder` (e.g. `db/units_tables/`) in the order the original's DB
    /// loader reads them (`VFS_ListFolderFiles` 0x01042C80, CONFIRMED): first every loose file
    /// of the [`LayerKind::Loose`] folders (first folder wins per name; within a folder in the
    /// order `FindFirstFileW` gives on NTFS, upper-cased names ascending), **even when a pack also
    /// has that name**; then the winning copy of each pack-held name, in the order the names were
    /// first indexed (mount order).
    pub fn table_files(&self, folder: &str) -> Vec<FolderFile> {
        let mut prefix = normalize_path(folder);
        if !prefix.ends_with('\\') {
            prefix.push('\\');
        }
        let direct = |p: &str| p.starts_with(&prefix) && !p[prefix.len()..].contains('\\');
        let file_name = |p: &Path| p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let mut out: Vec<FolderFile> = Vec::new();
        for (layer_index, layer) in self.layers.iter().enumerate() {
            let LayerSource::Dir(d) = layer.source else { continue };
            if layer.kind != LayerKind::Loose {
                continue;
            }
            // The folder's files are one run of the sorted list.
            let files = &self.dirs[d].files;
            let start = files.partition_point(|(n, _)| n.as_str() < prefix.as_str());
            let mut here: Vec<&(String, PathBuf)> =
                files[start..].iter().take_while(|(n, _)| n.starts_with(&prefix)).filter(|(n, _)| direct(n)).collect();
            here.sort_by_cached_key(|(_, rel)| file_name(rel).to_uppercase());
            for (norm, rel) in here {
                if !out.iter().any(|f| f.path == *norm) {
                    out.push(FolderFile { path: norm.clone(), layer: layer_index, bob: file_name(rel).starts_with("bob_") });
                }
            }
        }
        let packed_file = |n: &String, s: &Slot| {
            let bob = match s.hit {
                Hit::Pack { pack, entry, .. } => {
                    let p = &self.packs[pack as usize].entries()[entry as usize].path;
                    p.rsplit(['\\', '/']).next().is_some_and(|f| f.starts_with("bob_"))
                }
                Hit::Loose { dir, file, .. } => file_name(&self.dirs[dir as usize].files[file as usize].1).starts_with("bob_"),
            };
            FolderFile { path: n.clone(), layer: s.hit.layer(), bob }
        };
        let not_loose = |s: &Slot| self.layers[s.hit.layer()].kind != LayerKind::Loose;
        if prefix.starts_with("db\\") {
            // Indexed at mount time, already in first-indexed order.
            for n in self.db_folders.get(&prefix).into_iter().flatten() {
                if let Some(s) = self.index.get(n).filter(|s| not_loose(s)) {
                    out.push(packed_file(n, s));
                }
            }
        } else {
            let mut packed: Vec<(u32, FolderFile)> =
                self.index.iter().filter(|(n, s)| direct(n) && not_loose(s)).map(|(n, s)| (s.first, packed_file(n, s))).collect();
            packed.sort_by_key(|(first, _)| *first);
            out.extend(packed.into_iter().map(|(_, f)| f));
        }
        out
    }

    /// Reads the copy of a [`table_files`](Self::table_files) entry (a loose copy is read from its
    /// own folder even when a pack wins the path).
    pub fn read_folder_file(&self, file: &FolderFile) -> Result<Vec<u8>, PackError> {
        match self.layers.get(file.layer).map(|l| l.source) {
            Some(LayerSource::Dir(d)) => match self.dirs[d].position(&file.path) {
                Some(i) => Ok(std::fs::read(self.loose_file_path(d, i))?),
                None => Err(PackError::NotFound(file.path.clone())),
            },
            Some(LayerSource::Pack(_)) => self.read(&file.path),
            None => Err(PackError::NotFound(file.path.clone())),
        }
    }

    /// True if a DB row from `new` replaces the row with the same key from `old`
    /// (`DB_LoadTableFolderMergedByKey` 0x00E778A0, CONFIRMED): the file whose layer
    /// [`beats`](Self::beats) the other wins; on equal priority the new row replaces the old one
    /// only if the new file's name starts with `bob_`, otherwise the first row stays.
    pub fn db_row_replaces(&self, old: &FolderFile, new: &FolderFile) -> bool {
        if self.beats(new.layer, old.layer) {
            true
        } else if self.beats(old.layer, new.layer) {
            false
        } else {
            new.bob
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_util::{build_pack, temp_dir};
    use super::*;

    #[test]
    fn install_load_order_and_type_priority() {
        let _lock = super::LANGUAGE_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let dir = temp_dir("vfs_order");
        std::fs::write(dir.join("a_patch.pack"), build_pack(2, &[("text\\ui.loc", b"patch")])).unwrap();
        std::fs::write(dir.join("b_boot.pack"), build_pack(0, &[("text\\ui.loc", b"boot"), ("boot.txt", b"b")]))
            .unwrap();
        std::fs::write(
            dir.join("c_data.pack"),
            build_pack(1, &[("text\\ui.loc", b"release"), ("db\\x_tables\\x", b"x")]),
        )
        .unwrap();
        // A mod-type pack is NOT mounted by open_install: it needs a user.script.txt line.
        std::fs::write(dir.join("d_mod.pack"), build_pack(3, &[("db\\x_tables\\x", b"modded")])).unwrap();
        std::fs::write(dir.join("e_movie.pack"), build_pack(4, &[("movies\\m.bik", b"bik")])).unwrap();
        std::fs::write(dir.join("local_fr.pack"), build_pack(1, &[("text\\ui.loc", b"french")])).unwrap();
        std::fs::write(dir.join("language.txt"), "EN").unwrap();
        std::fs::write(dir.join("loose.txt"), "loose").unwrap();

        let vfs = Vfs::open_install(&dir).unwrap();
        let kinds: Vec<_> = vfs.layers().iter().map(|l| l.kind).collect();
        use LayerKind::*;
        // Boot, release, movie, patch (VFS_MountReleaseMoviePatchPacks 0x01084010), then loose.
        assert_eq!(kinds, [Boot, Release, Movie, Patch, Loose]);
        assert_eq!(vfs.read("TEXT/ui.loc").unwrap(), b"patch");
        assert_eq!(vfs.read("db/x_tables/x").unwrap(), b"x");
        assert_eq!(vfs.read("boot.txt").unwrap(), b"b");
        assert_eq!(vfs.read("Loose.TXT").unwrap(), b"loose");
        assert_eq!(vfs.origin("loose.txt").unwrap().kind, LayerKind::Loose);
        assert!(vfs.find("loose.txt").is_none(), "find() only reports pack winners");
        assert!(vfs.loose_path("loose.txt").unwrap().ends_with("loose.txt"));
        assert!(matches!(vfs.read("nope"), Err(PackError::NotFound(_))));
        assert_eq!(vfs.list("db/"), ["db\\x_tables\\x"]);
        // boot.txt, text\ui.loc, db\x_tables\x, movies\m.bik + 2 loose files.
        assert_eq!(vfs.len(), 6);
    }

    /// Rule: a higher header type wins in any mount order; equal types keep the copy mounted first.
    #[test]
    fn priority_by_type_then_first_mounted() {
        let dir = temp_dir("vfs_priority");
        std::fs::write(dir.join("patch.pack"), build_pack(2, &[("f", b"patch")])).unwrap();
        std::fs::write(dir.join("rel1.pack"), build_pack(1, &[("f", b"rel1"), ("g", b"rel1")])).unwrap();
        std::fs::write(dir.join("rel2.pack"), build_pack(1, &[("G", b"rel2")])).unwrap();
        let mut vfs = Vfs::new();
        vfs.mount(PackFile::open(dir.join("patch.pack")).unwrap());
        vfs.mount(PackFile::open(dir.join("rel1.pack")).unwrap());
        vfs.mount(PackFile::open(dir.join("rel2.pack")).unwrap());
        assert_eq!(vfs.read("f").unwrap(), b"patch", "patch (2) beats release (1) mounted later");
        assert_eq!(vfs.read("g").unwrap(), b"rel1", "equal types: the first mounted stays");
        assert!(vfs.beats(0, 1) && !vfs.beats(1, 0) && !vfs.beats(1, 2) && !vfs.beats(2, 1));
    }

    /// Rule: loose files never beat a pack; they only fill paths no pack has.
    #[test]
    fn loose_files_only_fill_gaps() {
        let dir = temp_dir("vfs_loose");
        let loose = dir.join("loose");
        std::fs::create_dir_all(loose.join("Sub Dir")).unwrap();
        std::fs::write(loose.join("Sub Dir").join("A.txt"), b"loose").unwrap();
        std::fs::write(loose.join("only_loose.txt"), b"only").unwrap();
        std::fs::write(loose.join("skipped.pack"), b"not content").unwrap();
        std::fs::write(dir.join("p.pack"), build_pack(1, &[("sub dir\\a.txt", b"packed")])).unwrap();
        let mut vfs = Vfs::new();
        vfs.mount_dir(&loose, LayerKind::Loose).unwrap();
        vfs.mount(PackFile::open(dir.join("p.pack")).unwrap());
        assert_eq!(vfs.read("sub dir/a.txt").unwrap(), b"packed");
        assert_eq!(vfs.read("ONLY_LOOSE.txt").unwrap(), b"only");
        assert!(!vfs.contains("skipped.pack"));
        // A folder of our mods\ folder ranks above every pack.
        vfs.mount_dir(&loose, LayerKind::ModsFolder).unwrap();
        assert_eq!(vfs.read("sub dir/a.txt").unwrap(), b"loose");
    }

    /// Rule: when both packs are graph nodes the later node wins, before the header types and in
    /// any mount order, also between two packs with no pair of their own (0x0108EBB0).
    #[test]
    fn precedence_graph_overrides_types() {
        let dir = temp_dir("vfs_precedence");
        std::fs::write(dir.join("a.pack"), build_pack(2, &[("f", b"a"), ("g", b"a")])).unwrap();
        std::fs::write(dir.join("b.pack"), build_pack(1, &[("f", b"b")])).unwrap();
        std::fs::write(dir.join("c.pack"), build_pack(3, &[("g", b"c"), ("f", b"c")])).unwrap();
        std::fs::write(dir.join("d.pack"), build_pack(1, &[("g", b"d")])).unwrap();
        let mut graph = PackGraph::new();
        graph.add_precedence("a.pack", "b.pack");
        graph.add_precedence("c.pack", "d.pack");
        assert_eq!(graph.order(), ["a.pack", "c.pack", "b.pack", "d.pack"]);
        let graph = Arc::new(graph);
        for order in [["a", "b", "c", "d"], ["d", "c", "b", "a"]] {
            let mut vfs = Vfs::new();
            vfs.set_pack_graph(Arc::clone(&graph));
            for p in order {
                vfs.mount(PackFile::open(dir.join(format!("{p}.pack"))).unwrap());
            }
            // b (release) is the last node holding f: it beats the patch a and the mod c.
            assert_eq!(vfs.read("f").unwrap(), b"b", "mount order {order:?}");
            // d (release) is after a and c.
            assert_eq!(vfs.read("g").unwrap(), b"d", "mount order {order:?}");
        }
        // A pack that is not a node: header types again.
        std::fs::write(dir.join("e.pack"), build_pack(3, &[("f", b"e")])).unwrap();
        let mut vfs = Vfs::new();
        vfs.set_pack_graph(Arc::clone(&graph));
        vfs.mount(PackFile::open(dir.join("b.pack")).unwrap());
        vfs.mount(PackFile::open(dir.join("e.pack")).unwrap());
        assert_eq!(vfs.read("f").unwrap(), b"e");
    }

    /// A graph-node pack replaces its own earlier copy (`VFS_ShouldPackOverride(p, p)` is true for
    /// a node), so two copies of one path in one pack: the later one; not a node: the first.
    #[test]
    fn a_graph_node_pack_replaces_itself() {
        let dir = temp_dir("vfs_precedence_self");
        std::fs::write(dir.join("a.pack"), build_pack(3, &[("f", b"first"), ("f", b"second")])).unwrap();
        let mut graph = PackGraph::new();
        graph.add_precedence("a.pack", "z.pack");
        let mut vfs = Vfs::new();
        vfs.set_pack_graph(Arc::new(graph));
        vfs.mount(PackFile::open(dir.join("a.pack")).unwrap());
        assert_eq!(vfs.read("f").unwrap(), b"second");
        let vfs = Vfs::from_packs(vec![PackFile::open(dir.join("a.pack")).unwrap()]);
        assert_eq!(vfs.read("f").unwrap(), b"first");
    }

    /// DB folder listing: loose files first (even when a pack has the name), then pack names in
    /// first-mount order; and the row rule (priority, then `bob_`).
    #[test]
    fn table_files_order_and_row_rule() {
        let dir = temp_dir("vfs_table_files");
        let loose = dir.join("loose");
        std::fs::create_dir_all(loose.join("db").join("t_tables").join("deep")).unwrap();
        std::fs::write(loose.join("db").join("t_tables").join("units"), b"L").unwrap();
        std::fs::write(loose.join("db").join("t_tables").join("bob_x"), b"L").unwrap();
        std::fs::write(loose.join("db").join("t_tables").join("deep").join("no"), b"L").unwrap();
        std::fs::write(dir.join("rel.pack"), build_pack(1, &[("db\\t_tables\\units", b"R"), ("db\\t_tables\\zz", b"R")]))
            .unwrap();
        std::fs::write(dir.join("m1.pack"), build_pack(3, &[("db\\t_tables\\units", b"M"), ("db\\t_tables\\aa", b"M")]))
            .unwrap();
        std::fs::write(dir.join("m2.pack"), build_pack(3, &[("db\\t_tables\\bob_y", b"N")])).unwrap();
        let mut vfs = Vfs::new();
        vfs.mount(PackFile::open(dir.join("rel.pack")).unwrap());
        vfs.mount_dir(&loose, LayerKind::Loose).unwrap();
        vfs.mount(PackFile::open(dir.join("m1.pack")).unwrap());
        vfs.mount(PackFile::open(dir.join("m2.pack")).unwrap());
        let files = vfs.table_files("db/t_tables");
        let names: Vec<(&str, usize)> = files.iter().map(|f| (f.path.as_str(), f.layer)).collect();
        assert_eq!(
            names,
            [
                ("db\\t_tables\\bob_x", 1),
                ("db\\t_tables\\units", 1),
                ("db\\t_tables\\units", 2),
                ("db\\t_tables\\zz", 0),
                ("db\\t_tables\\aa", 2),
                ("db\\t_tables\\bob_y", 3),
            ]
        );
        assert_eq!(vfs.read_folder_file(&files[1]).unwrap(), b"L", "the loose copy, though m1 wins the path");
        let (loose_units, m1_units, m2_bob) = (&files[1], &files[2], &files[5]);
        assert!(vfs.db_row_replaces(loose_units, m1_units), "a pack beats a loose file");
        assert!(!vfs.db_row_replaces(m1_units, loose_units));
        assert!(!vfs.db_row_replaces(m2_bob, m1_units), "equal types: first stays unless bob_");
        assert!(vfs.db_row_replaces(m1_units, m2_bob), "equal types: a bob_ file replaces");
    }

    /// The `db\` folder index built at mount time lists a folder exactly as the whole-index scan
    /// does (used for any other folder): same files, same first-indexed order, same winners.
    #[test]
    fn db_folder_index_matches_the_full_scan() {
        let dir = temp_dir("vfs_db_folder_index");
        let files = |root: &str| -> Vec<(String, Vec<u8>)> {
            ["zz", "units", "deep\\no", "bob_a", "aa"].iter().map(|n| (format!("{root}\\t_tables\\{n}"), n.as_bytes().to_vec())).collect()
        };
        for (name, t, root) in [("a.pack", 1, "db"), ("b.pack", 3, "db"), ("c.pack", 1, "xx"), ("d.pack", 3, "xx")] {
            let mut entries = files(root);
            if t == 3 {
                entries.reverse();
            }
            let refs: Vec<(&str, &[u8])> = entries.iter().map(|(p, b)| (p.as_str(), b.as_slice())).collect();
            std::fs::write(dir.join(name), build_pack(t, &refs)).unwrap();
        }
        let mut vfs = Vfs::new();
        for name in ["a.pack", "b.pack", "c.pack", "d.pack"] {
            vfs.mount(PackFile::open(dir.join(name)).unwrap());
        }
        let strip = |list: Vec<FolderFile>, root: &str| -> Vec<(String, usize, bool)> {
            list.into_iter().map(|f| (f.path.trim_start_matches(root).to_owned(), f.layer % 2, f.bob)).collect()
        };
        let indexed = strip(vfs.table_files("db/t_tables"), "db");
        assert_eq!(indexed, strip(vfs.table_files("xx/t_tables"), "xx"));
        assert_eq!(indexed.len(), 4, "the nested file is not in the folder");
    }

    #[test]
    fn language_detection() {
        assert_eq!(language_of("local_en_patch.pack").as_deref(), Some("en"));
        assert_eq!(language_of("LOCAL_FR.pack").as_deref(), Some("fr"));
        assert_eq!(language_of("data.pack"), None);
    }
}

/// The language override (our own setting; the install's `language.txt` is never written).
static LANGUAGE_OVERRIDE: std::sync::RwLock<Option<String>> = std::sync::RwLock::new(None);

/// Serialises the tests that depend on [`LANGUAGE_OVERRIDE`]: it is process-wide, and tests run
/// in parallel threads.
#[cfg(test)]
pub(crate) static LANGUAGE_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Sets (or clears) the language [`Vfs::open_install`] uses, e.g. from NapoleonRust's own
/// settings or `--language fr`. A language that is not installed is ignored (with no packs to
/// read, the text would be missing), so the install's language is used instead.
pub fn set_language_override(language: Option<&str>) {
    if let Ok(mut l) = LANGUAGE_OVERRIDE.write() {
        *l = language.map(|s| s.trim().to_ascii_lowercase()).filter(|s| !s.is_empty());
    }
}

/// The languages installed in `data_dir`: every `XX` with a `local_XX*.pack`, sorted, lower case.
pub fn installed_languages(data_dir: impl AsRef<Path>) -> Vec<String> {
    let mut out: Vec<String> = std::fs::read_dir(data_dir.as_ref())
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            name.to_ascii_lowercase().ends_with(".pack").then(|| language_of(&name)).flatten()
        })
        .collect();
    out.sort();
    out.dedup();
    out
}

/// The install's own language: `data\language.txt` (e.g. `EN`), lower case; `en` if missing.
pub fn install_language(data_dir: impl AsRef<Path>) -> String {
    std::fs::read_to_string(data_dir.as_ref().join("language.txt"))
        .map(|s| s.trim().trim_start_matches('\u{feff}').trim().to_ascii_lowercase())
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "en".to_owned())
}

/// The language [`Vfs::open_install`] mounts: the override if it is installed, else
/// [`install_language`].
pub fn effective_language(data_dir: impl AsRef<Path>) -> String {
    let data_dir = data_dir.as_ref();
    let over = LANGUAGE_OVERRIDE.read().ok().and_then(|l| l.clone());
    match over {
        Some(l) if installed_languages(data_dir).contains(&l) => l,
        _ => install_language(data_dir),
    }
}

#[cfg(test)]
mod language_tests {
    use super::*;

    #[test]
    fn languages_and_override() {
        let _lock = LANGUAGE_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let dir = std::env::temp_dir().join("napoleonrust_tests").join("languages");
        std::fs::create_dir_all(&dir).unwrap();
        for f in ["local_en.pack", "local_en_patch.pack", "local_FR.pack", "data.pack"] {
            std::fs::write(dir.join(f), b"").unwrap();
        }
        std::fs::write(dir.join("language.txt"), "EN").unwrap();
        assert_eq!(installed_languages(&dir), ["en", "fr"]);
        assert_eq!(install_language(&dir), "en");
        set_language_override(Some("FR"));
        assert_eq!(effective_language(&dir), "fr");
        set_language_override(Some("de"));
        assert_eq!(effective_language(&dir), "en");
        set_language_override(None);
        assert_eq!(effective_language(&dir), "en");
    }
}
