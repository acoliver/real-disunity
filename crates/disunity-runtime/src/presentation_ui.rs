//! Copies detached UI from the prepared frame in the host's loaded engine image.
#[cfg(test)]
#[path = "presentation_ui_tests.rs"]
mod tests;

use crate::{
    DisunityHost, HostError, native,
    presentation::{Coverage, decode_quad, read_coverage},
};
use std::collections::BTreeMap;
use worldrunner_world_simulation::{
    graphics::{MaterialId, RenderTextureId},
    presentation::extraction::SpriteQuad,
    ui::{
        UiTexture,
        presentation::{UiDraw, UiDrawKind, UiMaterial, UiPresentation},
    },
};

type Count = unsafe extern "C" fn(u64, *mut u32, *mut u32) -> native::DisunityResult;
type Draw = unsafe extern "C" fn(
    u64,
    u32,
    *mut native::DisunityUiDraw,
    *mut native::DisunityQuad,
) -> native::DisunityResult;

type MaterialRead =
    unsafe extern "C" fn(u64, u64, *mut native::DisunityMaterialInfo) -> native::DisunityResult;
type ShaderRead = unsafe extern "C" fn(
    u64,
    u64,
    *mut native::DisunityShaderInfo,
    *mut u8,
) -> native::DisunityResult;

impl DisunityHost {
    /// Reads typed pixel-space UI from the last `export_presentation`, before its lossy live
    /// 2D conversion. Preparation and unload replace/release the snapshot with the scene frame.
    pub fn ui_presentation(&self) -> Result<UiPresentation, HostError> {
        let world = self.world.ok_or(HostError::NoWorld)?;
        let count: Count = self.ui_symbol(b"disunity_presentation_ui_count\0")?;
        let draw: Draw = self.ui_symbol(b"disunity_presentation_ui_draw\0")?;
        let coverage: Coverage = self.ui_symbol(b"disunity_presentation_ui_coverage\0")?;
        let (mut draws, mut textures) = (0, 0);
        self.check(unsafe { count(world, &mut draws, &mut textures) })?;
        let mut plan = UiPresentation::default();
        for index in 0..draws {
            let mut meta = native::DisunityUiDraw::default();
            let mut quad = native::DisunityQuad::default();
            self.check(unsafe { draw(world, index, &mut meta, &mut quad) })?;
            plan.draws.push(decode(meta, quad)?);
        }
        plan.coverage =
            read_coverage(world, textures, coverage).map_err(|failed| self.native_error(failed))?;
        plan.shaders = self.ui_shader_names(world, &plan.draws)?;
        Ok(plan)
    }

    /// The shader name of each material the draws name, read through the material and shader
    /// calls, because the draw record carries material identities only.
    fn ui_shader_names(
        &self,
        world: u64,
        draws: &[UiDraw],
    ) -> Result<BTreeMap<UiMaterial, String>, HostError> {
        let material: MaterialRead = self.ui_symbol(b"disunity_material_read\0")?;
        let shader: ShaderRead = self.ui_symbol(b"disunity_shader_read\0")?;
        let mut names = BTreeMap::new();
        for used in draws.iter().filter_map(|draw| draw.material) {
            if names.contains_key(&used) {
                continue;
            }
            let mut info = native::DisunityMaterialInfo::default();
            self.check(unsafe { material(world, used.id.to_bits(), &mut info) })?;
            names.insert(used, self.shader_name(world, shader, info.shader)?);
        }
        Ok(names)
    }

    fn shader_name(&self, world: u64, read: ShaderRead, shader: u64) -> Result<String, HostError> {
        let mut info = native::DisunityShaderInfo::default();
        self.check(unsafe { read(world, shader, &mut info, std::ptr::null_mut()) })?;
        let mut text = vec![0_u8; info.text_capacity as usize];
        self.check(unsafe { read(world, shader, &mut info, text.as_mut_ptr()) })?;
        text.truncate(info.name_length as usize);
        String::from_utf8(text).map_err(|_| HostError::Managed("a shader name is not UTF-8".into()))
    }

    /// A render texture's description, with the sampler the world holds for it. Fails with the
    /// world's reason when the world holds no render texture with this id, as after the managed
    /// side destroyed it.
    ///
    /// # Errors
    /// No world is loaded, the engine image has no such call, or the world holds no such texture.
    pub fn render_texture(&self, id: u64) -> Result<native::DisunityRenderTexture, HostError> {
        type Read = unsafe extern "C" fn(
            u64,
            u64,
            *mut native::DisunityRenderTexture,
        ) -> native::DisunityResult;
        let world = self.world.ok_or(HostError::NoWorld)?;
        let read: Read = self.ui_symbol(b"disunity_render_texture_read\0")?;
        let mut texture = native::DisunityRenderTexture::default();
        self.check(unsafe { read(world, id, &mut texture) })?;
        Ok(texture)
    }

