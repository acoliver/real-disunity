use base64::{Engine, engine::general_purpose::STANDARD};
use glam::{Mat3, Mat4, Quat, Vec3};
use serde_json::Value;
use std::collections::BTreeMap;
use worldrunner::scene::{
    gpu::{SceneView, dynamic::DynamicQuad},
    plan::{SceneTexture, TexelFormat, TextureColorSpace},
};

const MAX_GLYPHS: usize = 4096;
const MAX_ALPHA_BYTES: usize = 4 * 1024 * 1024;
const MAX_CACHED_TEXTURES: usize = 1024;
const MAX_CACHED_RGBA: usize = 64 * 1024 * 1024;
const MAX_TABLE_BYTES: usize = 256 * 1024 * 1024;
const MAX_QUADS: usize = 65_536;

#[derive(Default)]
pub(crate) struct UiGlyphs {
    masks: BTreeMap<Mask, usize>,
    rgba_bytes: usize,
    table_len: usize,
}

#[derive(PartialEq, Eq, PartialOrd, Ord)]
struct Mask {
    width: u32,
    height: u32,
    alpha: Vec<u8>,
}

#[derive(Default)]
struct Pending {
    masks: BTreeMap<Mask, usize>,
    textures: Vec<SceneTexture>,
    quads: Vec<DynamicQuad>,
    alpha_bytes: usize,
    rgba_bytes: usize,
    table_bytes: usize,
}

impl UiGlyphs {
    pub(crate) fn append(
        &mut self,
        layout: &Value,
        snapshot: &Value,
        view: &SceneView,
        textures: &mut Vec<SceneTexture>,
        quads: &mut Vec<DynamicQuad>,
    ) -> Result<bool, String> {
        let texts = list(&layout["texts"])?;
        let count = texts.iter().try_fold(0_usize, |count, text| {
            Ok::<_, String>(count.saturating_add(list(&text["glyphs"])?.len()))
        })?;
        if count > MAX_GLYPHS || quads.len().saturating_add(count) > MAX_QUADS {
            return Err("UI glyph/quad budget exceeded".into());
        }
        if textures.len() < self.table_len {
            return Err("UI texture table was replaced; recreate UiGlyphs".into());
        }
        if count == 0 {
            return Ok(false);
        }
        validate_viewport(layout, view)?;
        let objects = object_index(snapshot)?;
        let mut pending = Pending {
            table_bytes: table_bytes(textures)?,
            ..Pending::default()
        };
        for text in texts {
            pending.text(text, &objects, view, self, textures.len())?;
        }
        let changed = !pending.textures.is_empty();
        self.rgba_bytes += pending.rgba_bytes;
        self.masks.extend(pending.masks);
        textures.extend(pending.textures);
        quads.extend(pending.quads);
        self.table_len = textures.len();
        Ok(changed)
    }
}

impl Pending {
    fn text(
        &mut self,
        text: &Value,
        objects: &BTreeMap<u64, &Value>,
        view: &SceneView,
        cache: &UiGlyphs,
        base: usize,
    ) -> Result<(), String> {
        let glyphs = list(&text["glyphs"])?;
        if glyphs.is_empty() {
            return Ok(());
        }
        let overlay = if canvas_mode(text, objects)? == "screen_space_overlay" {
            Some(Overlay::new(view)?)
        } else {
            None
        };
        let tint: [f32; 4] = decode(&text["color"])?;
        if !tint
            .iter()
            .all(|v| v.is_finite() && (0.0..=1.0).contains(v))
        {
            return Err("UI text color must be finite RGBA in 0..=1".into());
        }
        let sorting_layer = decode(&text["sorting_layer"])?;
        let sorting_order = decode(&text["sorting_order"])?;
        for glyph in glyphs {
            let mut quad = pose(corners(glyph, overlay.as_ref())?)?;
            let mask = Mask::decode(glyph, &mut self.alpha_bytes)?;
            quad.texture = Some(self.texture(mask, cache, base)?);
            quad.tint = tint;
            quad.sorting_layer = sorting_layer;
            quad.sorting_order = sorting_order;
            self.quads.push(quad);
        }
        Ok(())
    }

