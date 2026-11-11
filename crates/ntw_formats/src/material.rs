//! Material descriptions (DESIGN.md §3.7.2): the data that says how a mesh's textures and
//! constants become the inputs of our physically based renderer. One description is one text
//! file, `materials/<key>.toml`, read through the game's files like every other game file, so a
//! mod replaces a description by shipping the same path and adds one by shipping a new path. Our
//! own defaults (the translation of the original's materials, [`BUILT_IN`]) are files of the
//! same format compiled into the program; a game file of the same key replaces one whole.
//!
//! A description is a recipe, not one mesh's material: it names texture *slots* ("diffuse",
//! "gloss", ...) and the mesh that uses it supplies the textures for them (a `.rigid_model`
//! material's own texture names, a unit part's texture stem, a glTF material). `[applies]` says
//! which meshes get the description by default.
//!
//! ```text
//! shader = "shaders/wet_stone.wgsl"   # optional: a mod's own surface function (WGSL)
//!
//! [applies]                 # optional: the meshes that get this description by default
//! source = "rigid_model"    # the kind of mesh (each mesh importer names its own)
//! path = "rigidmodels/buildings/"   # optional path prefix; the longest matching prefix wins
//!
//! [alpha]
//! mode = "opaque"           # "opaque", "mask" or "blend"
//! cutoff = 0.5              # the mask threshold
//! two_sided = false
//! source_flags = true       # the mesh's own flags, where it has them, replace mode and two_sided
//!
//! [base_colour]             # also [specular_tint] and [emissive]: a colour input
//! texture = "diffuse"       # a slot's red, green and blue, multiplied by `value`
//! value = [1.0, 1.0, 1.0]   # the colour itself when the mesh has no such texture
//!
//! [roughness]               # also [opacity], [metalness], [specular_level], [occlusion]: 0 to 1
//! value = 0.8               # the value when the mesh has no such texture
//! texture = "gloss.a"       # one channel of a slot: <slot>.r, .g, .b or .a
//! convert = "specular_power" # "none", "one_minus", or "specular_power" (with `power`)
//! power = 32.0              # the Blinn-Phong exponent a sample of 1.0 stands for
//! scale = 1.0               # then: converted sample * scale + bias, kept within 0 to 1
//! bias = 0.0
//!
//! [normal]
//! texture = "normal"
//! encoding = "rgb"          # "rgb"; "ag" (x in alpha, y in green); "rg" (z rebuilt)
//! flip_green = false
//! scale = 1.0
//!
//! [faction_colour]
//! texture = "colour_mask"   # red, green and blue select the faction's three colours
//!
//! [baked]                   # shading painted into the base colour texture
//! base_colour_gain = 1.0    # multiplies the base colour
//! occlusion = 0.0           # 0 to 1: the share of ambient occlusion already painted in
//!
//! [constants]               # the mesh's own named constants -> the field each one replaces
//! specpower = "roughness.power"   # <input>.value/.scale/.bias/.power, normal.scale, param.<name>
//!
//! [params]                  # extra named numbers for the mod's shader
//! wetness = 0.5
//! ```
//!
//! An unknown table or key, a wrong type and a value out of range are errors that name the line:
//! a misspelt key must not silently do nothing.

use std::collections::BTreeMap;

use crate::campaign_map::GameFiles;
use crate::pack::normalize_path;
use crate::toml_subset::{self, Entry, Table, Value};

/// The folder of the game's files that holds the material descriptions.
pub const MATERIAL_FOLDER: &str = "materials";
/// A description file's extension; other files in the folder (a mod's shaders) are not read here.
pub const MATERIAL_EXTENSION: &str = ".toml";
/// The key of the description a mesh gets when no other applies to it.
pub const DEFAULT_MATERIAL: &str = "default";

/// Our own descriptions, compiled in: (key, file text). `vanilla/*` is the default translation
/// of the original's materials. Its channel meanings and hand-picked numbers stay tagged until the
/// original's shaders are read (BACKLOG §2 "The original's materials read in full"); each such
/// value carries its tag in a comment in the file.
pub const BUILT_IN: &[(&str, &str)] = &[
    (DEFAULT_MATERIAL, include_str!("materials/default.toml")),
    ("vanilla/mount", include_str!("materials/vanilla/mount.toml")),
    ("vanilla/rigid_model", include_str!("materials/vanilla/rigid_model.toml")),
    ("vanilla/unit_part", include_str!("materials/vanilla/unit_part.toml")),
];

/// One material description: see the module text for the file it is read from.
#[derive(Debug, Clone, PartialEq)]
pub struct MaterialDescription {
    /// The file's path under [`MATERIAL_FOLDER`] without its extension, `/`-separated, lower case
    /// (`vanilla/rigid_model`). A mesh or a mod names a description by it.
    pub key: String,
    /// Which meshes get this description by default; `None` for one that is only named.
    pub applies: Option<Applies>,
    /// Transparency and culling.
    pub alpha: Alpha,
    /// The surface colour without lighting.
    pub base_colour: ColourInput,
    /// How solid the surface is (1 = opaque); what [`Alpha::mode`] tests or blends.
    pub opacity: ScalarInput,
    /// The tangent-space normal map; `None` shades with the mesh's own normals.
    pub normal: Option<NormalInput>,
    /// Perceptual roughness (0 = mirror, 1 = matt).
    pub roughness: ScalarInput,
    /// Metalness (0 = non-metal, 1 = metal).
    pub metalness: ScalarInput,
    /// The strength of a non-metal's reflection (0.5 = the common 4%).
    pub specular_level: ScalarInput,
    /// The colour of a non-metal's reflection.
    pub specular_tint: ColourInput,
    /// Ambient occlusion from the material (1 = none); screen-space occlusion is separate.
    pub occlusion: ScalarInput,
    /// Light the surface gives off.
    pub emissive: ColourInput,
    /// The faction colour mask; `None` for a material that takes no faction colours.
    pub faction_colour: Option<FactionColour>,
    /// The compensation for shading painted into the base colour texture.
    pub baked: BakedShading,
    /// The mesh's own named constants and the field each one replaces
    /// ([`MaterialDescription::apply_constant`]), in file order.
    pub constants: Vec<(String, ConstantTarget)>,
    /// Extra named numbers a mod's shader reads, in file order.
    pub params: Vec<(String, f32)>,
    /// The game-file path of a mod's WGSL surface function; `None` for the built-in surface.
    pub shader: Option<String>,
}

