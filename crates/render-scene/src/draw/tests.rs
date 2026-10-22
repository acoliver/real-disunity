//! Behavioural claims about uploading one scene and drawing it.
//!
//! The end-to-end tests here are the only place where the vertex stride, the
//! vertex attribute offsets, the instance stride, the bind group layouts and
//! the WGSL bindings are all checked against each other at once. Pipeline
//! creation validates the shader against the layout, but a buffer whose stride
//! disagrees with the attributes, or a draw whose index count overruns the
//! vertex buffer, is only caught when a draw is actually recorded. Reading the
//! rendered pixels back is what turns "the driver accepted it" into "the
//! geometry landed where it was aimed".

use super::{DrawError, SceneGpu};
use crate::device::GpuContext;
use crate::frame::FrameUniform;
use crate::renderer::{DEPTH_FORMAT, ScenePipelines, depth_texture};
use crate::view::{CameraPose, SceneView, Viewport};
use wgpu::{
    Buffer, BufferDescriptor, BufferUsages, Color, CommandEncoder, CommandEncoderDescriptor,
    Device, Extent3d, LoadOp, MapMode, Operations, RenderPassColorAttachment,
    RenderPassDepthStencilAttachment, RenderPassDescriptor, StoreOp, TexelCopyBufferInfo,
    TexelCopyBufferLayout, Texture, TextureDescriptor, TextureDimension, TextureFormat,
    TextureUsages, TextureView, TextureViewDescriptor,
};
use worldrunner_render_plan::SceneDirection;
use worldrunner_render_plan::{
    BillboardPolicy, MaterialHandle, MeshHandle, ROTATION_MICRO_SCALE, RenderScene, SceneAlphaMode,
    SceneAmbient, SceneAntiAlias, SceneCamera, SceneColor, SceneColorGrading, SceneEffects,
    SceneEffectsParts, SceneGeometry, SceneMaterial, SceneMaterialParts, SceneMesh, SceneMeshLod,
    SceneObject, SceneParts, SceneResources, SceneRotation, SceneShadowCascades, SceneSky,
    SceneSun, SceneTerrain, SceneTerrainAppearance, SceneTerrainParts, SceneTerrainSurface,
    SceneTexture, SceneTransform, SceneVertex, TerrainLodPolicy, TextureColorSpace,
    TextureEncoding, TextureHandle,
};
mod environment;
mod material_factors;
mod terrain_blend;

/// The colour format every test renders into.
const COLOR_FORMAT: TextureFormat = TextureFormat::Rgba8UnormSrgb;

/// Edge length of the render target, chosen so a readback stays cheap.
const TARGET: u32 = 64;

/// Micro-units in one whole unit, matching the scene domain's scale.
const MICRO: i64 = 1_000_000;

fn context() -> GpuContext {
    pollster::block_on(GpuContext::request())
        .expect("GPU acceptance requires a real adapter with BC support")
}

/// Builds a single-mip 4x4 BC1 DDS whose every texel decodes to `color0`.
///
/// The block stores the requested `color0` against `color1 = 0x0000` with all
/// indices zero. Because `color0 > color1` this is the four-colour opaque
/// mode, so every texel resolves to `color0`. That matters: the obvious filler
/// of identical endpoints selects BC1's punch-through mode instead, where
/// index three decodes to transparent black, and a test expecting a colour
/// would then be reading the codec's alpha behaviour rather than the
/// renderer's.
fn bc1(color0: u16) -> Vec<u8> {
    bc1_with_format(color0, BC1_UNORM_SRGB)
}

/// DXGI `BC1_UNORM_SRGB`, the colour-managed block format.
const BC1_UNORM_SRGB: u32 = 72;

/// DXGI `BC1_UNORM`. Normal and rough/metal maps store measurements rather
/// than colour, and the container has to say so: the loader rejects a texture
/// whose declared colour space disagrees with its container, which is what
/// stops a normal map from being silently gamma-decoded into a wrong direction.
const BC1_UNORM: u32 = 71;

