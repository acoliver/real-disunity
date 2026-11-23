//! A synthetic outdoor scene: hilly terrain, a few thousand instanced props over a few dozen
//! materials, and a sun with three camera-split shadow cascades. No game assets are involved.
use worldrunner_render_plan::{
    MaterialHandle, MeshHandle, RenderScene, SceneAlphaMode, SceneAmbient, SceneAntiAlias,
    SceneBloom, SceneCamera, SceneColor, SceneColorGrading, SceneDirection, SceneEffects,
    SceneEffectsParts, SceneGeometry, SceneHdrPost, SceneMaterial, SceneMaterialParts, SceneMesh,
    SceneMeshLod, SceneObject, SceneParts, SceneResources, SceneRotation, SceneShadowCascades,
    SceneSky, SceneSun, SceneTerrain, SceneTerrainAppearance, SceneTerrainParts,
    SceneTerrainSurface, SceneTexture, SceneTransform, SceneValidationError, SceneVertex,
    TerrainLodPolicy, TexelFormat, TextureColorSpace, TextureHandle,
};

const MICRO: i64 = 1_000_000;
const MESH_COUNT: usize = 24;
const PROP_MATERIALS: usize = 40;
const TERRAIN_SAMPLES: u32 = 129;
const TERRAIN_SPACING_MM: i64 = 2_000;

/// A triangle wave of `period` and `amplitude`, in the integer units of its inputs.
fn wave(position: i64, period: i64, amplitude: i64) -> i64 {
    let phase = position.rem_euclid(period);
    (phase - period / 2).abs() * amplitude * 2 / period
}

/// Terrain height in millimetres at a position in millimetres east and north of the origin.
pub fn ground_height(east: i64, north: i64) -> i64 {
    wave(east, 80_000, 3_000) + wave(north, 110_000, 2_500) + wave(east + north, 45_000, 1_200)
}

pub fn lens() -> SceneCamera {
    SceneCamera {
        vertical_fov_millidegrees: 60_000,
        near_plane_millimeters: 100,
        far_plane_millimeters: 400_000,
    }
}

/// Deterministic pseudo-random stream, so every run draws the same scene.
pub struct Stream(u64);
impl Stream {
    pub fn new(seed: u64) -> Self {
        Self(seed)
    }
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        self.0 >> 33
    }
    pub fn below(&mut self, count: usize) -> u32 {
        let count = u64::try_from(count).expect("small count");
        u32::try_from(self.next() % count).expect("below a small count")
    }
    pub fn range(&mut self, low: i64, high: i64) -> i64 {
        let span = u64::try_from(high - low).expect("ordered bounds");
        low + i64::try_from(self.next() % span).expect("below the span")
    }
}

/// Cosine and sine in micro-units at sixteen even steps of a full turn.
pub const CIRCLE: [(i64, i64); 16] = [
    (1_000_000, 0),
    (923_880, 382_683),
    (707_107, 707_107),
    (382_683, 923_880),
    (0, 1_000_000),
    (-382_683, 923_880),
    (-707_107, 707_107),
    (-923_880, 382_683),
    (-1_000_000, 0),
    (-923_880, -382_683),
    (-707_107, -707_107),
    (-382_683, -923_880),
    (0, -1_000_000),
    (382_683, -923_880),
    (707_107, -707_107),
    (923_880, -382_683),
];

/// A tapering prism with counter-clockwise outward faces: `rings` bands of `segments` quads.
fn prism(
    rings: u32,
    segments: usize,
    radius_mm: i64,
    height_mm: i64,
) -> (Vec<SceneVertex>, Vec<u32>) {
    let stride = segments + 1;
    let mut vertices = Vec::new();
    for ring in 0..=rings {
        let remaining = i64::from(rings + 1 - ring);
        let radius = radius_mm * remaining / i64::from(rings + 1);
        for segment in 0..=segments {
            let (cosine, sine) = CIRCLE[segment % segments * (16 / segments)];
            vertices.push(SceneVertex {
                tangent_micro: None,
                position_millimeters: [
                    radius * cosine / MICRO,
                    radius * sine / MICRO,
                    height_mm * i64::from(ring) / i64::from(rings),
                ],
                normal_micro: [cosine, sine, 0],
                texcoord_micro: [
                    MICRO * i64::try_from(segment).expect("small")
                        / i64::try_from(segments).expect("small"),
                    MICRO * i64::from(ring) / i64::from(rings),
                ],
            });
        }
    }
    let stride = u32::try_from(stride).expect("small");
    let mut indices = Vec::new();
    for ring in 0..rings {
        for segment in 0..stride - 1 {
            let a = ring * stride + segment;
            let (b, c, d) = (a + 1, a + stride, a + stride + 1);
            indices.extend([a, b, c, b, d, c]);
        }
    }
    (vertices, indices)
}