/// Which meshes get a description by default.
#[derive(Debug, Clone, PartialEq)]
pub struct Applies {
    /// The kind of mesh, as its importer names it (`rigid_model`, `unit_variant`, ...).
    pub source: String,
    /// A game-file path prefix (normalized: lower case, `\`); empty for every mesh of the source.
    pub path: String,
}

/// Transparency and culling.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Alpha {
    /// How opacity is used.
    pub mode: AlphaMode,
    /// The threshold of [`AlphaMode::Mask`]: opacity below it is not drawn.
    pub cutoff: f32,
    /// Draw both faces.
    pub two_sided: bool,
    /// A mesh that carries its own flags (a rigid model's `_alphatest`, `_alphablend` and
    /// `_twosided` file-name flags, a glTF material's alpha mode) replaces `mode` and
    /// `two_sided` with them.
    pub source_flags: bool,
}

/// How a material's opacity is used.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlphaMode {
    /// Opacity is ignored.
    Opaque,
    /// Drawn where opacity reaches the cutoff, solid there.
    Mask,
    /// Blended over what is behind it.
    Blend,
}

/// A colour input: a texture slot's red, green and blue times `value`, or `value` alone.
#[derive(Debug, Clone, PartialEq)]
pub struct ColourInput {
    /// The colour, or the factor on the texture.
    pub value: [f32; 3],
    /// The texture slot, if the colour comes from a texture.
    pub texture: Option<String>,
}

/// A single-number input (0 to 1): one channel of a texture slot, converted, or `value`.
#[derive(Debug, Clone, PartialEq)]
pub struct ScalarInput {
    /// The value of a mesh that has no texture in the slot (and of an input without one).
    pub value: f32,
    /// The slot and channel, if the value comes from a texture.
    pub texture: Option<TextureChannel>,
    /// What the texture's sample means.
    pub convert: Convert,
    /// The factor on the converted sample.
    pub scale: f32,
    /// Added after `scale`.
    pub bias: f32,
}

/// One channel of a texture slot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextureChannel {
    /// The slot's name.
    pub slot: String,
    /// The channel read from it.
    pub channel: Channel,
}

/// A texture channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Channel {
    /// Red.
    R,
    /// Green.
    G,
    /// Blue.
    B,
    /// Alpha.
    A,
}

/// What a texture sample means for a [`ScalarInput`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Convert {
    /// The sample is the value.
    None,
    /// The value is 1 - sample (a glossiness map read as roughness).
    OneMinus,
    /// The sample scales a Blinn-Phong specular exponent, `power` at a sample of 1.0, and the
    /// value is the roughness of the same highlight width: `(2 / (exponent + 2))^(1/4)`.
    SpecularPower {
        /// The exponent a sample of 1.0 stands for.
        power: f32,
    },
}

/// The normal map of a material.
#[derive(Debug, Clone, PartialEq)]
pub struct NormalInput {
    /// The texture slot.
    pub texture: String,
    /// How the normal is stored in the texture.
    pub encoding: NormalEncoding,
    /// The texture's green axis points the other way (the "DirectX" convention).
    pub flip_green: bool,
    /// The strength of the bumps (1 = as stored).
    pub scale: f32,
}

/// How a normal map stores its normal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NormalEncoding {
    /// x, y and z in red, green and blue.
    Rgb,
    /// x in alpha and y in green (the swizzled DXT5 form); z is rebuilt.
    Ag,
    /// x in red and y in green; z is rebuilt.
    Rg,
}

/// The faction colour mask of a material.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FactionColour {
    /// The slot of the mask, whose red, green and blue select the faction's three colours.
    pub texture: String,
}

/// The compensation for shading painted into a base colour texture.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BakedShading {
    /// Multiplies the base colour (1 = as painted).
    pub base_colour_gain: f32,
    /// The share (0 to 1) of ambient occlusion already painted in: screen-space occlusion is
    /// weakened by it on this material, so creases are not darkened twice.
    pub occlusion: f32,
}

/// The field of a description that a mesh's named constant replaces.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConstantTarget {
    /// A field of one of the single-number inputs.
    Scalar(ScalarSlot, ScalarField),
    /// [`NormalInput::scale`].
    NormalScale,
    /// A [`MaterialDescription::params`] entry, by name.
    Param(String),
}

/// The single-number inputs of a description.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScalarSlot {
    /// [`MaterialDescription::opacity`].
    Opacity,
    /// [`MaterialDescription::roughness`].
    Roughness,
    /// [`MaterialDescription::metalness`].
    Metalness,
    /// [`MaterialDescription::specular_level`].
    SpecularLevel,
    /// [`MaterialDescription::occlusion`].
    Occlusion,
}

/// A field of a [`ScalarInput`] a constant can replace.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScalarField {
    /// [`ScalarInput::value`].
    Value,
    /// [`ScalarInput::scale`].
    Scale,
    /// [`ScalarInput::bias`].
    Bias,
    /// The `power` of [`Convert::SpecularPower`].
    Power,
}

impl ScalarSlot {
    /// Every single-number input with its table name.
    const ALL: [(Self, &'static str); 5] = [
        (Self::Opacity, "opacity"),
        (Self::Roughness, "roughness"),
        (Self::Metalness, "metalness"),
        (Self::SpecularLevel, "specular_level"),
        (Self::Occlusion, "occlusion"),
    ];

    fn from_name(name: &str) -> Option<Self> {
        Self::ALL.iter().find(|(_, n)| *n == name).map(|(slot, _)| *slot)
    }

    fn name(self) -> &'static str {
        Self::ALL.iter().find(|(slot, _)| *slot == self).map_or("", |(_, n)| n)
    }
}

impl ScalarInput {
    /// An input that is the number `value`.
    const fn constant(value: f32) -> Self {
        Self { value, texture: None, convert: Convert::None, scale: 1.0, bias: 0.0 }
    }

    /// The input's value for a texture `sample` (0 to 1) of its channel, or for a mesh with no
    /// texture in the slot (`None`). This is the rule the surface shader applies per pixel.
    pub fn resolve(&self, sample: Option<f32>) -> f32 {
        let Some(sample) = sample else { return self.value };
        let converted = match self.convert {
            Convert::None => sample,
            Convert::OneMinus => 1.0 - sample,
            Convert::SpecularPower { power } => (2.0 / (power * sample + 2.0)).powf(0.25),
        };
        (converted * self.scale + self.bias).clamp(0.0, 1.0)
    }
}

