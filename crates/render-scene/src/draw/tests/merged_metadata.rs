//! With reflections on, the depth and normal prepass also writes the reflection material, so the
//! geometry is drawn once for both.

use super::*;
use crate::compositor::{FrameRenderer, encode_scene};
use crate::scene_metadata::{MetadataOutput, SceneMetadata};
use worldrunner_render_plan::{SceneDecal, SceneSsr, SceneTint};
use worldrunner_render_post::{AttachmentView, PassSlot, ResourceRef, ToneMap};

const SIZE: [u32; 2] = [65, 49];
const PIXEL_BYTES: usize = 8;
/// Half-float bits of the opaque black the material target is cleared to.
const CLEARED_MATERIAL: [u8; PIXEL_BYTES] = [0, 0, 0, 0, 0, 0, 0, 0x3c];

struct Products {
    normals: Vec<u8>,
    material: Vec<u8>,
    depth: Vec<u8>,
}

fn reflections() -> SceneSsr {
    SceneSsr {
        enabled: true,
        max_steps: 64,
        thickness_millimeters: 100,
        roughness_cutoff_micro: 600_000,
        resolution_scale_micro: 500_000,
        ..Default::default()
    }
}

fn lit(scene: &RenderScene, ssr: bool) -> RenderScene {
    let mut parts = effect_parts();
    if !ssr {
        parts.ssao = Some(crate::ssr::neutral_ao());
    }
    let mut effects = SceneEffects::new(parts).unwrap();
    if ssr {
        effects = effects.with_ssr(reflections()).unwrap();
    }
    scene
        .with_objects_and_effects(scene.geometry().objects().to_vec(), Some(effects))
        .unwrap()
}

fn embedded_decal() -> SceneDecal {
    let mut decal = decals::decal();
    decal.dimensions_millimeters = [4000; 3];
    decal.edge_fade_micro = 0;
    decal
}

/// The scenes under test and the camera that frames each one.
fn scenes() -> Vec<(&'static str, RenderScene, CameraPose)> {
    vec![
        (
            "opaque and masked quads",
            ssao::source(0, true, 255),
            pose(),
        ),
        (
            "partly cut out masked quad",
            foliage::foliage_scene(None, SceneTint::WHITE, 0),
            pose(),
        ),
        (
            "relief impostor",
            impostor::relief_scene(),
            CameraPose {
                eye_millimeters: [0, -4000, 2000],
                target_millimeters: [0, 0, 1000],
            },
        ),
        (
            "decal on an object",
            decals::source(false, false)
                .with_decals(vec![embedded_decal()])
                .unwrap(),
            pose(),
        ),
        (
            "decal on terrain",
            decals::source(false, true)
                .with_decals(vec![embedded_decal()])
                .unwrap(),
            pose(),
        ),
        (
            "contact surface over terrain",
            foliage::contact_scene(0, true, false),
            pose(),
        ),
    ]
}

fn render(
    context: &GpuContext,
    shot: (&RenderScene, CameraPose),
    pipelines: &ScenePipelines,
) -> FrameRenderer {
    let scene = shot.0;
    let mut renderer = FrameRenderer::hdr(
        context,
        SIZE,
        COLOR_FORMAT,
        1,
        ToneMap::Reinhard,
        scene.effects(),
    )
    .unwrap();
    draw_with(context, &mut renderer, shot, pipelines);
    renderer
}

fn draw_with(
    context: &GpuContext,
    renderer: &mut FrameRenderer,
    shot: (&RenderScene, CameraPose),
    pipelines: &ScenePipelines,
) {
    let (scene, pose) = shot;
    let mut gpu =
        SceneGpu::upload(context.device(), context.submission(), scene, pipelines).unwrap();
    let color = AttachmentView::new(
        ResourceRef::new("scene color"),
        renderer.targets().scene().texture(),
    )
    .unwrap();
    let depth = AttachmentView::new(
        ResourceRef::new("scene depth"),
        renderer.targets().depth().texture(),
    )
    .unwrap();
    let mut encoder = context.submission().begin(context.device()).unwrap();
    encode_scene(
        renderer,
        context.device(),
        &mut encoder,
        (&mut gpu, pipelines),
        (&color, &depth),
        FrameUniform::new(&view(pose), scene.effects()).unwrap(),
    )
    .unwrap();
    renderer.targets().encode_resolve(&mut encoder);
    encoder.finish().submit().unwrap();
}

fn view(pose: CameraPose) -> SceneView {
    SceneView::new(
        pose,
        camera(),
        Viewport {
            width: SIZE[0],
            height: SIZE[1],
        },
    )
    .unwrap()
}

fn read(context: &GpuContext, texture: &wgpu::Texture) -> Vec<u8> {
    ssao::read_texture(context, SIZE, texture)
}

/// The prepass normals, the reflection material and the linear depth one frame produced.
fn products(
    context: &GpuContext,
    renderer: &FrameRenderer,
    material: Option<&wgpu::Texture>,
) -> Products {
    let screen = renderer.ssao.as_ref().unwrap();
    Products {
        normals: read(context, &screen.normals),
        material: material.map_or_else(Vec::new, |texture| read(context, texture)),
        depth: read(context, screen.passes.depth_levels()[0]),
    }
}

