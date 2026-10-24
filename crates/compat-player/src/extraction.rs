mod pixels;
use serde_json::Value;
use std::{
    collections::BTreeMap,
    fs::File,
    io::{BufReader, Read},
    path::Path,
};
use worldrunner::compat_content::{AssetKind, ImportedScene, SpriteAsset};
use worldrunner::scene::{
    gpu::{CameraPose, OrthographicLens, SceneView, Viewport, dynamic::DynamicQuad},
    plan::{SceneTexture, TexelFormat, TextureColorSpace},
};

pub(crate) struct AssetTable {
    pub textures: Vec<SceneTexture>,
    sprites: BTreeMap<String, SpriteGeometry>,
}
#[derive(Clone)]
struct SpriteGeometry {
    texture: usize,
    rect: [f32; 4],
    uv: [f32; 4],
}

pub(crate) struct Contribution {
    pub view: SceneView,
    pub quads: Vec<DynamicQuad>,
    pub clear: [f64; 4],
}

impl AssetTable {
    pub fn load(root: &Path, graph: &ImportedScene) -> Result<Self, String> {
        let mut table = Self {
            textures: Vec::new(),
            sprites: BTreeMap::new(),
        };
        for asset in &graph.assets {
            if asset.kind != AssetKind::Texture || asset.sprites.is_empty() {
                continue;
            }
            let source_root = root.canonicalize().map_err(|error| error.to_string())?;
            let path = root
                .join(&asset.path)
                .canonicalize()
                .map_err(|error| error.to_string())?;
            if !path.starts_with(&source_root) {
                return Err("sprite texture escapes source root".into());
            }
            let (size, bytes) = decode_texture(&path)?;
            let index = table.textures.len();
            table.textures.push(texture(size, bytes));
            for sprite in &asset.sprites {
                let geometry = geometry(index, size, sprite)?;
                table
                    .sprites
                    .insert(format!("{}:{}", asset.guid, sprite.file_id), geometry);
            }
        }
        for fallback in &graph.fallback_sprites {
            let index = table.textures.len();
            image_size(fallback.texture_size)?;
            let bytes = fallback.rgba8().map_err(|error| error.to_string())?;
            table.textures.push(texture(fallback.texture_size, bytes));
            table.sprites.insert(
                fallback.name.clone(),
                geometry(index, fallback.texture_size, &fallback.sprite)?,
            );
        }
        Ok(table)
    }

    pub fn extract(&self, snapshot: &Value, viewport: Viewport) -> Result<Contribution, String> {
        let objects = snapshot["objects"]
            .as_array()
            .ok_or("native snapshot has no object array")?;
        let mut quads = Vec::new();
        let mut camera = None;
        for object in objects {
            if object["active_in_hierarchy"] != true {
                continue;
            }
            for component in object["components"]
                .as_array()
                .ok_or("native object has no components")?
            {
                if component["enabled"] != true {
                    continue;
                }
                match component["kind"].as_str() {
                    Some("Camera") => {
                        let depth = component["fields"]["depth"].as_f64().unwrap_or(0.0);
                        if camera.as_ref().is_none_or(|(prior, _, _)| depth > *prior) {
                            camera = Some((depth, object, component));
                        }
                    }
                    Some("SpriteRenderer") => {
                        if !component["fields"]["sprite"].is_null() {
                            quads.push(self.quad(object, component)?);
                        }
                    }
                    Some("Tilemap") => {
                        for (object, component) in crate::tiles::sprites(object, component)? {
                            quads.push(self.quad(&object, &component)?);
                        }
                    }
                    _ => {}
                }
            }
        }
        let (_, object, component) = camera.ok_or("admitted scene has no active camera")?;
        let (view, clear) = camera_view(object, component, viewport)?;
        Ok(Contribution { view, quads, clear })
    }
}

fn camera_view(
    object: &Value,
    component: &Value,
    viewport: Viewport,
) -> Result<(SceneView, [f64; 4]), String> {
    let fields = &component["fields"];
    if fields["orthographic"] != true {
        return Err("live compatibility perspective camera is not yet admitted".into());
    }
    let pose = &object["transform"];
    let eye: [f32; 3] = array(&pose["position"])?;
    let rotation: [f32; 4] = array(&pose["rotation"])?;
    let forward = rotate(rotation, [0., 0., 1.]);
    let up = canonical(rotate(rotation, [0., 1., 0.]));
    let eye = canonical(eye);
    let forward = canonical(forward);
    let target = std::array::from_fn(|index| eye[index] + forward[index]);
    let view = SceneView::orthographic_with_up(
        CameraPose {
            eye_millimeters: millimeters(eye)?,
            target_millimeters: millimeters(target)?,
        },
        OrthographicLens {
            vertical_half_extent_meters: scalar(&fields["orthographic_size"])?,
            near_meters: scalar(&fields["near"])?,
            far_meters: scalar(&fields["far"])?,
        },
        up,
        viewport,
    )
    .map_err(|error| error.to_string())?;
    Ok((view, array(&fields["background"])?))
}