impl MaterialDescription {
    /// A description with every input at its default: white, opaque, non-metal, roughness 0.5,
    /// no textures.
    pub fn new(key: &str) -> Self {
        Self {
            key: key.to_owned(),
            applies: None,
            alpha: Alpha { mode: AlphaMode::Opaque, cutoff: 0.5, two_sided: false, source_flags: true },
            base_colour: ColourInput { value: [1.0; 3], texture: None },
            opacity: ScalarInput::constant(1.0),
            normal: None,
            roughness: ScalarInput::constant(0.5),
            metalness: ScalarInput::constant(0.0),
            specular_level: ScalarInput::constant(0.5),
            specular_tint: ColourInput { value: [1.0; 3], texture: None },
            occlusion: ScalarInput::constant(1.0),
            emissive: ColourInput { value: [0.0; 3], texture: None },
            faction_colour: None,
            baked: BakedShading { base_colour_gain: 1.0, occlusion: 0.0 },
            constants: Vec::new(),
            params: Vec::new(),
            shader: None,
        }
    }

    /// One of the single-number inputs.
    pub fn scalar(&self, slot: ScalarSlot) -> &ScalarInput {
        match slot {
            ScalarSlot::Opacity => &self.opacity,
            ScalarSlot::Roughness => &self.roughness,
            ScalarSlot::Metalness => &self.metalness,
            ScalarSlot::SpecularLevel => &self.specular_level,
            ScalarSlot::Occlusion => &self.occlusion,
        }
    }

    fn scalar_mut(&mut self, slot: ScalarSlot) -> &mut ScalarInput {
        match slot {
            ScalarSlot::Opacity => &mut self.opacity,
            ScalarSlot::Roughness => &mut self.roughness,
            ScalarSlot::Metalness => &mut self.metalness,
            ScalarSlot::SpecularLevel => &mut self.specular_level,
            ScalarSlot::Occlusion => &mut self.occlusion,
        }
    }

    /// Puts a mesh's own named constant (`specpower` = 24.0 of a `.rigid_model` material) into
    /// the field `[constants]` binds it to. `Ok(true)`: replaced; `Ok(false)`: this description
    /// does not bind the name (the constant has no use here); `Err`: the value cannot go in the
    /// field (not finite, or outside the field's range), and the description keeps its own.
    pub fn apply_constant(&mut self, name: &str, value: f32) -> Result<bool, String> {
        let Some(target) = self.constants.iter().find(|(n, _)| n == name).map(|(_, t)| t.clone()) else { return Ok(false) };
        let check = |(min, max): (f32, f32)| {
            if value.is_finite() && (min..=max).contains(&value) { Ok(()) } else { Err(format!("constant {name} = {value} is outside {min} to {max}")) }
        };
        match target {
            ConstantTarget::NormalScale => {
                check(ANY)?;
                // A description that binds normal.scale has a [normal] table (checked on parse).
                if let Some(normal) = &mut self.normal {
                    normal.scale = value;
                }
            }
            ConstantTarget::Param(param) => {
                check(ANY)?;
                if let Some(slot) = self.params.iter_mut().find(|(n, _)| *n == param) {
                    slot.1 = value;
                }
            }
            ConstantTarget::Scalar(slot, field) => {
                check(match field {
                    ScalarField::Value => UNIT,
                    ScalarField::Scale | ScalarField::Bias => ANY,
                    ScalarField::Power => POSITIVE,
                })?;
                let input = self.scalar_mut(slot);
                match field {
                    ScalarField::Value => input.value = value,
                    ScalarField::Scale => input.scale = value,
                    ScalarField::Bias => input.bias = value,
                    // A description that binds <input>.power converts by specular power (checked on parse).
                    ScalarField::Power => input.convert = Convert::SpecularPower { power: value },
                }
            }
        }
        Ok(true)
    }
}

/// The range of a 0-to-1 field.
const UNIT: (f32, f32) = (0.0, 1.0);
/// The range of a field that only has to be a sane number.
const ANY: (f32, f32) = (-1.0e6, 1.0e6);
/// The range of a field that cannot be negative.
const POSITIVE: (f32, f32) = (0.0, 1.0e6);

/// A table's keys, each to be taken once; what is left over at the end is an unknown key.
struct Fields<'a> {
    table: &'a Table,
    taken: Vec<bool>,
}

impl<'a> Fields<'a> {
    fn new(table: &'a Table) -> Self {
        Self { table, taken: vec![false; table.entries.len()] }
    }

    fn take(&mut self, key: &str) -> Option<&'a Entry> {
        let i = self.table.entries.iter().position(|e| e.key == key)?;
        self.taken[i] = true;
        Some(&self.table.entries[i])
    }

    fn number(&mut self, key: &str, range: (f32, f32)) -> Result<Option<f32>, String> {
        self.take(key).map(|e| number(e, &e.value, range)).transpose()
    }

    fn text(&mut self, key: &str) -> Result<Option<(&'a str, usize)>, String> {
        self.take(key).map(|e| text(e).map(|t| (t, e.line))).transpose()
    }

    fn flag(&mut self, key: &str) -> Result<Option<bool>, String> {
        self.take(key)
            .map(|e| match e.value {
                Value::Bool(b) => Ok(b),
                _ => Err(wrong(e, "true or false")),
            })
            .transpose()
    }

    fn colour(&mut self, key: &str, range: (f32, f32)) -> Result<Option<[f32; 3]>, String> {
        let Some(e) = self.take(key) else { return Ok(None) };
        match &e.value {
            Value::List(items) if items.len() == 3 => Ok(Some([number(e, &items[0], range)?, number(e, &items[1], range)?, number(e, &items[2], range)?])),
            _ => Err(wrong(e, "a list of 3 numbers [red, green, blue]")),
        }
    }

    /// The name of a texture slot under `key`.
    fn slot(&mut self, key: &str) -> Result<Option<String>, String> {
        let Some((name, line)) = self.text(key)? else { return Ok(None) };
        if is_name(name) { Ok(Some(name.to_owned())) } else { Err(format!("line {line}: {key} = {name:?} must be a texture slot name (letters, digits, '_')")) }
    }

    fn finish(self) -> Result<(), String> {
        match self.table.entries.iter().zip(&self.taken).find(|(_, taken)| !**taken) {
            Some((e, _)) if self.table.name.is_empty() => Err(format!("line {}: unknown key {:?}", e.line, e.key)),
            Some((e, _)) => Err(format!("line {}: unknown key {:?} in [{}]", e.line, e.key, self.table.name)),
            None => Ok(()),
        }
    }
}

