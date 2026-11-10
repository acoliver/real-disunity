//! The terrains, cameras and page sources the captures draw.
//!
//! The terrain is not flat and its surfaces differ in all three palette maps, so a wrong map,
//! a wrong layer or a transposed binding shows in the pixels. Nothing here draws or compares.

use worldrunner_render_gpu::configuration::AllocationBudget;
use worldrunner_render_plan::{
    MaterialHandle, RenderScene, SceneAlphaMode, SceneCamera, SceneEffects, SceneGeometry,
    SceneMaterial, SceneMaterialParts, SceneNormalEncoding, SceneParts, SceneResources,
    SceneTerrain, SceneTerrainAppearance, SceneTerrainParts, SceneTerrainSurface, SceneTexture,
    TerrainLayer, TerrainLayeredSource, TerrainLodPolicy, TerrainPageSettings,
    TerrainPageSettingsParts, TerrainSplatFilter, TerrainStreamingPolicy, TerrainTypeGrid,
    TexelFormat, TextureColorSpace, TextureHandle,
};

use super::SIZE;
use crate::frame::FrameUniform;
use crate::terrain::pages::composition::LayerSample;
use crate::terrain::pages::scene::TerrainPageSource;
use crate::{CameraPose, SceneView, Viewport};

/// Samples along each edge of the plain terrain.
const EDGE: u32 = 9;

fn texels(
    format: TexelFormat,
    space: TextureColorSpace,
    size: [u32; 2],
    bytes: &[u8],
) -> SceneTexture {
    SceneTexture::Texels {
        format,
        color_space: space,
        width: size[0],
        height: size[1],
        mips: 1,
        bytes: bytes.into(),
    }
}

/// Three 2x2 maps for the surface `index`: albedo, normal and roughness-metallic, each with four
/// distinct texels so that sampling a neighbour, a wrong map or a wrong layer changes the result.
fn palette_maps(index: u8) -> [SceneTexture; 3] {
    let shift = index * 70;
    let albedo: Vec<u8> = (0..4u8)
        .flat_map(|texel| {
            [
                40u8.wrapping_add(shift).wrapping_add(texel * 50),
                200u8.wrapping_sub(shift).wrapping_sub(texel * 30),
                90u8.wrapping_add(texel * 20),
                255,
            ]
        })
        .collect();
    let normal: Vec<u8> = (0..4u8)
        .flat_map(|texel| [118 + texel * 6, 138 - texel * 5 + index * 4, 250, 255])
        .collect();
    let rough_metal: Vec<u8> = (0..4u8)
        .flat_map(|texel| [60 + texel * 40 + index * 30, 20 + texel * 60, 0, 255])
        .collect();
    [
        texels(
            TexelFormat::Rgba8Unorm,
            TextureColorSpace::Srgb,
            [2, 2],
            &albedo,
        ),
        texels(
            TexelFormat::Rgba8Unorm,
            TextureColorSpace::Linear,
            [2, 2],
            &normal,
        ),
        texels(
            TexelFormat::Rgba8Unorm,
            TextureColorSpace::Linear,
            [2, 2],
            &rough_metal,
        ),
    ]
}

fn terrain_material(index: u32) -> SceneMaterial {
    SceneMaterial::new(SceneMaterialParts {
        albedo: TextureHandle::from_raw(index * 3),
        normal: Some(TextureHandle::from_raw(index * 3 + 1)),
        roughness_metallic: Some(TextureHandle::from_raw(index * 3 + 2)),
        ambient_occlusion: None,
        self_illumination: None,
        roughness_micro: 1_000_000,
        metalness_micro: 0,
        alpha_mode: SceneAlphaMode::Opaque,
        double_sided: false,
    })
    .expect("the fixture material carries in-range scalars")
}

fn resources() -> SceneResources {
    let textures = (0..2u8).flat_map(palette_maps).collect();
    SceneResources::new(textures, (0..2).map(terrain_material).collect())
        .expect("the fixture resources validate")
}

fn effects() -> SceneEffects {
    SceneEffects::new(crate::draw::tests::effect_parts()).expect("the fixture effects validate")
}