fn bc1_with_format(color0: u16, dxgi_format: u32) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(156);
    bytes.extend_from_slice(b"DDS ");
    bytes.extend_from_slice(&124u32.to_le_bytes());
    bytes.extend_from_slice(&0u32.to_le_bytes());
    bytes.extend_from_slice(&4u32.to_le_bytes());
    bytes.extend_from_slice(&4u32.to_le_bytes());
    bytes.extend_from_slice(&0u32.to_le_bytes());
    bytes.extend_from_slice(&0u32.to_le_bytes());
    bytes.extend_from_slice(&1u32.to_le_bytes());
    bytes.resize(76, 0);
    bytes.extend_from_slice(&32u32.to_le_bytes());
    bytes.extend_from_slice(&0x0004u32.to_le_bytes());
    bytes.extend_from_slice(b"DX10");
    bytes.resize(128, 0);
    bytes.extend_from_slice(&dxgi_format.to_le_bytes());
    bytes.extend_from_slice(&3u32.to_le_bytes());
    bytes.extend_from_slice(&0u32.to_le_bytes());
    bytes.extend_from_slice(&1u32.to_le_bytes());
    bytes.extend_from_slice(&0u32.to_le_bytes());
    bytes.extend_from_slice(&color0.to_le_bytes());
    bytes.extend_from_slice(&[0x00, 0x00, 0x00, 0x00, 0x00, 0x00]);
    bytes
}

/// RGB565 white, the neutral albedo most tests use.
const WHITE_565: u16 = 0xFFFF;

/// RGB565 pure red, used to tell a material's own texture from the placeholder.
const RED_565: u16 = 0xF800;

fn texture(color0: u16) -> SceneTexture {
    SceneTexture::Encoded {
        encoding: TextureEncoding::DdsContainer,
        color_space: TextureColorSpace::Srgb,
        bytes: bc1(color0).into(),
    }
}

fn white_texture() -> SceneTexture {
    texture(WHITE_565)
}

/// RGB565 mid-grey, which is what an unperturbed tangent-space normal map
/// stores.
///
/// White is emphatically not neutral here. A normal map is decoded as
/// `rg * 2 - 1`, so white decodes to a tangent normal lying flat in the surface
/// plane with no upward component at all, and every lit surface using it turns
/// black. Half-intensity is the value that decodes back to straight up.
const FLAT_NORMAL_565: u16 = 0x8410;

fn flat_normal_texture() -> SceneTexture {
    SceneTexture::Encoded {
        encoding: TextureEncoding::DdsContainer,
        // A normal map stores directions, not colour, so it must not be
        // gamma-decoded on the way in.
        color_space: TextureColorSpace::Linear,
        bytes: bc1_with_format(FLAT_NORMAL_565, BC1_UNORM).into(),
    }
}

fn material(alpha_mode: SceneAlphaMode, double_sided: bool) -> SceneMaterial {
    SceneMaterial::new(SceneMaterialParts {
        albedo: TextureHandle::from_raw(0),
        normal: None,
        roughness_metallic: None,
        ambient_occlusion: None,
        self_illumination: None,
        roughness_micro: 1_000_000,
        metalness_micro: 0,
        alpha_mode,
        double_sided,
    })
    .expect("the fixture material carries in-range scalars")
}

/// A material fit to back a terrain palette layer.
///
/// Terrain samples albedo, normal and rough/metal as three texture arrays, and
/// an array layer cannot be left empty: every layer must agree on format and
/// extent, so the 1x1 placeholder that stands in for a missing map elsewhere
/// cannot stand in here. A terrain material must therefore carry all three
/// maps, and the fixture says so explicitly rather than inheriting the sparse
/// `material` helper and failing at upload.
fn terrain_material() -> SceneMaterial {
    SceneMaterial::new(SceneMaterialParts {
        albedo: TextureHandle::from_raw(0),
        normal: Some(TextureHandle::from_raw(1)),
        roughness_metallic: Some(TextureHandle::from_raw(2)),
        ambient_occlusion: None,
        self_illumination: None,
        roughness_micro: 1_000_000,
        metalness_micro: 0,
        alpha_mode: SceneAlphaMode::Opaque,
        double_sided: false,
    })
    .expect("the fixture material carries in-range scalars")
}