fn mesh(index: usize) -> Result<SceneMesh, SceneValidationError> {
    let step = i64::try_from(index).expect("small");
    let (radius, height) = (500 + 120 * (step % 5), 1_500 + 400 * (step % 4));
    let segments = [8, 16, 16, 8][index % 4];
    let rings = 2 + u32::try_from(index % 7).expect("small");
    let level = |level, rings, segments| {
        let (vertices, indices) = prism(rings, segments, radius, height);
        SceneMeshLod::new(level, vertices, indices, None)
    };
    SceneMesh::with_distances(
        vec![level(0, rings, segments)?, level(1, 1, 8)?],
        vec![40_000, 400_000],
    )
}

fn texture(index: usize) -> SceneTexture {
    let tint = |channel: usize| {
        u8::try_from((index * (37 + channel * 29) + 40) % 200 + 40).expect("bounded")
    };
    let texels = (0..64_u8)
        .flat_map(|texel| {
            [
                tint(0) ^ texel,
                tint(1),
                tint(2),
                if texel % 5 == 0 { 0 } else { 255 },
            ]
        })
        .collect::<Vec<u8>>();
    SceneTexture::Texels {
        format: TexelFormat::Rgba8Unorm,
        color_space: TextureColorSpace::Srgb,
        width: 8,
        height: 8,
        mips: 1,
        bytes: texels.into(),
    }
}

fn map(bytes: [u8; 4]) -> SceneTexture {
    SceneTexture::Texels {
        format: TexelFormat::Rgba8Unorm,
        color_space: TextureColorSpace::Linear,
        width: 1,
        height: 1,
        mips: 1,
        bytes: Vec::from(bytes).into(),
    }
}

fn material(index: usize) -> Result<SceneMaterial, SceneValidationError> {
    let masked = index % 10 == 9;
    SceneMaterial::new(SceneMaterialParts {
        albedo: TextureHandle::from_raw(u32::try_from(index).expect("small")),
        normal: None,
        roughness_metallic: None,
        ambient_occlusion: None,
        self_illumination: None,
        roughness_micro: 300_000 + u32::try_from(index % 7).expect("small") * 100_000,
        metalness_micro: 0,
        alpha_mode: if masked {
            SceneAlphaMode::Mask {
                cutoff_micro: 500_000,
            }
        } else {
            SceneAlphaMode::Opaque
        },
        double_sided: masked,
    })
}

fn resources() -> Result<SceneResources, SceneValidationError> {
    let mut textures: Vec<_> = (0..PROP_MATERIALS).map(texture).collect();
    textures.extend([map([128, 128, 255, 255]), map([0, 255, 0, 255])]);
    let mut materials = (0..PROP_MATERIALS)
        .map(material)
        .collect::<Result<Vec<_>, _>>()?;
    materials.push(SceneMaterial::new(SceneMaterialParts {
        albedo: TextureHandle::from_raw(0),
        normal: Some(TextureHandle::from_raw(40)),
        roughness_metallic: Some(TextureHandle::from_raw(41)),
        ambient_occlusion: None,
        self_illumination: None,
        roughness_micro: 1_000_000,
        metalness_micro: 0,
        alpha_mode: SceneAlphaMode::Opaque,
        double_sided: false,
    })?);
    SceneResources::new(textures, materials)
}

fn terrain() -> Result<SceneTerrain, SceneValidationError> {
    let half = i64::from(TERRAIN_SAMPLES / 2) * TERRAIN_SPACING_MM;
    let heights = (0..TERRAIN_SAMPLES)
        .flat_map(|row| {
            (0..TERRAIN_SAMPLES).map(move |column| {
                let east = i64::from(column) * TERRAIN_SPACING_MM - half;
                let north = i64::from(row) * TERRAIN_SPACING_MM - half;
                i32::try_from(ground_height(east, north)).expect("bounded")
            })
        })
        .collect();
    SceneTerrain::new(SceneTerrainParts {
        columns: TERRAIN_SAMPLES,
        rows: TERRAIN_SAMPLES,
        origin_east_millimeters: -half,
        origin_north_millimeters: -half,
        sample_spacing_millimeters: u32::try_from(TERRAIN_SPACING_MM).expect("positive"),
        heights_millimeters: heights,
        surface_indices: vec![
            0;
            usize::try_from(TERRAIN_SAMPLES * TERRAIN_SAMPLES).expect("small")
        ],
        surfaces: vec![SceneTerrainSurface::new(
            MaterialHandle::from_raw(u32::try_from(PROP_MATERIALS).expect("small")),
            SceneTerrainAppearance::NEUTRAL,
        )],
        lod_policy: TerrainLodPolicy::FullResolution,
    })
}