impl AssetTable {
    fn quad(&self, object: &Value, component: &Value) -> Result<DynamicQuad, String> {
        let fields = &component["fields"];
        let reference = &fields["sprite"];
        let key = sprite_key(reference)
            .ok_or_else(|| format!("unresolved sprite reference {reference}"))?;
        let geometry = self
            .sprites
            .get(&key)
            .ok_or_else(|| format!("sprite {key} has no admitted texture/subasset"))?;
        let pose = &object["transform"];
        let rotation: [f32; 4] = array(&pose["rotation"])?;
        let mut uv = geometry.uv;
        if fields["flip_x"] == true {
            uv.swap(0, 2);
        }
        if fields["flip_y"] == true {
            uv.swap(1, 3);
        }
        Ok(DynamicQuad {
            position: canonical(array(&pose["position"])?),
            rotation: [-rotation[0], -rotation[2], -rotation[1], rotation[3]],
            scale: canonical(array(&pose["scale"])?),
            rect: geometry.rect,
            uv_rect: uv,
            tint: array(&fields["color"])?,
            texture: Some(geometry.texture),
            sorting_layer: serde_json::from_value(fields["sorting_layer"].clone())
                .map_err(|error| error.to_string())?,
            sorting_order: serde_json::from_value(fields["sorting_order"].clone())
                .map_err(|error| error.to_string())?,
        })
    }
}

pub(crate) fn sprite_key(reference: &Value) -> Option<String> {
    if let Some(builtin) = reference["$builtin"].as_str() {
        return Some(builtin.into());
    }
    if let Some(guid) = reference["guid"].as_str() {
        return Some(format!("{guid}:{}", reference["file_id"].as_i64()?));
    }
    reference["$ref"].as_str().map(str::to_owned)
}
fn canonical(value: [f32; 3]) -> [f32; 3] {
    [value[0], value[2], value[1]]
}
fn array<T: serde::de::DeserializeOwned>(value: &Value) -> Result<T, String> {
    serde_json::from_value(value.clone()).map_err(|error| error.to_string())
}
fn scalar(value: &Value) -> Result<f32, String> {
    array(value)
}
fn millimeters(value: [f32; 3]) -> Result<[i64; 3], String> {
    let converted = value.map(|number| {
        (f64::from(number) * 1000.)
            .round()
            .to_string()
            .parse::<i64>()
    });
    Ok([
        converted[0].clone().map_err(|error| error.to_string())?,
        converted[1].clone().map_err(|error| error.to_string())?,
        converted[2].clone().map_err(|error| error.to_string())?,
    ])
}
fn rotate(q: [f32; 4], v: [f32; 3]) -> [f32; 3] {
    let cross = |a: [f32; 3], b: [f32; 3]| {
        [
            a[1] * b[2] - a[2] * b[1],
            a[2] * b[0] - a[0] * b[2],
            a[0] * b[1] - a[1] * b[0],
        ]
    };
    let xyz = [q[0], q[1], q[2]];
    let t = cross(xyz, v).map(|number| number * 2.);
    let second = cross(xyz, t);
    std::array::from_fn(|index| v[index] + q[3] * t[index] + second[index])
}
fn geometry(
    texture: usize,
    size: [u32; 2],
    sprite: &SpriteAsset,
) -> Result<SpriteGeometry, String> {
    let [x, y, w, h] = sprite.rect;
    let ppu = sprite.pixels_per_unit;
    if !ppu.is_finite() || ppu <= 0. {
        return Err("sprite pixels per unit must be positive".into());
    }
    let [px, py] = sprite.pivot;
    Ok(SpriteGeometry {
        texture,
        rect: array(&serde_json::json!([
            -px * w / ppu,
            -py * h / ppu,
            (1. - px) * w / ppu,
            (1. - py) * h / ppu
        ]))?,
        uv: array(&serde_json::json!([
            x / f64::from(size[0]),
            1. - (y + h) / f64::from(size[1]),
            (x + w) / f64::from(size[0]),
            1. - y / f64::from(size[1])
        ]))?,
    })
}
fn texture(size: [u32; 2], bytes: Vec<u8>) -> SceneTexture {
    SceneTexture::Texels {
        format: TexelFormat::Rgba8Unorm,
        color_space: TextureColorSpace::Srgb,
        width: size[0],
        height: size[1],
        mips: 1,
        bytes: bytes.into_boxed_slice(),
    }
}
fn decode_png(path: &Path) -> Result<([u32; 2], Vec<u8>), String> {
    let file = File::open(path).map_err(|error| format!("{}: {error}", path.display()))?;
    let mut decoder = png::Decoder::new(BufReader::new(file));
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info().map_err(|error| error.to_string())?;
    image_size([reader.info().width, reader.info().height])?;
    let mut bytes = vec![
        0;
        reader
            .output_buffer_size()
            .ok_or("PNG output exceeds addressable memory")?
    ];
    let info = reader
        .next_frame(&mut bytes)
        .map_err(|error| error.to_string())?;
    bytes.truncate(info.buffer_size());
    let expected = image_size([info.width, info.height])?;
    let rgba = match info.color_type {
        png::ColorType::Rgba => bytes,
        png::ColorType::Rgb => pixels::expand_rgb(&bytes, expected)?,
        png::ColorType::Grayscale => pixels::expand_l8(&bytes, expected)?,
        png::ColorType::GrayscaleAlpha => pixels::expand_la8(&bytes, expected)?,
        png::ColorType::Indexed => {
            return Err("PNG palette expansion did not produce texels".into());
        }
    };
    Ok(([info.width, info.height], rgba))
}