fn vertex(position: [i64; 3], texcoord: [i64; 2]) -> SceneVertex {
    SceneVertex {
        position_millimeters: position,
        normal_micro: [0, 0, MICRO],
        texcoord_micro: texcoord,
    }
}

/// A two-metre ground quad at elevation zero, facing up.
///
/// The winding is counter-clockwise seen from above, which is what the
/// pipelines declare as front-facing. A quad wound the other way would vanish
/// entirely under back-face culling, which is exactly the failure the
/// single-sided test below is looking for.
fn quad() -> SceneMesh {
    let half = 1_000;
    SceneMesh::new(vec![
        SceneMeshLod::new(
            0,
            vec![
                vertex([-half, -half, 0], [0, 0]),
                vertex([half, -half, 0], [MICRO, 0]),
                vertex([half, half, 0], [MICRO, MICRO]),
                vertex([-half, half, 0], [0, MICRO]),
            ],
            vec![0, 1, 2, 0, 2, 3],
            None,
        )
        .expect("the fixture quad is a valid level"),
    ])
    .expect("a one-level chain is valid")
}

/// The same quad with both triangles wound the opposite way.
fn reversed_quad() -> SceneMesh {
    SceneMesh::new(vec![
        SceneMeshLod::new(
            0,
            quad().lods()[0].vertices().to_vec(),
            vec![2, 1, 0, 3, 2, 0],
            None,
        )
        .expect("the reversed quad is a valid level"),
    ])
    .expect("a one-level chain is valid")
}

/// A vertex of an upright card, whose outward normal is local `-Y`.
fn card_vertex(position: [i64; 3], texcoord: [i64; 2]) -> SceneVertex {
    SceneVertex {
        position_millimeters: position,
        normal_micro: [0, -MICRO, 0],
        texcoord_micro: texcoord,
    }
}

/// A two-metre card standing on the ground plane, authored to the convention
/// the billboard shader assumes: spanning local `X` and `Z`, facing local `-Y`.
///
/// Winding is counter-clockwise seen from local `-Y`, so a viewer the card
/// faces sees its front. That is what makes the culling in these tests a
/// signal: if a billboard fails to turn, the card is not merely dim, it is
/// gone.
fn card(billboard: Option<BillboardPolicy>) -> SceneMesh {
    let half = 1_000;
    SceneMesh::new(vec![
        SceneMeshLod::new(
            0,
            vec![
                card_vertex([-half, 0, 0], [0, MICRO]),
                card_vertex([half, 0, 0], [MICRO, MICRO]),
                card_vertex([half, 0, 2 * half], [MICRO, 0]),
                card_vertex([-half, 0, 2 * half], [0, 0]),
            ],
            vec![0, 1, 2, 0, 2, 3],
            billboard,
        )
        .expect("the fixture card is a valid level"),
    ])
    .expect("a one-level chain is valid")
}

/// A half turn about the vertical axis, which points a card away from a
/// camera standing south of it.
fn turned_away() -> SceneRotation {
    SceneRotation {
        x: 0,
        y: 0,
        z: ROTATION_MICRO_SCALE,
        w: 0,
    }
}