fn wrong(e: &Entry, want: &str) -> String {
    format!("line {}: {} must be {want}, not {}", e.line, e.key, e.value.kind())
}

fn number(e: &Entry, value: &Value, (min, max): (f32, f32)) -> Result<f32, String> {
    match value {
        Value::Number(v) if (min as f64..=max as f64).contains(v) => Ok(*v as f32),
        Value::Number(v) => Err(format!("line {}: {} = {v} is outside {min} to {max}", e.line, e.key)),
        _ => Err(wrong(e, "a number")),
    }
}

fn text(e: &Entry) -> Result<&str, String> {
    match &e.value {
        Value::Text(t) => Ok(t),
        _ => Err(wrong(e, "a string")),
    }
}

/// A slot, source, constant or parameter name: one or more of letters, digits and `_`.
fn is_name(name: &str) -> bool {
    !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

impl TextureChannel {
    /// Reads `<slot>.<channel>`, e.g. `gloss.a`.
    fn parse(text: &str) -> Result<Self, String> {
        let channel = |c: &str| match c {
            "r" => Some(Channel::R),
            "g" => Some(Channel::G),
            "b" => Some(Channel::B),
            "a" => Some(Channel::A),
            _ => None,
        };
        match text.split_once('.').and_then(|(slot, c)| Some((slot, channel(c)?))) {
            Some((slot, channel)) if is_name(slot) => Ok(Self { slot: slot.to_owned(), channel }),
            _ => Err(format!("texture = {text:?} must be <slot>.r, .g, .b or .a")),
        }
    }
}

impl ConstantTarget {
    /// Reads `<input>.<field>`, `normal.scale` or `param.<name>`.
    fn parse(text: &str) -> Option<Self> {
        let (table, field) = text.split_once('.')?;
        match (table, field) {
            ("param", name) => is_name(name).then(|| Self::Param(name.to_owned())),
            ("normal", "scale") => Some(Self::NormalScale),
            _ => {
                let field = match field {
                    "value" => ScalarField::Value,
                    "scale" => ScalarField::Scale,
                    "bias" => ScalarField::Bias,
                    "power" => ScalarField::Power,
                    _ => return None,
                };
                Some(Self::Scalar(ScalarSlot::from_name(table)?, field))
            }
        }
    }
}

fn colour_input(f: &mut Fields<'_>, input: &mut ColourInput, range: (f32, f32)) -> Result<(), String> {
    if let Some(value) = f.colour("value", range)? {
        input.value = value;
    }
    if let Some(slot) = f.slot("texture")? {
        input.texture = Some(slot);
    }
    Ok(())
}

fn scalar_input(f: &mut Fields<'_>, input: &mut ScalarInput) -> Result<(), String> {
    if let Some(value) = f.number("value", UNIT)? {
        input.value = value;
    }
    if let Some((channel, line)) = f.text("texture")? {
        input.texture = Some(TextureChannel::parse(channel).map_err(|e| format!("line {line}: {e}"))?);
    }
    let power = f.take("power").map(|e| number(e, &e.value, POSITIVE).map(|p| (p, e.line))).transpose()?;
    let convert = f.text("convert")?;
    input.convert = match (convert, power) {
        (None | Some(("none", _)), None) => Convert::None,
        (Some(("one_minus", _)), None) => Convert::OneMinus,
        (Some(("specular_power", _)), Some((power, _))) => Convert::SpecularPower { power },
        (Some(("specular_power", line)), None) => return Err(format!("line {line}: convert = \"specular_power\" needs `power`, the exponent a sample of 1.0 stands for")),
        (Some(("none" | "one_minus", _)) | None, Some((_, line))) => return Err(format!("line {line}: `power` is only for convert = \"specular_power\"")),
        (Some((other, line)), _) => return Err(format!("line {line}: convert = {other:?} must be \"none\", \"one_minus\" or \"specular_power\"")),
    };
    let scale = f.take("scale");
    let bias = f.take("bias");
    if let Some(e) = scale {
        input.scale = number(e, &e.value, ANY)?;
    }
    if let Some(e) = bias {
        input.bias = number(e, &e.value, ANY)?;
    }
    if input.texture.is_none() {
        let unused = convert.map(|(_, line)| ("convert", line)).or(scale.map(|e| ("scale", e.line))).or(bias.map(|e| ("bias", e.line)));
        if let Some((key, line)) = unused {
            return Err(format!("line {line}: `{key}` does nothing without a `texture`"));
        }
    }
    Ok(())
}

/// Parses one description file's text; `key` becomes [`MaterialDescription::key`]. An error
/// names the first bad line (1-based) and what is wrong with it.
pub fn parse_material(key: &str, file_text: &str) -> Result<MaterialDescription, String> {
    let mut m = MaterialDescription::new(key);
    // (line, constant name): the bindings whose target is checked once every table is read.
    let mut bound: Vec<(usize, String)> = Vec::new();
    for table in &toml_subset::parse(file_text)? {
        let mut f = Fields::new(table);
        match table.name.as_str() {
            "" => {
                if let Some((path, line)) = f.text("shader")? {
                    if path.len() <= ".wgsl".len() || !path.to_ascii_lowercase().ends_with(".wgsl") {
                        return Err(format!("line {line}: shader = {path:?} must be the game-file path of a .wgsl file"));
                    }
                    m.shader = Some(path.to_owned());
                }
            }
            "applies" => {
                let source = match f.text("source")? {
                    Some((source, _)) if is_name(source) => source.to_owned(),
                    Some((source, line)) => return Err(format!("line {line}: source = {source:?} must be a mesh kind's name (letters, digits, '_')")),
                    None => return Err(format!("line {}: [applies] needs `source`, the kind of mesh", table.line)),
                };
                let path = f.text("path")?.map(|(path, _)| normalize_path(path)).unwrap_or_default();
                m.applies = Some(Applies { source, path });
            }
            "alpha" => {
                match f.text("mode")? {
                    None => {}
                    Some(("opaque", _)) => m.alpha.mode = AlphaMode::Opaque,
                    Some(("mask", _)) => m.alpha.mode = AlphaMode::Mask,
                    Some(("blend", _)) => m.alpha.mode = AlphaMode::Blend,
                    Some((other, line)) => return Err(format!("line {line}: mode = {other:?} must be \"opaque\", \"mask\" or \"blend\"")),
                }
                if let Some(cutoff) = f.number("cutoff", UNIT)? {
                    m.alpha.cutoff = cutoff;
                }
                if let Some(two_sided) = f.flag("two_sided")? {
                    m.alpha.two_sided = two_sided;
                }
                if let Some(source_flags) = f.flag("source_flags")? {
                    m.alpha.source_flags = source_flags;
                }
            }
            "base_colour" => colour_input(&mut f, &mut m.base_colour, UNIT)?,
            "specular_tint" => colour_input(&mut f, &mut m.specular_tint, UNIT)?,
            "emissive" => colour_input(&mut f, &mut m.emissive, POSITIVE)?,
            "normal" => {
                let Some(texture) = f.slot("texture")? else {
                    return Err(format!("line {}: [normal] needs `texture`, the normal map's slot", table.line));
                };
                let encoding = match f.text("encoding")? {
                    None | Some(("rgb", _)) => NormalEncoding::Rgb,
                    Some(("ag", _)) => NormalEncoding::Ag,
                    Some(("rg", _)) => NormalEncoding::Rg,
                    Some((other, line)) => return Err(format!("line {line}: encoding = {other:?} must be \"rgb\", \"ag\" or \"rg\"")),
                };
                m.normal = Some(NormalInput { texture, encoding, flip_green: f.flag("flip_green")?.unwrap_or(false), scale: f.number("scale", ANY)?.unwrap_or(1.0) });
            }
            "faction_colour" => {
                let Some(texture) = f.slot("texture")? else {
                    return Err(format!("line {}: [faction_colour] needs `texture`, the colour mask's slot", table.line));
                };
                m.faction_colour = Some(FactionColour { texture });
            }
            "baked" => {
                if let Some(gain) = f.number("base_colour_gain", POSITIVE)? {
                    m.baked.base_colour_gain = gain;
                }
                if let Some(occlusion) = f.number("occlusion", UNIT)? {
                    m.baked.occlusion = occlusion;
                }
            }
            "constants" => {
                for e in &table.entries {
                    let target = text(e)?;
                    let Some(target) = ConstantTarget::parse(target) else {
                        return Err(format!(
                            "line {}: {} = {target:?} must be <input>.value, .scale, .bias or .power (opacity, roughness, metalness, specular_level, occlusion), normal.scale or param.<name>",
                            e.line, e.key
                        ));
                    };
                    m.constants.push((e.key.clone(), target));
                    bound.push((e.line, e.key.clone()));
                }
                continue;
            }
            "params" => {
                for e in &table.entries {
                    m.params.push((e.key.clone(), number(e, &e.value, ANY)?));
                }
                continue;
            }
            name => match ScalarSlot::from_name(name) {
                Some(slot) => scalar_input(&mut f, m.scalar_mut(slot))?,
                None => return Err(format!("line {}: unknown table [{name}]", table.line)),
            },
        }
        f.finish()?;
    }
    // A binding to a field the description does not have would silently do nothing.
    for ((line, name), (_, target)) in bound.iter().zip(&m.constants) {
        let missing = match target {
            ConstantTarget::NormalScale => m.normal.is_none().then(|| "there is no [normal] table".to_owned()),
            ConstantTarget::Param(param) => (!m.params.iter().any(|(n, _)| n == param)).then(|| format!("[params] has no {param:?}")),
            ConstantTarget::Scalar(slot, ScalarField::Power) => {
                (!matches!(m.scalar(*slot).convert, Convert::SpecularPower { .. })).then(|| format!("[{}] does not convert by \"specular_power\"", slot.name()))
            }
            ConstantTarget::Scalar(slot, ScalarField::Scale | ScalarField::Bias) => m.scalar(*slot).texture.is_none().then(|| format!("[{}] has no texture", slot.name())),
            ConstantTarget::Scalar(_, ScalarField::Value) => None,
        };
        if let Some(missing) = missing {
            return Err(format!("line {line}: constant {name} is bound to a field that is not there: {missing}"));
        }
    }
    Ok(m)
}

/// Every material description of the game: ours ([`BUILT_IN`]) under the game files' own.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MaterialLibrary {
    by_key: BTreeMap<String, MaterialDescription>,
}