    fn ui_symbol<T: Copy>(&self, name: &'static [u8]) -> Result<T, HostError> {
        unsafe { self._library.get(name).map(|symbol| *symbol) }.map_err(|_| {
            HostError::MissingSymbol(String::from_utf8_lossy(name).trim_end_matches('\0').into())
        })
    }
}

fn decode(meta: native::DisunityUiDraw, quad: native::DisunityQuad) -> Result<UiDraw, HostError> {
    let kind = match meta.kind & !4 {
        0 | 2 => UiDrawKind::Graphic,
        1 => UiDrawKind::Text,
        kind => return Err(HostError::Managed(format!("unknown UI draw kind {kind}"))),
    };
    if quad.overlay != 1 {
        return Err(HostError::Managed(
            "UI overlay metadata on a non-overlay quad".into(),
        ));
    }
    let corner_colors = decode_corner_colors(&meta)?;
    let quad = decode_quad(quad);
    let draw = UiDraw {
        kind,
        depth_test: meta.kind & 4 != 0,
        corner_colors,
        distance_field: decode_distance(&meta)?,
        triangle_uv: (meta.kind & !4 == 2).then_some([
            [meta.triangle_u0, meta.triangle_v0],
            [meta.triangle_u1, meta.triangle_v1],
            [meta.triangle_u2, meta.triangle_v2],
        ]),
        corners: quad.corners,
        uv_rect: quad.uv_rect,
        color: quad.color,
        texture: decode_texture(kind, meta.texture_kind, &quad)?,
        material: decode_material(&meta)?,
        sorting_layer: quad.sorting_layer,
        sorting_order: quad.sorting_order,
        hierarchy_order: usize::try_from(meta.hierarchy_order)
            .map_err(|_| HostError::Managed("UI hierarchy order overflow".into()))?,
        clip_rect: [meta.clip_x, meta.clip_y, meta.clip_width, meta.clip_height],
    };
    draw.validate()
        .map_err(|e| HostError::Managed(e.to_string()))?;
    Ok(draw)
}

fn decode_material(meta: &native::DisunityUiDraw) -> Result<Option<UiMaterial>, HostError> {
    match (meta.material, meta.material_generation) {
        (0, 0) => Ok(None),
        (0, _) => Err(HostError::Managed(
            "UI material generation without a material".into(),
        )),
        (id, generation) => Ok(Some(UiMaterial {
            id: MaterialId::from_bits(id),
            generation,
        })),
    }
}

/// A target names a world render texture; any other texture is a stored image on a graphic and a
/// coverage index on text.
fn decode_texture(
    kind: UiDrawKind,
    texture_kind: u32,
    quad: &SpriteQuad,
) -> Result<Option<UiTexture>, HostError> {
    let Some(id) = quad.texture else {
        return Ok(None);
    };
    match (texture_kind, kind) {
        (0, UiDrawKind::Graphic) => Ok(Some(UiTexture::Image(id))),
        (0, UiDrawKind::Text) => Ok(Some(UiTexture::Coverage(id))),
        (1, _) => Ok(Some(UiTexture::Target(RenderTextureId::from_bits(id)))),
        _ => Err(HostError::Managed(format!(
            "unknown UI texture kind {texture_kind}"
        ))),
    }
}

fn decode_corner_colors(meta: &native::DisunityUiDraw) -> Result<Option<[[f32; 4]; 4]>, HostError> {
    let colors = [
        [
            meta.corner_0_r,
            meta.corner_0_g,
            meta.corner_0_b,
            meta.corner_0_a,
        ],
        [
            meta.corner_1_r,
            meta.corner_1_g,
            meta.corner_1_b,
            meta.corner_1_a,
        ],
        [
            meta.corner_2_r,
            meta.corner_2_g,
            meta.corner_2_b,
            meta.corner_2_a,
        ],
        [
            meta.corner_3_r,
            meta.corner_3_g,
            meta.corner_3_b,
            meta.corner_3_a,
        ],
    ];
    if meta.corner_colors_present > 1
        || colors
            .into_iter()
            .flatten()
            .any(|v| !v.is_finite() || !(0. ..=1.).contains(&v))
    {
        return Err(HostError::Managed("invalid UI corner_colors".into()));
    }
    Ok((meta.corner_colors_present == 1).then_some(colors))
}

fn decode_distance(
    meta: &native::DisunityUiDraw,
) -> Result<Option<worldrunner_world_simulation::ui::presentation::UiDistanceStyle>, HostError> {
    use worldrunner_world_simulation::ui::presentation::{UiDistanceMode, UiDistanceStyle};
    let mode = match meta.distance_mode {
        0 => return Ok(None),
        1 => UiDistanceMode::Fill,
        2 => UiDistanceMode::Outline,
        3 => UiDistanceMode::Glow,
        _ => return Err(HostError::Managed("invalid UI distance mode".into())),
    };
    Ok(Some(UiDistanceStyle {
        spread_px: meta.distance_spread,
        radius_px: meta.distance_radius,
        mode,
        effect_color: [meta.effect_r, meta.effect_g, meta.effect_b, meta.effect_a],
    }))
}