    fn texture(&mut self, mask: Mask, cache: &UiGlyphs, base: usize) -> Result<usize, String> {
        if let Some(index) = cache.masks.get(&mask).or_else(|| self.masks.get(&mask)) {
            return Ok(*index);
        }
        let bytes = mask.alpha.len() * 4;
        if cache.masks.len() + self.masks.len() >= MAX_CACHED_TEXTURES
            || cache.rgba_bytes + self.rgba_bytes + bytes > MAX_CACHED_RGBA
            || self.table_bytes + self.rgba_bytes + bytes > MAX_TABLE_BYTES
        {
            return Err("UI glyph texture cache/table budget exceeded".into());
        }
        let index = base + self.textures.len();
        self.textures.push(SceneTexture::Texels {
            format: TexelFormat::Rgba8Unorm,
            color_space: TextureColorSpace::Srgb,
            width: mask.width,
            height: mask.height,
            mips: 1,
            bytes: mask
                .alpha
                .iter()
                .flat_map(|a| [255, 255, 255, *a])
                .collect(),
        });
        self.masks.insert(mask, index);
        self.rgba_bytes += bytes;
        Ok(index)
    }
}

impl Mask {
    fn decode(glyph: &Value, total: &mut usize) -> Result<Self, String> {
        let width: u32 = decode(&glyph["width"])?;
        let height: u32 = decode(&glyph["height"])?;
        if width == 0 || height == 0 || width > 4096 || height > 4096 {
            return Err("UI mask dimensions must be in 1..=4096".into());
        }
        let count = usize::try_from(u64::from(width) * u64::from(height))
            .map_err(|error| error.to_string())?;
        *total = total.saturating_add(count);
        if *total > MAX_ALPHA_BYTES {
            return Err("UI decoded alpha mask budget exceeded".into());
        }
        let encoded = glyph["alpha_base64"]
            .as_str()
            .ok_or("UI mask has no base64 alpha")?;
        if encoded.len() != count.div_ceil(3) * 4 {
            return Err("UI mask base64 length differs from dimensions".into());
        }
        let alpha = STANDARD
            .decode(encoded)
            .map_err(|error| error.to_string())?;
        if alpha.len() != count {
            return Err("UI mask decoded length differs from dimensions".into());
        }
        Ok(Self {
            width,
            height,
            alpha,
        })
    }
}

fn pose(points: [Vec3; 4]) -> Result<DynamicQuad, String> {
    if !points.iter().all(|p| p.is_finite()) {
        return Err("UI corners must be finite".into());
    }
    let right = points[1] - points[0];
    let up = points[3] - points[0];
    let width = right.length();
    let height = up.length();
    if !width.is_finite() || !height.is_finite() || width <= 0. || height <= 0. {
        return Err("UI corners must have nonzero finite edges".into());
    }
    let x = right / width;
    let z = up / height;
    if x.dot(z).abs() > 0.0001
        || (points[2] - points[0] - right - up).length() > width.max(height) * 0.0001 + 0.00001
    {
        return Err("UI corners must describe an orthogonal parallelogram".into());
    }
    let rotation = Quat::from_mat3(&Mat3::from_cols(x, z.cross(x), z)).normalize();
    if !rotation.is_finite() {
        return Err("UI corner rotation is invalid".into());
    }
    Ok(DynamicQuad {
        position: points[0].to_array(),
        rotation: rotation.to_array(),
        rect: [0., 0., width, height],
        ..DynamicQuad::default()
    })
}

fn corners(glyph: &Value, overlay: Option<&Overlay>) -> Result<[Vec3; 4], String> {
    if let Some(overlay) = overlay {
        let screen: [[f32; 2]; 4] = decode(&glyph["screen_corners"])?;
        return Ok(screen.map(|p| overlay.point(p)));
    }
    let world: [[f32; 3]; 4] = decode(&glyph["world_corners"])?;
    Ok(world.map(|[x, y, z]| Vec3::new(x, z, y)))
}