/// A prop standing on the ground at `east`, `north` with a random heading and size, whose mesh
/// and material `choose` picks.
fn prop(
    stream: &mut Stream,
    (east, north): (i64, i64),
    choose: impl FnOnce(&mut Stream) -> (u32, u32),
) -> SceneObject {
    let (w, z) = CIRCLE[usize::try_from(stream.range(0, 8)).expect("small")];
    let transform = SceneTransform {
        position_millimeters: [east, north, ground_height(east, north)],
        rotation: SceneRotation { x: 0, y: 0, z, w },
        scale_millionths: u64::try_from(stream.range(700_000, 1_400_000)).expect("positive"),
    };
    let (mesh, material) = choose(stream);
    SceneObject::new(
        transform,
        MeshHandle::from_raw(mesh),
        MaterialHandle::from_raw(material),
    )
}

/// Half the side of the square of terrain the props stand on, in millimetres.
fn half_extent() -> i64 {
    i64::from(TERRAIN_SAMPLES / 2 - 4) * TERRAIN_SPACING_MM
}

fn objects(count: usize, clutter: usize) -> Vec<SceneObject> {
    let mut stream = Stream::new(0x5eed);
    let half = half_extent();
    let props = (0..count).map(|_| {
        let at = (stream.range(-half, half), stream.range(-half, half));
        prop(&mut stream, at, |stream| {
            (stream.below(MESH_COUNT), stream.below(PROP_MATERIALS))
        })
    });
    let mut clutter_stream = Stream::new(0xc1u64);
    let pieces = (0..clutter).map(|_| {
        let at = (
            clutter_stream.range(-half, half),
            clutter_stream.range(-half, half),
        );
        self::clutter(&mut clutter_stream, at)
    });
    props.chain(pieces).collect()
}

/// How many kinds of clutter there are. Each kind is one mesh with one material, so clutter
/// fills a few large instance batches, as streamed grass and debris do.
const CLUTTER_KINDS: usize = 8;

/// A piece of clutter of a random kind at `at`.
pub fn clutter(stream: &mut Stream, at: (i64, i64)) -> SceneObject {
    prop(stream, at, |stream| {
        let kind = stream.below(CLUTTER_KINDS);
        (kind, kind * 5)
    })
}

/// Clutter `radius` millimetres around `center` at most, clamped to the terrain.
pub fn clutter_near(stream: &mut Stream, center: (i64, i64), radius: i64) -> SceneObject {
    let half = half_extent();
    let at = (
        (center.0 + stream.range(-radius, radius)).clamp(-half, half),
        (center.1 + stream.range(-radius, radius)).clamp(-half, half),
    );
    clutter(stream, at)
}

/// Sun, even ambient light, three camera-split cascades, and an HDR bloom plus FXAA chain.
pub fn effects() -> Result<SceneEffects, SceneValidationError> {
    let white = SceneColor {
        red: 255,
        green: 255,
        blue: 255,
    };
    let effects = SceneEffects::new(SceneEffectsParts {
        water: None,
        sky: SceneSky::ProceduralHemisphere {
            zenith: white,
            horizon: white,
        },
        sun: SceneSun {
            direction: SceneDirection::new([600_000, 0, 800_000])?,
            irradiance_micro: 2_000_000,
        },
        ambient: SceneAmbient {
            sky_irradiance_micro: 300_000,
            ground_irradiance_micro: 150_000,
        },
        shadows: SceneShadowCascades::camera_splits(vec![10_000, 40_000, 150_000], 100_000)?,
        ssao: None,
        camera: lens(),
        grading: SceneColorGrading {
            exposure_micro: 1_000_000,
            contrast_micro: 1_000_000,
            saturation_micro: 1_000_000,
        },
        post: Vec::new(),
        anti_alias: SceneAntiAlias::Disabled,
    })?;
    effects.with_hdr_post(SceneHdrPost {
        tone_map: None,
        grading: None,
        adaptive_exposure: None,
        bloom: Some(SceneBloom {
            threshold_micro: 1_000_000,
            soft_knee_micro: 500_000,
            intensity_micro: 300_000,
            levels: 5,
            recipe: None,
        }),
        fxaa_subpixel_micro: Some(750_000),
        fxaa_quality: None,
    })
}

pub fn scene(object_count: usize, clutter: usize) -> Result<RenderScene, SceneValidationError> {
    let meshes = (0..MESH_COUNT).map(mesh).collect::<Result<Vec<_>, _>>()?;
    RenderScene::new(SceneParts {
        resources: resources()?,
        geometry: SceneGeometry::new(meshes, objects(object_count, clutter))?,
        terrain: Some(terrain()?),
        effects: Some(effects()?),
    })
}