/// Builds a scene holding one single-sided card under the given policy.
fn card_scene(billboard: Option<BillboardPolicy>, rotation: SceneRotation) -> RenderScene {
    let placement = SceneTransform {
        rotation,
        ..identity()
    };
    let object = SceneObject::new(
        placement,
        MeshHandle::from_raw(0),
        MaterialHandle::from_raw(0),
    );
    RenderScene::new(SceneParts {
        resources: SceneResources::new(
            vec![white_texture()],
            vec![material(SceneAlphaMode::Opaque, false)],
        )
        .expect("the fixture resources validate"),
        geometry: SceneGeometry::new(vec![card(billboard)], vec![object])
            .expect("the fixture geometry validates"),
        terrain: None,
        effects: Some(effects()),
    })
    .expect("the fixture scene validates")
}

fn identity() -> SceneTransform {
    SceneTransform {
        position_millimeters: [0, 0, 0],
        rotation: SceneRotation {
            x: 0,
            y: 0,
            z: 0,
            w: ROTATION_MICRO_SCALE,
        },
        scale_millionths: 1_000_000,
    }
}

fn camera() -> SceneCamera {
    SceneCamera {
        vertical_fov_millidegrees: 60_000,
        near_plane_millimeters: 100,
        far_plane_millimeters: 1_000_000,
    }
}

fn effects() -> SceneEffects {
    SceneEffects::new(effect_parts()).expect("the fixture effects validate")
}

fn effect_parts() -> SceneEffectsParts {
    SceneEffectsParts {
        water: None,
        sky: SceneSky::ProceduralHemisphere {
            zenith: SceneColor {
                red: 255,
                green: 255,
                blue: 255,
            },
            horizon: SceneColor {
                red: 255,
                green: 255,
                blue: 255,
            },
        },
        sun: SceneSun {
            // Straight down from the zenith, so the upward-facing quad is lit
            // head on and a dark result cannot be blamed on grazing incidence.
            direction: SceneDirection::new([0, 0, 1_000_000]).unwrap(),
            irradiance_micro: 1_000_000,
        },
        ambient: SceneAmbient {
            sky_irradiance_micro: 1_000_000,
            ground_irradiance_micro: 1_000_000,
        },
        shadows: SceneShadowCascades::new(vec![10_000, 40_000], 50_000)
            .expect("two ascending cascades are valid"),
        ssao: None,
        camera: camera(),
        grading: SceneColorGrading {
            exposure_micro: 1_000_000,
            contrast_micro: 1_000_000,
            saturation_micro: 1_000_000,
        },
        post: Vec::new(),
        anti_alias: SceneAntiAlias::Disabled,
    }
}

/// Builds a scene holding exactly one texture, material and mesh.
fn build(
    texture: SceneTexture,
    material: SceneMaterial,
    mesh: SceneMesh,
    placements: usize,
) -> RenderScene {
    let objects = (0..placements)
        .map(|_| {
            SceneObject::new(
                identity(),
                MeshHandle::from_raw(0),
                MaterialHandle::from_raw(0),
            )
        })
        .collect();
    RenderScene::new(SceneParts {
        resources: SceneResources::new(vec![texture], vec![material])
            .expect("the fixture resources validate"),
        geometry: SceneGeometry::new(vec![mesh], objects).expect("the fixture geometry validates"),
        terrain: None,
        effects: Some(effects()),
    })
    .expect("the fixture scene validates")
}

