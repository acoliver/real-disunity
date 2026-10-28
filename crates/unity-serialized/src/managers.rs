//! Layouts of the project-wide managers a player loads at startup: `BuildSettings`,
//! `TagManager`, `ResourceManager` and `PreloadData`.

use crate::{
    ClassId, SerializedError,
    object::{Layout, ObjectReader, PPTR_LEN, PPtr},
};

/// The length prefix of an empty string or array.
const LENGTH_MIN: usize = 4;

fn strings(r: &mut ObjectReader<'_>, what: &'static str) -> Result<Vec<String>, SerializedError> {
    r.array(what, LENGTH_MIN, ObjectReader::string)
}

/// The build's licence and feature flags, in the order Unity stores them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BuildFlag {
    HasProVersion,
    IsNoWatermarkBuild,
    IsPrototypingBuild,
    IsEducationalBuild,
    IsEmbedded,
    IsTrial,
    HasPublishingRights,
    HasShadows,
    HasSoftShadows,
    HasLocalLightShadows,
    HasAdvancedVersion,
    EnableDynamicBatching,
    UsesOnMouseEvents,
    HasClusterRendering,
}

const BUILD_FLAG_COUNT: usize = 14;

/// The values of every [`BuildFlag`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BuildFlags([bool; BUILD_FLAG_COUNT]);

impl BuildFlags {
    #[must_use]
    pub const fn get(&self, flag: BuildFlag) -> bool {
        self.0[flag as usize]
    }

    /// The flags in Unity's order, as [`BuildFlag`] lists them.
    #[must_use]
    pub const fn new(values: [bool; BUILD_FLAG_COUNT]) -> Self {
        Self(values)
    }
}

/// The scenes in the build and the build's settings.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BuildSettings {
    /// Scene paths in build order; scene `i` is stored in the file `level{i}`.
    pub scenes: Vec<String>,
    pub preloaded_plugins: Vec<String>,
    pub enabled_vr_devices: Vec<String>,
    pub build_tags: Vec<String>,
    pub flags: BuildFlags,
    /// The Unity version that built the player, for example `6000.3.15f1`.
    pub version: String,
    pub auth_token: String,
    /// Unity's `GraphicsDeviceType` values the player may use, in preference order.
    pub graphics_apis: Vec<i32>,
}

impl Layout for BuildSettings {
    const CLASS: ClassId = ClassId::BUILD_SETTINGS;

    fn read(r: &mut ObjectReader<'_>) -> Result<Self, SerializedError> {
        let scenes = strings(r, "scene path")?;
        let preloaded_plugins = strings(r, "preloaded plugin")?;
        let enabled_vr_devices = strings(r, "VR device")?;
        let build_tags = strings(r, "build tag")?;
        let mut flags = [false; BUILD_FLAG_COUNT];
        for flag in &mut flags {
            *flag = r.bool()?;
        }
        r.align()?;
        Ok(Self {
            scenes,
            preloaded_plugins,
            enabled_vr_devices,
            build_tags,
            flags: BuildFlags(flags),
            version: r.string()?,
            auth_token: r.string()?,
            graphics_apis: r.array("graphics API", 4, ObjectReader::i32)?,
        })
    }
}

/// A sorting layer: its name and the ID renderers refer to it by.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SortingLayer {
    pub name: String,
    pub unique_id: u32,
}

/// The project's tags, layers, sorting layers and rendering layers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TagManager {
    /// User tags; Unity's built-in tags such as `Untagged` and `Player` are not stored.
    pub tags: Vec<String>,
    /// Layer names by layer index, empty for unnamed layers.
    pub layers: Vec<String>,
    /// Sorting layers in drawing order.
    pub sorting_layers: Vec<SortingLayer>,
    pub rendering_layers: Vec<String>,
}

impl Layout for TagManager {
    const CLASS: ClassId = ClassId::TAG_MANAGER;

    fn read(r: &mut ObjectReader<'_>) -> Result<Self, SerializedError> {
        Ok(Self {
            tags: strings(r, "tag")?,
            layers: strings(r, "layer")?,
            sorting_layers: r.array("sorting layer", LENGTH_MIN + 4, |r| {
                Ok(SortingLayer {
                    name: r.string()?,
                    unique_id: r.u32()?,
                })
            })?,
            rendering_layers: strings(r, "rendering layer")?,
        })
    }
}

/// An asset and the assets it needs loaded with it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResourceDependency {
    pub object: PPtr,
    pub dependencies: Vec<PPtr>,
}

/// The assets `Resources.Load` can find, by path.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResourceManager {
    /// Lower-case resource paths without extensions, and the asset each names. A path may
    /// appear more than once, for assets of different types.
    pub container: Vec<(String, PPtr)>,
    pub dependent_assets: Vec<ResourceDependency>,
}

impl Layout for ResourceManager {
    const CLASS: ClassId = ClassId::RESOURCE_MANAGER;

    fn read(r: &mut ObjectReader<'_>) -> Result<Self, SerializedError> {
        Ok(Self {
            container: r.array("resource", LENGTH_MIN + PPTR_LEN, |r| {
                Ok((r.string()?, r.pptr()?))
            })?,
            dependent_assets: r.array("resource dependency", PPTR_LEN + LENGTH_MIN, |r| {
                Ok(ResourceDependency {
                    object: r.pptr()?,
                    dependencies: r.pptrs("resource dependency reference")?,
                })
            })?,
        })
    }
}

/// The assets a scene or asset bundle loads before it starts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PreloadData {
    pub name: String,
    pub assets: Vec<PPtr>,
    pub dependencies: Vec<String>,
    pub explicit_data_layout: bool,
}

impl Layout for PreloadData {
    const CLASS: ClassId = ClassId::PRELOAD_DATA;

    fn read(r: &mut ObjectReader<'_>) -> Result<Self, SerializedError> {
        Ok(Self {
            name: r.string()?,
            assets: r.pptrs("preloaded asset")?,
            dependencies: strings(r, "preload dependency")?,
            explicit_data_layout: r.bool()?,
        })
    }
}
