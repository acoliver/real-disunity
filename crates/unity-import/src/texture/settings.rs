use crate::yaml::{Mapping, Node, Scalar};

/// Texture sampling filter. Unity's importer default (`-1`) is bilinear.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Filter {
    Point,
    Bilinear,
    Trilinear,
}

/// Texture addressing. Unity's importer default (`-1`) is repeat.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Wrap {
    Repeat,
    Clamp,
    Mirror,
    MirrorOnce,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpriteMode {
    /// The texture is not a sprite source.
    None,
    /// The whole texture is one sprite.
    Single,
    /// The texture is a sheet of sprites listed in the `.meta` file.
    Multiple,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TextureSettings {
    pub sprite_mode: SpriteMode,
    pub pixels_per_unit: f64,
    pub filter: Filter,
    /// Horizontal and vertical addressing.
    pub wrap: [Wrap; 2],
    /// Whether texels are sRGB encoded. Linear textures hold data values.
    pub srgb: bool,
    pub max_size: u32,
    /// Whether Unity generates a physics shape for sprites without an authored one.
    pub generate_fallback_physics_shape: bool,
    /// Alignment and custom pivot of a single-mode sprite.
    pub(super) alignment: i64,
    pub(super) pivot: [f64; 2],
}

impl TextureSettings {
    pub(super) fn read(importer: &Mapping) -> Result<Self, String> {
        let texture_settings = importer.get("textureSettings");
        let setting = |key| texture_settings.and_then(|s| s.get(key));
        Ok(Self {
            sprite_mode: match int(importer.get("spriteMode"), 0)? {
                0 => SpriteMode::None,
                1 => SpriteMode::Single,
                2 => SpriteMode::Multiple,
                mode => return Err(format!("sprite mode {mode} is not supported")),
            },
            pixels_per_unit: positive(float(importer.get("spritePixelsToUnits"), 100.0)?)?,
            filter: match int(setting("filterMode"), -1)? {
                0 => Filter::Point,
                -1 | 1 => Filter::Bilinear,
                2 => Filter::Trilinear,
                mode => return Err(format!("filter mode {mode} is not supported")),
            },
            wrap: [wrap(setting("wrapU"))?, wrap(setting("wrapV"))?],
            srgb: int(
                importer.get("mipmaps").and_then(|m| m.get("sRGBTexture")),
                1,
            )? != 0,
            max_size: u32::try_from(int(importer.get("maxTextureSize"), 2048)?)
                .map_err(|_| "maxTextureSize is out of range".to_owned())?,
            generate_fallback_physics_shape: int(
                importer.get("spriteGenerateFallbackPhysicsShape"),
                1,
            )? != 0,
            alignment: int(importer.get("alignment"), 0)?,
            pivot: vector2(importer.get("spritePivot"), [0.5, 0.5])?,
        })
    }

    /// Unity downscales textures larger than `maxTextureSize`. The importer does not, so it
    /// rejects them instead of producing different texels.
    pub(super) fn check_size(&self, width: u32, height: u32) -> Result<(), String> {
        if width.max(height) > self.max_size {
            return Err(format!(
                "image is {width}x{height}; Unity downscales textures above maxTextureSize {}, which is not supported",
                self.max_size
            ));
        }
        Ok(())
    }
}

fn wrap(node: Option<&Node>) -> Result<Wrap, String> {
    match int(node, -1)? {
        -1 | 0 => Ok(Wrap::Repeat),
        1 => Ok(Wrap::Clamp),
        2 => Ok(Wrap::Mirror),
        3 => Ok(Wrap::MirrorOnce),
        mode => Err(format!("wrap mode {mode} is not supported")),
    }
}

fn scalar<'a>(node: Option<&'a Node>, what: &str) -> Result<Option<&'a Scalar>, String> {
    node.map(|node| {
        node.as_scalar()
            .ok_or_else(|| format!("expected a scalar for {what}"))
    })
    .transpose()
}

pub(super) fn int(node: Option<&Node>, default: i64) -> Result<i64, String> {
    scalar(node, "an integer setting")?.map_or(Ok(default), |s| {
        s.as_i64()
            .ok_or_else(|| format!("`{}` is not an integer", s.text()))
    })
}

pub(super) fn float(node: Option<&Node>, default: f64) -> Result<f64, String> {
    let value = scalar(node, "a number setting")?.map_or(Ok(default), |s| {
        s.as_f64()
            .ok_or_else(|| format!("`{}` is not a number", s.text()))
    })?;
    if value.is_finite() {
        Ok(value)
    } else {
        Err(format!("`{value}` is not finite"))
    }
}

pub(super) fn vector2(node: Option<&Node>, default: [f64; 2]) -> Result<[f64; 2], String> {
    let Some(node) = node else {
        return Ok(default);
    };
    Ok([
        float(node.get("x"), default[0])?,
        float(node.get("y"), default[1])?,
    ])
}

fn positive(value: f64) -> Result<f64, String> {
    if value > 0.0 {
        Ok(value)
    } else {
        Err(format!("pixels per unit {value} must be positive"))
    }
}