struct Overlay {
    inverse: Mat4,
    pixels: [f32; 2],
    fraction: f32,
}
impl Overlay {
    fn new(view: &SceneView) -> Result<Self, String> {
        let inverse = view.view_projection_matrix().inverse();
        let near = inverse.project_point3(Vec3::ZERO);
        let far = inverse.project_point3(Vec3::Z);
        let range = (far - near).length();
        if !inverse.is_finite() || !range.is_finite() || range <= 0. {
            return Err("UI overlay requires an invertible finite camera view".into());
        }
        let viewport = view.viewport();
        let pixels = [viewport.width, viewport.height]
            .map(|v| u16::try_from(v).map(f32::from).map_err(|e| e.to_string()));
        Ok(Self {
            inverse,
            pixels: [pixels[0].clone()?, pixels[1].clone()?],
            fraction: (1. / range).min(0.5),
        })
    }
    fn point(&self, [x, y]: [f32; 2]) -> Vec3 {
        let ndc = Vec3::new(
            2. * x / self.pixels[0] - 1.,
            2. * y / self.pixels[1] - 1.,
            0.,
        );
        let near = self.inverse.project_point3(ndc);
        let far = self.inverse.project_point3(ndc.with_z(1.));
        near.lerp(far, self.fraction)
    }
}

fn canvas_mode<'a>(text: &Value, objects: &BTreeMap<u64, &'a Value>) -> Result<&'a str, String> {
    let mut handle = text["object"]
        .as_u64()
        .ok_or("UI text has no object handle")?;
    for _ in 0..objects.len() {
        let object = objects
            .get(&handle)
            .ok_or("UI Canvas ancestry object is absent")?;
        for component in list(&object["components"])? {
            if component["kind"] == "Canvas" {
                return match component["fields"]["render_mode"].as_str() {
                    Some(
                        mode @ ("screen_space_overlay" | "screen_space_camera" | "world_space"),
                    ) => Ok(mode),
                    _ => Err("UI Canvas render mode is invalid".into()),
                };
            }
        }
        handle = object["transform"]["parent"]
            .as_u64()
            .ok_or("UI text has no Canvas ancestor")?;
    }
    Err("UI Canvas ancestry contains a cycle".into())
}
fn object_index(snapshot: &Value) -> Result<BTreeMap<u64, &Value>, String> {
    list(&snapshot["objects"])?
        .iter()
        .map(|object| {
            Ok((
                object["handle"]
                    .as_u64()
                    .ok_or("native object has no handle")?,
                object,
            ))
        })
        .collect()
}
fn validate_viewport(layout: &Value, view: &SceneView) -> Result<(), String> {
    let native: [u32; 2] = decode(&layout["viewport"])?;
    let viewport = view.viewport();
    if native != [viewport.width, viewport.height] || native.iter().any(|v| *v == 0 || *v > 16384) {
        return Err("UI layout viewport differs from the current renderer viewport".into());
    }
    Ok(())
}
fn table_bytes(textures: &[SceneTexture]) -> Result<usize, String> {
    let bytes = textures.iter().fold(0_usize, |total, texture| {
        let (SceneTexture::Texels { bytes, .. } | SceneTexture::Encoded { bytes, .. }) = texture;
        total.saturating_add(bytes.len())
    });
    if bytes > MAX_TABLE_BYTES {
        return Err("UI texture table byte budget exceeded".into());
    }
    Ok(bytes)
}
fn list(value: &Value) -> Result<&[Value], String> {
    value
        .as_array()
        .map(Vec::as_slice)
        .ok_or_else(|| "native UI array is missing".into())
}
fn decode<T: serde::de::DeserializeOwned>(value: &Value) -> Result<T, String> {
    serde_json::from_value(value.clone()).map_err(|error| error.to_string())
}

#[cfg(test)]
#[path = "ui_tests.rs"]
mod tests;