impl MaterialLibrary {
    /// Reads every description: [`BUILT_IN`], then each `materials/**.toml` of the game's files
    /// (the winning copy of each path, so a mod's file replaces a lower one's and ours). A file
    /// that cannot be read or is malformed is logged once and skipped; where it would have
    /// replaced a built-in description, the built-in stays in use. Loading never stops on one.
    pub fn load(files: &GameFiles<'_>) -> Self {
        let (library, problems) = Self::read(files);
        for problem in &problems {
            log::error!("Material description {problem}");
        }
        library
    }

    /// [`MaterialLibrary::load`] without the logging: the library and one line per problem (a bad
    /// file with its path and line, two descriptions that apply to the same meshes).
    pub fn read(files: &GameFiles<'_>) -> (Self, Vec<String>) {
        let mut by_key = BTreeMap::new();
        let mut problems = Vec::new();
        for (key, text) in BUILT_IN {
            match parse_material(key, text) {
                Ok(m) => {
                    by_key.insert((*key).to_owned(), m);
                }
                Err(e) => problems.push(format!("built in as {key}: {e}; skipped")),
            }
        }
        let prefix = normalize_path(&format!("{MATERIAL_FOLDER}/"));
        for path in files.vfs.list(&prefix) {
            let Some(key) = path[prefix.len()..].strip_suffix(MATERIAL_EXTENSION).filter(|k| !k.is_empty()) else { continue };
            let key = key.replace('\\', "/");
            let parsed = files
                .read(path)
                .and_then(|bytes| String::from_utf8(bytes).map_err(|_| "not UTF-8 text".to_owned()))
                .and_then(|text| parse_material(&key, &text));
            match parsed {
                Ok(m) => {
                    by_key.insert(key, m);
                }
                Err(e) if by_key.contains_key(&key) => problems.push(format!("{path}: {e}; the built-in {key} stays in use")),
                Err(e) => problems.push(format!("{path}: {e}; skipped")),
            }
        }
        let mut claimed: BTreeMap<(&str, &str), &str> = BTreeMap::new();
        for m in by_key.values() {
            let Some(applies) = &m.applies else { continue };
            if let Some(first) = claimed.get(&(applies.source.as_str(), applies.path.as_str())) {
                problems.push(format!("{}: applies to the same meshes ({} under {:?}) as {first}, which is used", m.key, applies.source, applies.path));
            } else {
                claimed.insert((applies.source.as_str(), applies.path.as_str()), m.key.as_str());
            }
        }
        (Self { by_key }, problems)
    }