/// A flat eight-metre terrain half a metre below the ground quad.
///
/// The palette entries alternate across the samples, so a test can tell a
/// per-material split from one merged surface. The grid sits below the quad
/// rather than through it, so terrain reaching the target cannot be confused
/// with two coplanar surfaces fighting over the depth buffer.
fn terrain(materials: usize) -> SceneTerrain {
    let limit = u8::try_from(materials).expect("the fixture palette is tiny");
    let indices = (0..9)
        .map(|sample| u8::try_from(sample % usize::from(limit)).expect("a remainder fits its base"))
        .collect();
    SceneTerrain::new(SceneTerrainParts {
        columns: 3,
        rows: 3,
        origin_east_millimeters: -4_000,
        origin_north_millimeters: -4_000,
        sample_spacing_millimeters: 4_000,
        heights_millimeters: vec![-500; 9],
        surface_indices: indices,
        surfaces: (0..u32::from(limit))
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

/// Builds a scene of one white quad over a terrain of the given palette size.
fn terrain_scene(materials: usize) -> RenderScene {
    let object = SceneObject::new(
        identity(),
        MeshHandle::from_raw(0),
        MaterialHandle::from_raw(0),
    );
    RenderScene::new(SceneParts {
        resources: SceneResources::new(
            vec![white_texture(), flat_normal_texture(), white_texture()],
            (0..materials).map(|_| terrain_material()).collect(),
        )
        .expect("the fixture resources validate"),
        geometry: SceneGeometry::new(vec![quad()], vec![object])
            .expect("the fixture geometry validates"),
        terrain: Some(terrain(materials)),
        effects: Some(effects()),
    })
    .expect("the fixture scene validates")
}

/// Builds a scene of one white quad, placed the given number of times.
fn scene(alpha_mode: SceneAlphaMode, double_sided: bool, placements: usize) -> RenderScene {
    build(
        white_texture(),
        material(alpha_mode, double_sided),
        quad(),
        placements,
    )
}

/// The pose used by every rendering test: above and south of the quad.
///
/// An eye placed directly overhead would make the view basis degenerate, so
/// the camera is offset south and looks back at the origin, which also proves
/// the quad is not merely filling the frame by accident of a null transform.
fn pose() -> CameraPose {
    CameraPose {
        eye_millimeters: [0, -2_000, 2_000],
        target_millimeters: [0, 0, 0],
    }
}

/// A camera high above the card, where a vertical card is strongly
/// foreshortened but one free to tilt is not.
fn overhead_pose() -> CameraPose {
    CameraPose {
        eye_millimeters: [0, -1_000, 6_000],
        target_millimeters: [0, 0, 1_000],
    }
}

fn frame_from(pose: CameraPose) -> FrameUniform {
    let view = SceneView::new(
        pose,
        camera(),
        Viewport {
            width: TARGET,
            height: TARGET,
        },
    )
    .expect("the fixture pose is well formed");
    FrameUniform::new(&view, Some(&effects())).expect("the fixture frame builds")
}

/// Renders one scene into a fresh target and returns the RGBA pixels.
/// Creates the colour target the tests read back.
fn color_target(device: &Device) -> Texture {
    device.create_texture(&TextureDescriptor {
        label: Some("test target"),
        size: Extent3d {
            width: TARGET,
            height: TARGET,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: TextureDimension::D2,
        format: COLOR_FORMAT,
        usage: TextureUsages::RENDER_ATTACHMENT | TextureUsages::COPY_SRC,
        view_formats: &[],
    })
}

/// Records one clear-and-draw pass over the supplied attachments.
fn record_scene(
    encoder: &mut CommandEncoder,
    color_view: &TextureView,
    depth_view: &TextureView,
    gpu: &SceneGpu,
    pipelines: &ScenePipelines,
) {
    let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
        label: Some("scene"),
        color_attachments: &[Some(RenderPassColorAttachment {
            view: color_view,
            depth_slice: None,
            resolve_target: None,
            ops: Operations {
                load: LoadOp::Clear(Color::BLACK),
                store: StoreOp::Store,
            },
        })],
        depth_stencil_attachment: Some(RenderPassDepthStencilAttachment {
            view: depth_view,
            depth_ops: Some(Operations {
                load: LoadOp::Clear(1.0),
                store: StoreOp::Store,
            }),
            stencil_ops: None,
        }),
        timestamp_writes: None,
        occlusion_query_set: None,
        multiview_mask: None,
    });
    gpu.draw(&mut pass, pipelines);
}

/// Waits for the queue to drain, then copies the mapped bytes out.
fn map_pixels(device: &Device, readback: &Buffer) -> Vec<u8> {
    let slice = readback.slice(..);
    slice.map_async(MapMode::Read, |result| {
        result.expect("the readback buffer maps");
    });
    device
        .poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: None,
        })
        .expect("the queue drains");
    let pixels = slice
        .get_mapped_range()
        .expect("the mapped range is available once the queue has drained")
        .to_vec();
    readback.unmap();
    pixels
}