/// A frame with reflections on: the prepass writes the normals and the material together.
fn merged(context: &GpuContext, scene: &RenderScene, pose: CameraPose) -> Products {
    let pipelines =
        ScenePipelines::with_samples(context.device(), wgpu::TextureFormat::Rgba16Float, 1)
            .unwrap();
    let renderer = render(context, (&lit(scene, true), pose), &pipelines);
    let material = renderer.ssr.as_ref().unwrap().material_target();
    products(context, &renderer, Some(material))
}

/// A frame with the prepass drawing the material pipelines alone, which is what the removed
/// reflection material pass drew. The target is the normals texture, cleared transparent.
fn material_only(context: &GpuContext, scene: &RenderScene, pose: CameraPose) -> Products {
    let scene = lit(scene, false);
    let pipelines =
        ScenePipelines::with_ssao(context.device(), wgpu::TextureFormat::Rgba16Float, 1).unwrap();
    let mut renderer = FrameRenderer::hdr(
        context,
        SIZE,
        COLOR_FORMAT,
        1,
        ToneMap::Reinhard,
        scene.effects(),
    )
    .unwrap();
    let metadata = renderer.ssao.as_mut().unwrap();
    (metadata.geometry.pipelines, metadata.geometry.group) = crate::scene_metadata::pipelines(
        context.device(),
        &metadata.uniform,
        MetadataOutput::Material,
    );
    draw_with(context, &mut renderer, (&scene, pose), &pipelines);
    let screen = renderer.ssao.as_ref().unwrap();
    Products {
        normals: Vec::new(),
        material: read(context, &screen.normals),
        depth: read(context, screen.passes.depth_levels()[0]),
    }
}

/// A frame with the stock normal-only prepass.
fn normals_only(context: &GpuContext, scene: &RenderScene, pose: CameraPose) -> Products {
    let scene = lit(scene, false);
    let pipelines =
        ScenePipelines::with_ssao(context.device(), wgpu::TextureFormat::Rgba16Float, 1).unwrap();
    let renderer = render(context, (&scene, pose), &pipelines);
    products(context, &renderer, None)
}

fn pixels(bytes: &[u8]) -> impl Iterator<Item = &[u8]> {
    bytes.chunks_exact(PIXEL_BYTES)
}

#[test]
fn merged_prepass_writes_the_same_texels_as_separate_normal_and_material_draws() {
    let context = context();
    for (name, scene, pose) in scenes() {
        let merged = merged(&context, &scene, pose);
        let normals = normals_only(&context, &scene, pose);
        let material = material_only(&context, &scene, pose);
        assert_eq!(merged.normals, normals.normals, "{name}: normals");
        assert_eq!(merged.depth, normals.depth, "{name}: depth");
        assert_eq!(merged.depth, material.depth, "{name}: material pass depth");
        let mut covered = 0;
        for ((pixel, coverage), expected) in pixels(&merged.material)
            .zip(pixels(&normals.normals))
            .zip(pixels(&material.material))
        {
            if coverage[6..] == [0, 0] {
                assert_eq!(pixel, CLEARED_MATERIAL, "{name}: uncovered material");
            } else {
                covered += 1;
                assert_eq!(pixel, expected, "{name}: covered material");
            }
        }
        assert!(covered > 0, "{name}: nothing was drawn");
    }
}

fn geometry_passes(context: &GpuContext, scene: &RenderScene, ssr: bool) -> Vec<PassSlot> {
    let pose = pose();
    crate::scene_metadata::EXECUTED_PASSES.with_borrow_mut(Vec::clear);
    if ssr {
        merged(context, scene, pose);
    } else {
        normals_only(context, scene, pose);
    }
    crate::scene_metadata::EXECUTED_PASSES.with_borrow(|passes| {
        passes
            .iter()
            .copied()
            .filter(|slot| matches!(slot, PassSlot::DepthNormalPrepass | PassSlot::Opaque))
            .collect()
    })
}

#[test]
fn reflections_add_no_geometry_pass() {
    let context = context();
    let scene = ssao::source(0, true, 255);
    let without = geometry_passes(&context, &scene, false);
    assert_eq!(without, [PassSlot::DepthNormalPrepass, PassSlot::Opaque]);
    assert_eq!(geometry_passes(&context, &scene, true), without);
}

#[test]
fn prepass_declares_the_material_only_when_one_is_supplied() {
    let context = context();
    let mut screen = SceneMetadata::new(context.device(), SIZE, crate::ssr::neutral_ao()).unwrap();
    let ssr = crate::ssr::Ssr::new(
        context.device(),
        worldrunner_render_post::FrameConfig::new(SIZE, wgpu::TextureFormat::Rgba16Float, 1),
        reflections(),
        &mut screen,
    )
    .unwrap();
    let declare = |material| {
        let (mut passes, mut views) = (Vec::new(), Vec::new());
        crate::ssao::passes::prepass(&screen, material, &mut passes, &mut views).unwrap();
        (passes.remove(0), views.len())
    };
    let (plain, plain_views) = declare(None);
    assert!(plain.additional_colors.is_empty());
    let (combined, combined_views) = declare(Some(ssr.material_target()));
    let [material] = &combined.additional_colors[..] else {
        panic!("the prepass declares exactly one extra colour output");
    };
    assert_eq!(material.resource, ResourceRef::new("reflection material"));
    assert_eq!(material.format, wgpu::TextureFormat::Rgba16Float);
    assert_eq!(
        material.load,
        worldrunner_render_post::AttachmentLoad::ClearColor(wgpu::Color::BLACK)
    );
    assert_eq!(combined.color, plain.color);
    assert_eq!(combined.depth, plain.depth);
    assert_eq!(combined_views, plain_views + 1);
}