    /// The description of that key.
    pub fn get(&self, key: &str) -> Option<&MaterialDescription> {
        self.by_key.get(key)
    }

    /// The description a mesh of kind `source` at game-file path `path` gets by default: the one
    /// whose `[applies]` names the source with the longest path prefix of `path` (of two with the
    /// same prefix, the first by key). `None` when no description applies: the caller then uses
    /// [`DEFAULT_MATERIAL`].
    pub fn for_source(&self, source: &str, path: &str) -> Option<&MaterialDescription> {
        let path = normalize_path(path);
        let mut best: Option<(&MaterialDescription, usize)> = None;
        for m in self.by_key.values() {
            let Some(applies) = m.applies.as_ref().filter(|a| a.source == source && path.starts_with(&a.path)) else { continue };
            if best.is_none_or(|(_, len)| applies.path.len() > len) {
                best = Some((m, applies.path.len()));
            }
        }
        best.map(|(m, _)| m)
    }

    /// Every description, by key.
    pub fn iter(&self) -> impl Iterator<Item = &MaterialDescription> {
        self.by_key.values()
    }

    /// How many descriptions there are.
    pub fn len(&self) -> usize {
        self.by_key.len()
    }

    /// True when there is none.
    pub fn is_empty(&self) -> bool {
        self.by_key.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pack::test_util::temp_dir;
    use crate::pack::{LayerKind, Vfs};

    fn channel(slot: &str, channel: Channel) -> Option<TextureChannel> {
        Some(TextureChannel { slot: slot.to_owned(), channel })
    }

    /// Writes `text` at `dir/materials/<name>`.
    fn write(dir: &std::path::Path, name: &str, text: impl AsRef<[u8]>) {
        let path = dir.join(MATERIAL_FOLDER).join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    /// Our own descriptions parse, and the vanilla translation says what DESIGN.md §3.7.2 says.
    #[test]
    fn the_built_in_descriptions_parse() {
        let vfs = Vfs::new();
        let (library, problems) = MaterialLibrary::read(&GameFiles { vfs: &vfs });
        assert_eq!(problems, Vec::<String>::new());
        assert_eq!(library.iter().map(|m| m.key.as_str()).collect::<Vec<_>>(), BUILT_IN.iter().map(|(k, _)| *k).collect::<Vec<_>>());
        assert!(library.get(DEFAULT_MATERIAL).is_some_and(|m| m.applies.is_none()));

        let rigid = library.for_source("rigid_model", "rigidmodels/buildings/barn.rigid_model").expect("rigid models have a description");
        assert_eq!(rigid.key, "vanilla/rigid_model");
        assert!(rigid.alpha.source_flags, "the original's file-name flags decide");
        assert_eq!(rigid.base_colour.texture.as_deref(), Some("diffuse"));
        assert_eq!(rigid.opacity.texture, channel("diffuse", Channel::A));
        assert_eq!(rigid.roughness.texture, channel("gloss", Channel::A));
        assert!(matches!(rigid.roughness.convert, Convert::SpecularPower { .. }));
        assert_eq!(rigid.metalness, ScalarInput::constant(0.0), "the original has no metalness");
        assert_eq!(rigid.specular_tint.texture.as_deref(), Some("gloss"));
        assert!(rigid.faction_colour.is_none());
        assert!(rigid.constants.contains(&("specpower".to_owned(), ConstantTarget::Scalar(ScalarSlot::Roughness, ScalarField::Power))));
        assert!(rigid.constants.contains(&("bumpfactor".to_owned(), ConstantTarget::NormalScale)));

        let part = library.for_source("unit_variant", "unitmodels/textures/x").expect("unit parts have a description");
        assert_eq!(part.key, "vanilla/unit_part");
        assert_eq!(part.alpha.mode, AlphaMode::Mask);
        assert_eq!(part.faction_colour, Some(FactionColour { texture: "colour_mask".to_owned() }));
        assert_eq!(library.for_source("weighted_mesh", "").map(|m| m.key.as_str()), Some("vanilla/mount"));
        assert!(library.for_source("gltf", "models/tank.gltf").is_none(), "nothing applies: the caller takes the default");
    }

    /// A file that sets every field reads into exactly those values.
    #[test]
    fn parses_every_field() {
        let text = r#"
shader = "Shaders/Wet_Stone.wgsl"
[applies]
source = "rigid_model"
path = "RigidModels/Buildings/"
[alpha]
mode = "blend"
cutoff = 0.25
two_sided = true
source_flags = false
[base_colour]
texture = "diffuse"
value = [0.5, 0.25, 1]
[opacity]
texture = "diffuse.a"
[normal]
texture = "bump"
encoding = "ag"
flip_green = true
scale = 2.0
[roughness]
value = 0.9
texture = "gloss.a"
convert = "specular_power"
power = 64
scale = 0.5
bias = 0.1
[metalness]
value = 1.0
[specular_level]
texture = "gloss.r"
convert = "one_minus"
[specular_tint]
texture = "gloss"
[occlusion]
texture = "ao.g"
convert = "none"
[emissive]
value = [4.0, 2.0, 0.0]
texture = "glow"
[faction_colour]
texture = "mask"
[baked]
base_colour_gain = 1.2
occlusion = 0.6
[constants]
specpower = "roughness.power"
bumpfactor = "normal.scale"
wet = "param.wetness"
[params]
wetness = 0.5
"#;
        let m = parse_material("my_mod/wet_stone", text).expect("parses");
        let expected = MaterialDescription {
            key: "my_mod/wet_stone".to_owned(),
            applies: Some(Applies { source: "rigid_model".to_owned(), path: "rigidmodels\\buildings\\".to_owned() }),
            alpha: Alpha { mode: AlphaMode::Blend, cutoff: 0.25, two_sided: true, source_flags: false },
            base_colour: ColourInput { value: [0.5, 0.25, 1.0], texture: Some("diffuse".to_owned()) },
            opacity: ScalarInput { texture: channel("diffuse", Channel::A), ..ScalarInput::constant(1.0) },
            normal: Some(NormalInput { texture: "bump".to_owned(), encoding: NormalEncoding::Ag, flip_green: true, scale: 2.0 }),
            roughness: ScalarInput { value: 0.9, texture: channel("gloss", Channel::A), convert: Convert::SpecularPower { power: 64.0 }, scale: 0.5, bias: 0.1 },
            metalness: ScalarInput::constant(1.0),
            specular_level: ScalarInput { texture: channel("gloss", Channel::R), convert: Convert::OneMinus, ..ScalarInput::constant(0.5) },
            specular_tint: ColourInput { value: [1.0; 3], texture: Some("gloss".to_owned()) },
            occlusion: ScalarInput { texture: channel("ao", Channel::G), ..ScalarInput::constant(1.0) },
            emissive: ColourInput { value: [4.0, 2.0, 0.0], texture: Some("glow".to_owned()) },
            faction_colour: Some(FactionColour { texture: "mask".to_owned() }),
            baked: BakedShading { base_colour_gain: 1.2, occlusion: 0.6 },
            constants: vec![
                ("specpower".to_owned(), ConstantTarget::Scalar(ScalarSlot::Roughness, ScalarField::Power)),
                ("bumpfactor".to_owned(), ConstantTarget::NormalScale),
                ("wet".to_owned(), ConstantTarget::Param("wetness".to_owned())),
            ],
            params: vec![("wetness".to_owned(), 0.5)],
            shader: Some("Shaders/Wet_Stone.wgsl".to_owned()),
        };
        assert_eq!(m, expected);
        assert_eq!(parse_material("empty", "# nothing set\n").expect("empty"), MaterialDescription::new("empty"));
    }

    /// Each mistake is refused with its line: unknown tables and keys (a misspelt key must not
    /// silently do nothing), wrong types, values out of range, fields that would have no effect.
    #[test]
    fn refuses_bad_files() {
        for (text, line) in [
            ("[roughnes]\nvalue = 0.5\n", "line 1"),
            ("[roughness]\nvalu = 0.5\n", "line 2"),
            ("shadr = \"a.wgsl\"\n", "line 1"),
            ("shader = \"a.glsl\"\n", "line 1"),
            ("shader = \".wgsl\"\n", "line 1"),
            ("[roughness]\nvalue = 1.5\n", "line 2"),
            ("[roughness]\nvalue = \"rough\"\n", "line 2"),
            ("[roughness]\ntexture = \"gloss\"\n", "line 2"),
            ("[roughness]\ntexture = \"gloss.x\"\n", "line 2"),
            ("[roughness]\ntexture = \"gloss.a\"\nconvert = \"specular_power\"\n", "line 3"),
            ("[roughness]\ntexture = \"gloss.a\"\nconvert = \"one_minus\"\npower = 8\n", "line 4"),
            ("[roughness]\ntexture = \"gloss.a\"\nconvert = \"gamma\"\n", "line 3"),
            ("[roughness]\nvalue = 0.5\nscale = 2\n", "line 3"),
            ("[roughness]\nconvert = \"one_minus\"\n", "line 2"),
            ("[base_colour]\nvalue = [1, 1]\n", "line 2"),
            ("[base_colour]\nvalue = [1, 1, 2]\n", "line 2"),
            ("[base_colour]\ntexture = \"diffuse.r\"\n", "line 2"),
            ("[alpha]\nmode = \"cutout\"\n", "line 2"),
            ("[alpha]\ntwo_sided = \"yes\"\n", "line 2"),
            ("[alpha]\ncutoff = -0.1\n", "line 2"),
            ("\n[applies]\npath = \"x/\"\n", "line 2"),
            ("[applies]\nsource = \"rigid model\"\n", "line 2"),
            ("[normal]\nscale = 1\n", "line 1"),
            ("[normal]\ntexture = \"n\"\nencoding = \"xyz\"\n", "line 3"),
            ("[faction_colour]\n", "line 1"),
            ("[baked]\nocclusion = 2\n", "line 2"),
            ("[constants]\nspecpower = \"roughness.shine\"\n", "line 2"),
            ("[constants]\nspecpower = \"gloss.power\"\n", "line 2"),
            ("[constants]\nspecpower = 3\n", "line 2"),
            ("[constants]\nspecpower = \"roughness.power\"\n", "line 2"),
            ("[constants]\nbumpfactor = \"normal.scale\"\n", "line 2"),
            ("[constants]\nwet = \"param.wetness\"\n", "line 2"),
            ("[constants]\ns = \"roughness.scale\"\n[roughness]\nvalue = 0.5\n", "line 2"),
            ("[params]\nwetness = \"high\"\n", "line 2"),
            ("[roughness]\nvalue = 0.5\n[roughness]\nvalue = 0.6\n", "line 3"),
        ] {
            let e = parse_material("bad", text).expect_err(text);
            assert!(e.starts_with(&format!("{line}:")), "{text:?}: {e}");
        }
    }

    /// The conversions: a specular exponent becomes the roughness of the same highlight width, a
    /// glossiness is turned over, scale and bias follow and the result stays within 0 to 1; a
    /// mesh without the texture gets `value`.
    #[test]
    fn a_sample_becomes_the_input_value() {
        let gloss = |convert, scale, bias| ScalarInput { value: 0.8, texture: channel("gloss", Channel::A), convert, scale, bias };
        let from_power = gloss(Convert::SpecularPower { power: 30.0 }, 1.0, 0.0);
        assert_eq!(from_power.resolve(None), 0.8);
        assert_eq!(from_power.resolve(Some(0.0)), 1.0, "exponent 0: fully rough");
        assert!((from_power.resolve(Some(1.0)) - 0.5).abs() < 1e-6, "exponent 30: (2/32)^(1/4) = 0.5");
        assert!(from_power.resolve(Some(0.5)) > from_power.resolve(Some(1.0)), "a lower exponent is rougher");
        assert_eq!(gloss(Convert::OneMinus, 1.0, 0.0).resolve(Some(0.25)), 0.75);
        assert!((gloss(Convert::None, 2.0, 0.1).resolve(Some(0.25)) - 0.6).abs() < 1e-6);
        assert_eq!(gloss(Convert::None, 2.0, 0.5).resolve(Some(0.5)), 1.0, "kept within 0 to 1");
        assert_eq!(gloss(Convert::None, -1.0, 0.0).resolve(Some(0.5)), 0.0, "kept within 0 to 1");
    }

    /// A mesh's own constants replace the fields `[constants]` binds them to; an unbound name is
    /// left alone, and a value the field cannot hold is refused without changing the description.
    #[test]
    fn a_mesh_constant_replaces_its_bound_field() {
        let text = "[normal]\ntexture = \"normal\"\n[roughness]\ntexture = \"gloss.a\"\nconvert = \"specular_power\"\npower = 32\n\
                    [constants]\nspecpower = \"roughness.power\"\nbumpfactor = \"normal.scale\"\nshine = \"specular_level.value\"\nwet = \"param.wetness\"\n[params]\nwetness = 0.5\n";
        let mut m = parse_material("m", text).expect("parses");
        assert_eq!(m.apply_constant("specpower", 12.0), Ok(true));
        assert_eq!(m.roughness.convert, Convert::SpecularPower { power: 12.0 });
        assert_eq!(m.apply_constant("bumpfactor", 0.5), Ok(true));
        assert_eq!(m.normal.as_ref().map(|n| n.scale), Some(0.5));
        assert_eq!(m.apply_constant("shine", 0.25), Ok(true));
        assert_eq!(m.specular_level.value, 0.25);
        assert_eq!(m.apply_constant("wet", 0.9), Ok(true));
        assert_eq!(m.params, [("wetness".to_owned(), 0.9)]);
        assert_eq!(m.apply_constant("rimpower", 3.0), Ok(false), "not bound: no use here");
        let before = m.clone();
        assert!(m.apply_constant("specpower", f32::NAN).is_err());
        assert!(m.apply_constant("specpower", -1.0).is_err());
        assert!(m.apply_constant("shine", 1.5).is_err());
        assert_eq!(m, before, "a refused constant changes nothing");
    }

    /// A mod's folder replaces a description of the same path (a lower layer's file and ours) and
    /// adds its own; the most specific `[applies]` path wins for a mesh.
    #[test]
    fn a_mod_folder_overrides_and_adds_descriptions() {
        let dir = temp_dir("material_mod_override");
        let (data, mods) = (dir.join("data"), dir.join("mods"));
        write(&data, "vanilla/rigid_model.toml", "[applies]\nsource = \"rigid_model\"\n[roughness]\nvalue = 0.2\n");
        write(&mods, "Vanilla/Rigid_Model.toml", "[applies]\nsource = \"rigid_model\"\n[roughness]\nvalue = 0.7\n");
        write(&mods, "my_mod/wet_stone.toml", "[applies]\nsource = \"rigid_model\"\npath = \"rigidmodels/buildings/\"\n[roughness]\nvalue = 0.1\n");
        write(&mods, "my_mod/wet_stone.wgsl", "// a shader next to its description is not a description");
        let mut vfs = Vfs::new();
        vfs.mount_dir(&data, LayerKind::Loose).unwrap();
        vfs.mount_dir(&mods, LayerKind::ModsFolder).unwrap();
        let (library, problems) = MaterialLibrary::read(&GameFiles { vfs: &vfs });
        assert_eq!(problems, Vec::<String>::new());
        assert_eq!(library.len(), BUILT_IN.len() + 1);
        assert_eq!(library.get("vanilla/rigid_model").map(|m| m.roughness.value), Some(0.7), "the mod's file replaces the lower layer's and ours");
        assert_eq!(library.get("my_mod/wet_stone").map(|m| m.roughness.value), Some(0.1), "the mod's own description is added");
        assert_eq!(library.get("vanilla/unit_part"), parse_material("vanilla/unit_part", BUILT_IN[3].1).ok().as_ref(), "ours stays where no file replaces it");
        assert_eq!(library.for_source("rigid_model", "RigidModels/Buildings/barn.rigid_model").map(|m| m.key.as_str()), Some("my_mod/wet_stone"));
        assert_eq!(library.for_source("rigid_model", "rigidmodels/ships/brig.rigid_model").map(|m| m.key.as_str()), Some("vanilla/rigid_model"));
    }

    /// A bad file is reported once, with its path and line, and does not stop the load: the other
    /// files are read, and a built-in description a bad file would have replaced stays in use.
    #[test]
    fn a_bad_file_is_reported_once_and_loading_goes_on() {
        let dir = temp_dir("material_bad_file");
        write(&dir, "broken.toml", "[roughness]\nvalue = 0.5\nvalu = 1\n");
        write(&dir, "vanilla/unit_part.toml", "[alpha\n");
        write(&dir, "not_text.toml", [0xFF_u8, 0xFE, 0x00]);
        write(&dir, "good.toml", "[roughness]\nvalue = 0.3\n");
        write(&dir, "zz_clash.toml", "[applies]\nsource = \"rigid_model\"\n");
        let mut vfs = Vfs::new();
        vfs.mount_dir(&dir, LayerKind::ModsFolder).unwrap();
        let (library, problems) = MaterialLibrary::read(&GameFiles { vfs: &vfs });
        assert_eq!(problems.len(), 4, "{problems:#?}");
        let about = |name: &str| problems.iter().filter(|p| p.contains(name)).collect::<Vec<_>>();
        assert!(matches!(about("broken.toml")[..], [p] if p.contains("line 3") && p.ends_with("skipped")), "{problems:#?}");
        assert!(matches!(about("unit_part.toml")[..], [p] if p.contains("line 1") && p.ends_with("the built-in vanilla/unit_part stays in use")), "{problems:#?}");
        assert!(matches!(about("not_text.toml")[..], [p] if p.contains("not UTF-8")), "{problems:#?}");
        assert!(matches!(about("zz_clash")[..], [p] if p.contains("vanilla/rigid_model")), "{problems:#?}");
        assert!(library.get("broken").is_none() && library.get("not_text").is_none());
        assert_eq!(library.get("good").map(|m| m.roughness.value), Some(0.3));
        assert_eq!(library.get("vanilla/unit_part"), parse_material("vanilla/unit_part", BUILT_IN[3].1).ok().as_ref());
        assert_eq!(library.for_source("rigid_model", "x").map(|m| m.key.as_str()), Some("vanilla/rigid_model"), "of two that apply alike, the first by key");
    }
}