fn render(context: &GpuContext, scene: &RenderScene) -> Vec<u8> {
    render_from(context, scene, pose())
}

fn render_from(context: &GpuContext, scene: &RenderScene, pose: CameraPose) -> Vec<u8> {
    render_frame(context, scene, frame_from(pose))
}

fn render_frame(context: &GpuContext, scene: &RenderScene, frame: FrameUniform) -> Vec<u8> {
    let device = context.device();
    let queue = context.queue();
    let pipelines = ScenePipelines::new(device, COLOR_FORMAT);
    let gpu =
        SceneGpu::upload(device, queue, scene, &pipelines).expect("the fixture scene uploads");
    gpu.set_frame(queue, frame);

    let color = color_target(device);
    let color_view = color.create_view(&TextureViewDescriptor::default());
    let depth = depth_texture(
        device,
        Viewport {
            width: TARGET,
            height: TARGET,
        },
    );
    let depth_view = depth.create_view(&TextureViewDescriptor::default());
    assert_eq!(depth.format(), DEPTH_FORMAT);

    let readback = device.create_buffer(&BufferDescriptor {
        label: Some("readback"),
        size: u64::from(TARGET) * u64::from(TARGET) * 4,
        usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });

    let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor { label: None });
    record_scene(&mut encoder, &color_view, &depth_view, &gpu, &pipelines);
    encoder.copy_texture_to_buffer(
        color.as_image_copy(),
        TexelCopyBufferInfo {
            buffer: &readback,
            layout: TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(TARGET * 4),
                rows_per_image: Some(TARGET),
            },
        },
        Extent3d {
            width: TARGET,
            height: TARGET,
            depth_or_array_layers: 1,
        },
    );
    queue.submit([encoder.finish()]);

    map_pixels(device, &readback)
}

/// Counts pixels that are not the clear colour.
fn lit_pixels(pixels: &[u8]) -> usize {
    pixels
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|texel| texel[0] > 0 || texel[1] > 0 || texel[2] > 0)
        .count()
}

#[test]
fn an_uploaded_scene_groups_its_objects_into_one_batch_per_mesh_and_material() {
    let context = context();
    let scene = scene(SceneAlphaMode::Opaque, false, 4);
    let pipelines = ScenePipelines::new(context.device(), COLOR_FORMAT);
    let gpu = SceneGpu::upload(context.device(), context.queue(), &scene, &pipelines)
        .expect("the fixture scene uploads");
    assert_eq!(gpu.batch_counts(), [1, 0, 0, 0]);
}

#[test]
fn a_masked_material_lands_in_its_own_pipeline_bucket() {
    // The bucket index decides which pipeline is bound, and therefore whether
    // the alpha-testing fragment entry point runs at all. A misfiled batch
    // would render foliage as opaque rectangles.
    let context = context();
    let scene = scene(
        SceneAlphaMode::Mask {
            cutoff_micro: 501_961,
        },
        true,
        1,
    );
    let pipelines = ScenePipelines::new(context.device(), COLOR_FORMAT);
    let gpu = SceneGpu::upload(context.device(), context.queue(), &scene, &pipelines)
        .expect("the fixture scene uploads");
    assert_eq!(gpu.batch_counts(), [0, 0, 0, 1]);
}