pub(crate) fn image_size(size: [u32; 2]) -> Result<usize, String> {
    if size.iter().any(|side| *side == 0 || *side > 4096) {
        return Err("texture dimensions must be in 1..=4096".into());
    }
    usize::try_from(u64::from(size[0]) * u64::from(size[1]) * 4).map_err(|error| error.to_string())
}

fn decode_texture(path: &Path) -> Result<([u32; 2], Vec<u8>), String> {
    let mut file = File::open(path).map_err(|error| error.to_string())?;
    if file.metadata().map_err(|error| error.to_string())?.len() > 16_777_216 {
        return Err("encoded texture exceeds 16 MiB".into());
    }
    let mut signature = [0; 8];
    file.read_exact(&mut signature)
        .map_err(|error| error.to_string())?;
    if signature == *b"\x89PNG\r\n\x1a\n" {
        return decode_png(path);
    }
    if signature[..3] == [255, 216, 255] {
        return decode_jpeg(path);
    }
    Err("unsupported texture signature; expected PNG or JPEG".into())
}
fn decode_jpeg(path: &Path) -> Result<([u32; 2], Vec<u8>), String> {
    let file = File::open(path).map_err(|error| error.to_string())?;
    let mut decoder = jpeg_decoder::Decoder::new(BufReader::new(file));
    decoder.set_max_decoding_buffer_size(67_108_864);
    decoder.read_info().map_err(|error| error.to_string())?;
    let info = decoder.info().ok_or("JPEG metadata is absent")?;
    let size = [u32::from(info.width), u32::from(info.height)];
    let expected = image_size(size)?;
    if !matches!(
        info.pixel_format,
        jpeg_decoder::PixelFormat::RGB24 | jpeg_decoder::PixelFormat::L8
    ) {
        return Err(format!(
            "unsupported JPEG pixel format {:?}",
            info.pixel_format
        ));
    }
    let pixels = decoder.decode().map_err(|error| error.to_string())?;
    let channels = info.pixel_format.pixel_bytes();
    if pixels.len() != expected / 4 * channels {
        return Err("JPEG pixel count differs from dimensions".into());
    }
    let rgba = if channels == 3 {
        pixels::expand_rgb(&pixels, expected)?
    } else {
        pixels::expand_l8(&pixels, expected)?
    };
    Ok((size, rgba))
}

#[cfg(test)]
mod texture_format_tests {
    use super::*;
    #[test]
    fn original_breakout_jpeg_is_decoded_without_rewriting_it() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../.work/games/breakout/Assets/Images/coconut.jpg");
        let original = std::fs::read(&path).unwrap();
        let (size, pixels) = decode_texture(&path).unwrap();
        assert_eq!(size, [140, 140]);
        assert_eq!(pixels.len(), 140 * 140 * 4);
        assert!(pixels.chunks_exact(4).all(|p| p[3] == 255));
        assert!(pixels.chunks_exact(4).any(|p| p[..3] != pixels[..3]));
        assert_eq!(original, std::fs::read(path).unwrap());
        let renamed = tempfile::NamedTempFile::with_suffix(".bin").unwrap();
        std::fs::write(renamed.path(), original).unwrap();
        assert_eq!(decode_texture(renamed.path()).unwrap(), (size, pixels));
    }
    #[test]
    fn unknown_truncated_and_oversized_encoded_textures_fail_closed() {
        let file = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(file.path(), b"unknown image format").unwrap();
        assert!(
            decode_texture(file.path())
                .unwrap_err()
                .contains("unsupported texture signature")
        );
        std::fs::write(file.path(), [255, 216, 255, 224, 0, 0, 0, 0]).unwrap();
        assert!(decode_texture(file.path()).is_err());
        file.as_file().set_len(16_777_217).unwrap();
        assert!(
            decode_texture(file.path())
                .unwrap_err()
                .contains("exceeds 16 MiB")
        );
    }
}