/// A rolling `EDGE` by `EDGE` terrain of two surfaces, a metre between samples, centred on the
/// origin.
fn rolling() -> SceneTerrain {
    let samples = (EDGE * EDGE) as usize;
    let heights = (0..samples)
        .map(|index| {
            let (column, row) = (index % EDGE as usize, index / EDGE as usize);
            i32::try_from((column * 37 + row * 91 + column * row * 13) % 11).unwrap() * 90 - 450
        })
        .collect();
    let indices = (0..samples)
        .map(|index| u8::from((index % 4 + index / 4) % 3 == 0))
        .collect();
    SceneTerrain::new(SceneTerrainParts {
        columns: EDGE,
        rows: EDGE,
        origin_east_millimeters: -4_000,
        origin_north_millimeters: -4_000,
        sample_spacing_millimeters: 1_000,
        heights_millimeters: heights,
        surface_indices: indices,
        surfaces: (0..2)
            .map(|material| {
                SceneTerrainSurface::new(
                    MaterialHandle::from_raw(material),
                    SceneTerrainAppearance::NEUTRAL,
                )
            })
            .collect(),
        lod_policy: TerrainLodPolicy::FullResolution,
    })
    .expect("the fixture terrain validates")
}

fn scene(terrain: SceneTerrain, resources: SceneResources) -> RenderScene {
    scene_with(terrain, resources, effects())
}

fn scene_with(
    terrain: SceneTerrain,
    resources: SceneResources,
    effects: SceneEffects,
) -> RenderScene {
    RenderScene::new(SceneParts {
        resources,
        geometry: SceneGeometry::new(vec![], vec![]).expect("empty geometry"),
        terrain: Some(terrain),
        effects: Some(effects),
    })
    .expect("the fixture scene validates")
}

/// The terrain as one mesh, with the conventional palette arrays and a type grid.
pub(in crate::terrain) fn plain() -> RenderScene {
    scene(rolling(), resources())
}

/// The plain terrain in a scene that requests screen-space reflections.
pub(in crate::terrain) fn reflecting(request: worldrunner_render_plan::SceneSsr) -> RenderScene {
    let effects = effects()
        .with_ssr(request)
        .expect("the fixture request validates");
    scene_with(rolling(), resources(), effects)
}

/// The same terrain streamed as resident chunks.
pub(in crate::terrain) fn streamed(maximum_resident: u32) -> RenderScene {
    let terrain = rolling().with_streaming_policy(Some(
        TerrainStreamingPolicy::new(1, maximum_resident, 4, 1000).expect("a valid policy"),
    ));
    scene(terrain, resources())
}

fn layered_texels(size: [u32; 2], bytes: &[u8]) -> SceneTexture {
    texels(
        TexelFormat::Rgba8Unorm,
        TextureColorSpace::Linear,
        size,
        bytes,
    )
}

fn layered_surface(index: u32) -> SceneTerrainSurface {
    SceneTerrainSurface::new(
        MaterialHandle::from_raw(0),
        SceneTerrainAppearance {
            layered: Some(TerrainLayer {
                albedo_metallic: TextureHandle::from_raw(if index == 0 { 0 } else { 3 }),
                normal_roughness: TextureHandle::from_raw(1),
                height: TextureHandle::from_raw(2),
                normal_encoding: SceneNormalEncoding::Rgb,
                repeat_millimeters: 2000,
                offset_millimeters: [0; 2],
                roughness_metallic_offset_micro: [0; 2],
            }),
            blur_radius_millimeters: 500,
            ..SceneTerrainAppearance::NEUTRAL
        },
    )
}