#[test]
fn terrain_no_longer_enters_the_material_batches_however_many_surfaces_it_names() {
    // Terrain used to be split into one batch per material, which is the fault
    // this port removes: a batch can only name one material, so the boundary
    // between two of them had to fall on a quad edge. It is now one mesh drawn
    // from its own pipeline, and the count that proves it is the batch
    // accounting staying flat as the palette grows. If terrain crept back into
    // the material buckets these totals would climb with the surface count.
    let context = context();
    let pipelines = ScenePipelines::new(context.device(), COLOR_FORMAT);
    for surfaces in 1..=3 {
        let scene = terrain_scene(surfaces);
        let gpu = SceneGpu::upload(context.device(), context.queue(), &scene, &pipelines)
            .expect("the fixture scene uploads");
        assert_eq!(
            gpu.batch_counts(),
            [1, 0, 0, 0],
            "{surfaces} terrain surfaces changed the material batching"
        );
    }
}

#[test]
fn terrain_reaches_the_target_rather_than_being_dropped_on_the_way() {
    // The terrain grid is four times the quad's extent, so a scene carrying it
    // must light strictly more pixels than the same scene without. A silently
    // empty vertex buffer, a transposed attribute layout or an index range
    // that never leaves zero would all leave the two counts equal.
    let context = context();
    let bare = lit_pixels(&render(&context, &scene(SceneAlphaMode::Opaque, false, 1)));
    let composed = lit_pixels(&render(&context, &terrain_scene(1)));
    assert!(
        composed > bare,
        "terrain lit {composed} pixels against {bare} without it"
    );
}

#[test]
fn drawing_one_lit_quad_produces_pixels_that_are_not_the_clear_colour() {
    // This is the first moment the whole chain runs: packed vertices feeding
    // declared attributes, an instance buffer feeding the model matrix, a
    // material bind group feeding the sampler, and the frame uniform feeding
    // the projection. Any disagreement among them leaves the target black.
    let context = context();
    let pixels = render(&context, &scene(SceneAlphaMode::Opaque, false, 1));
    let lit = lit_pixels(&pixels);
    assert!(
        lit > 0,
        "the quad rendered nothing; the target is entirely the clear colour"
    );
}

#[test]
fn the_quad_covers_a_substantial_share_of_the_frame_rather_than_a_stray_sliver() {
    // A projection or basis error usually still puts *something* on screen,
    // typically a degenerate edge-on sliver or a single corner. Requiring a
    // large share of the frame is what separates "drew the quad" from "drew a
    // few pixels of something".
    let context = context();
    let pixels = render(&context, &scene(SceneAlphaMode::Opaque, false, 1));
    let lit = lit_pixels(&pixels);
    let total = (TARGET * TARGET) as usize;
    assert!(
        lit * 10 > total,
        "only {lit} of {total} pixels were covered, which is too few for a two-metre quad at two metres"
    );
}

#[test]
fn a_single_sided_quad_wound_the_other_way_is_culled_entirely() {
    // Proves the front-face convention actually reaches the driver. If winding
    // were ignored, this scene would look identical to the one above and the
    // culling test would be vacuous.
    let context = context();
    let scene = build(
        white_texture(),
        material(SceneAlphaMode::Opaque, false),
        reversed_quad(),
        1,
    );
    let pixels = render(&context, &scene);
    assert_eq!(
        lit_pixels(&pixels),
        0,
        "a back-facing single-sided quad must be culled"
    );
}

#[test]
fn a_double_sided_quad_wound_the_other_way_still_renders() {
    let context = context();
    let scene = build(
        white_texture(),
        material(SceneAlphaMode::Opaque, true),
        reversed_quad(),
        1,
    );
    assert!(lit_pixels(&render(&context, &scene)) > 0);
}

#[test]
fn the_materials_own_albedo_reaches_the_shader_rather_than_the_placeholder() {
    // Four of the five material texture slots fall back to a neutral 1x1
    // placeholder, so a bind group that wired albedo to the placeholder too
    // would still render a plausible lit surface. Giving the albedo a colour
    // no placeholder carries is what makes that mistake visible: a red quad
    // proves the sampled texels came from the scene's own table.
    let context = context();
    let scene = build(
        texture(RED_565),
        material(SceneAlphaMode::Opaque, false),
        quad(),
        1,
    );
    let pixels = render(&context, &scene);
    // Widened before comparing: a saturated channel plus the margin overflows
    // u8 and would panic in debug rather than report a colour.
    let colored = pixels
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|texel| {
            let (red, green, blue) = (
                u32::from(texel[0]),
                u32::from(texel[1]),
                u32::from(texel[2]),
            );
            red > green + 32 && red > blue + 32
        })
        .count();
    let frame = usize::try_from(TARGET * TARGET).expect("the fixture frame fits");
    assert!(
        colored * 10 > frame,
        "only {colored} pixels were red-dominant; the albedo did not reach the shader"
    );
}

#[test]
fn an_unreadable_texture_names_the_offending_table_entry() {
    let context = context();
    let pipelines = ScenePipelines::new(context.device(), COLOR_FORMAT);
    let broken = build(
        SceneTexture::Encoded {
            encoding: TextureEncoding::DdsContainer,
            color_space: TextureColorSpace::Srgb,
            bytes: vec![0u8; 8].into(),
        },
        material(SceneAlphaMode::Opaque, false),
        quad(),
        1,
    );
    let error = SceneGpu::upload(context.device(), context.queue(), &broken, &pipelines)
        .expect_err("a truncated container cannot be prepared");
    assert!(matches!(error, DrawError::Texture { index: 0, .. }));
}

#[test]
fn a_yaw_locked_card_turns_to_face_a_camera_its_placement_faced_away_from() {
    let context = context();
    // The placement is a half turn, so the authored card faces due north while
    // the camera stands to the south. Single-sided, that is not a dim card but
    // no card at all, which is what makes the pair of counts decisive.
    let placed = lit_pixels(&render(&context, &card_scene(None, turned_away())));
    assert_eq!(placed, 0, "the placement points the card away from the eye");

    let billboarded = lit_pixels(&render(
        &context,
        &card_scene(Some(BillboardPolicy::YawLocked), turned_away()),
    ));
    assert!(
        billboarded > 0,
        "a yaw-locked card must replace its authored rotation, not compose with it"
    );
}

#[test]
fn crossed_quads_are_drawn_as_placed_rather_than_turned_toward_the_camera() {
    let context = context();
    // Crossed quads earn their second quad precisely by not rotating: the
    // policy is a description of the geometry, not a request to face the eye.
    // Were it treated as a camera-facing mode, this count would rise above
    // zero and the two quads would swing together into one doubled surface.
    let drawn = lit_pixels(&render(
        &context,
        &card_scene(Some(BillboardPolicy::CrossQuads), turned_away()),
    ));
    assert_eq!(drawn, 0, "crossed quads keep the rotation they were given");
}

#[test]
fn a_spherical_card_tilts_toward_an_elevated_camera_while_a_yaw_locked_one_stays_upright() {
    let context = context();
    // Seen from well above, a card pinned upright is foreshortened to roughly
    // the cosine of the elevation, while one free to pitch presents its whole
    // face. Counting covered pixels separates the two axes of freedom without
    // asserting an exact projection.
    let yaw_locked = lit_pixels(&render_from(
        &context,
        &card_scene(Some(BillboardPolicy::YawLocked), turned_away()),
        overhead_pose(),
    ));
    let spherical = lit_pixels(&render_from(
        &context,
        &card_scene(Some(BillboardPolicy::Spherical), turned_away()),
        overhead_pose(),
    ));

    assert!(
        yaw_locked > 0,
        "the upright card still faces the eye in yaw"
    );
    assert!(
        spherical > yaw_locked * 2,
        "a spherical card should present far more of itself than an upright one: \
         spherical {spherical}, yaw-locked {yaw_locked}"
    );
}