/// A terrain whose surfaces are composed from layered sources instead of palette arrays.
pub(in crate::terrain) fn layered() -> RenderScene {
    let resources = SceneResources::new(
        vec![
            layered_texels([2, 1], &[60, 110, 160, 70, 150, 80, 100, 110]),
            layered_texels([1, 2], &[128, 128, 255, 130, 145, 115, 250, 170]),
            SceneTexture::Texels {
                format: TexelFormat::R8Unorm,
                color_space: TextureColorSpace::Linear,
                width: 1,
                height: 1,
                mips: 1,
                bytes: vec![160].into(),
            },
            layered_texels([1, 1], &[140, 90, 60, 100]),
        ],
        vec![
            SceneMaterial::new(SceneMaterialParts {
                albedo: TextureHandle::from_raw(0),
                normal: None,
                roughness_metallic: None,
                ambient_occlusion: None,
                self_illumination: None,
                roughness_micro: 1_000_000,
                metalness_micro: 0,
                alpha_mode: SceneAlphaMode::Opaque,
                double_sided: false,
            })
            .expect("a plain material"),
        ],
    )
    .expect("layered resources validate");
    let samples = 25usize;
    let heights = (0..samples)
        .map(|index| i32::try_from((index * 7) % 5).unwrap() * 120 - 240)
        .collect();
    let grid: Vec<u8> = (0..samples).map(|index| u8::from(index % 3 == 0)).collect();
    let terrain = SceneTerrain::new(SceneTerrainParts {
        columns: 5,
        rows: 5,
        origin_east_millimeters: -2_000,
        origin_north_millimeters: -2_000,
        sample_spacing_millimeters: 1_000,
        heights_millimeters: heights,
        surface_indices: grid.clone(),
        surfaces: (0..2).map(layered_surface).collect(),
        lod_policy: TerrainLodPolicy::FullResolution,
    })
    .expect("layered terrain validates")
    .with_layered_source(TerrainLayeredSource {
        anisotropy: worldrunner_render_plan::TextureAnisotropy::default(),
        channel_map: None,
        type_grid: TerrainTypeGrid::new([5, 5], [-2_000; 2], 1000, grid)
            .expect("a valid type grid"),
        filter: TerrainSplatFilter::Bilinear,
        colorization: None,
    })
    .expect("a valid layered source");
    scene(terrain, resources)
}

/// Material pages for the terrain: one root page of the whole terrain, sampled from `sample`.
pub(in crate::terrain) fn page_source(layered: bool) -> TerrainPageSource {
    TerrainPageSource {
        settings: TerrainPageSettings::new(TerrainPageSettingsParts {
            interior: [SIZE * 2; 2],
            border: 1,
            filter_radius: 1,
            maximum_depth: 0,
            split_threshold_millipixels: 2000,
            merge_threshold_millipixels: 1000,
            maximum_selected_pages: 1,
            resident_bytes: 1_000_000,
            upload_bytes_per_frame: 1_000_000,
        })
        .expect("valid page settings"),
        slots: 1,
        budget: AllocationBudget(20_000_000),
        sample: if layered {
            Box::new(|_| panic!("typed layered sources never use the linear callback"))
        } else {
            Box::new(|at| {
                // Bounded and position-dependent, so a wrong page lookup shows.
                vec![LayerSample {
                    color: [
                        channel(at[0]),
                        channel(at[1]),
                        channel(1.0 - at[0] * at[1]),
                        255,
                    ],
                    normal: [0, 0, i16::MAX],
                    roughness: 150,
                    weight: 1,
                }]
            })
        },
    }
}

#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "the value is clamped to 0..=255 before it is cast"
)]
fn channel(value: f64) -> u8 {
    (value.clamp(0.0, 1.0) * 255.0).round() as u8
}

/// The camera every capture looks through: above and south of the terrain, looking at its
/// middle.
pub(in crate::terrain) fn pose() -> CameraPose {
    CameraPose {
        eye_millimeters: [0, -5_000, 5_500],
        target_millimeters: [0, 0, 0],
    }
}

/// The frame uniform of `pose` over the `SIZE` by `SIZE` targets.
pub(in crate::terrain) fn frame(pose: CameraPose) -> FrameUniform {
    frame_with(pose, &effects())
}

/// [`frame`] for a scene with `effects`.
pub(in crate::terrain) fn frame_with(pose: CameraPose, effects: &SceneEffects) -> FrameUniform {
    let view = SceneView::new(
        pose,
        SceneCamera {
            vertical_fov_millidegrees: 60_000,
            near_plane_millimeters: 100,
            far_plane_millimeters: 1_000_000,
        },
        Viewport {
            width: SIZE,
            height: SIZE,
        },
    )
    .expect("the fixture pose is well formed");
    FrameUniform::new(&view, Some(effects)).expect("the fixture frame builds")
}

/// `frame` seen through a mirrored projection with the winding convention that matches it, so
/// the same triangles face the camera and the pipelines run with `FrontFace::Cw`.
pub(in crate::terrain) fn mirrored(mut frame: FrameUniform) -> FrameUniform {
    frame.view_convention = worldrunner_camera::ViewConvention {
        handedness: worldrunner_camera::ViewHandedness::DirectXLeftHanded,
    };
    frame.view_projection =
        glam::Mat4::from_scale(glam::Vec3::new(-1.0, 1.0, 1.0)) * frame.view_projection;
    frame
}
