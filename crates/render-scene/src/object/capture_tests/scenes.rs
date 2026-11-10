//! The object scenes, cameras and frame the captures draw.
//!
//! Each scene is a grid of upright cards, one per material, in front of a camera. The atlas
//! has four colour frames and an alpha ramp in every frame, so a masked cutoff, a coverage
//! edge and an impostor frame blend all show in the pixels. Nothing here draws or compares.

use worldrunner_render_plan::{
    BillboardPolicy, MaterialHandle, MeshHandle, ROTATION_MICRO_SCALE, RenderScene, SceneAlphaMode,
    SceneAmbientOcclusion, SceneCamera, SceneDirection, SceneEffects, SceneEffectsParts,
    SceneGeometry, SceneImpostor, SceneImpostorLayout, SceneMaterial, SceneMaterialFeatures,
    SceneMaterialParts, SceneMesh, SceneMeshLod, SceneObject, SceneParts, SceneResources,
    SceneRotation, SceneSun, SceneTexture, SceneTint, SceneTransform, SceneVertex, SceneWind,
    TexelFormat, TextureColorSpace, TextureHandle,
};

use super::SIZE;
use crate::frame::FrameUniform;
use crate::{CameraPose, SceneView, Viewport};

const MICRO: i64 = 1_000_000;
/// Distance between neighbouring cards, in millimetres.
const SPACING: i64 = 1_400;
/// Cards stand on a row 1.5 m above the one below.
const ROW: i64 = 1_500;
/// Scale of a card, in millionths.
const SCALE: u64 = 620_000;
/// How far the previous-frame placement of a moving card is from its current one.
pub(in crate::object) const MOTION_MILLIMETERS: i64 = 150;

/// How a card is sampled.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(in crate::object) enum Sampling {
    /// Its own texture coordinates.
    #[default]
    Surface,
    /// A frame of an impostor atlas.
    Impostor,
    /// A frame of an impostor atlas displaced by its relief depth.
    Relief,
}

/// How a card's alpha is used.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(in crate::object) enum Alpha {
    #[default]
    Opaque,
    /// Fragments below the cutoff are discarded.
    Cutout,
    /// The cutout, also turned into covered samples when the pass is multisampled.
    Coverage,
}

/// What a material needs, which together select its `PipelineKey`.
#[derive(Clone, Copy, Debug, Default)]
pub(in crate::object) struct Spec {
    pub sampling: Sampling,
    pub alpha: Alpha,
    pub double_sided: bool,
    pub wind: bool,
}

impl Spec {
    const fn impostor(self) -> bool {
        !matches!(self.sampling, Sampling::Surface)
    }

    const fn masked(self) -> bool {
        !matches!(self.alpha, Alpha::Opaque)
    }
}

fn texels(
    format: TexelFormat,
    space: TextureColorSpace,
    [width, height]: [u32; 2],
    bytes: Vec<u8>,
) -> SceneTexture {
    SceneTexture::Texels {
        format,
        color_space: space,
        width,
        height,
        mips: 1,
        bytes: bytes.into(),
    }
}

/// Four frames of 8 by 8 texels, each a distinct colour with alpha rising across it.
fn atlas() -> SceneTexture {
    let colors: [[u8; 3]; 4] = [[220, 60, 40], [50, 200, 70], [60, 80, 230], [240, 230, 200]];
    let mut bytes = Vec::new();
    for _row in 0..8u8 {
        for color in colors {
            for column in 0..8u8 {
                bytes.extend_from_slice(&[color[0], color[1], color[2], 36 * column]);
            }
        }
    }
    texels(
        TexelFormat::Rgba8Unorm,
        TextureColorSpace::Srgb,
        [32, 8],
        bytes,
    )
}

/// A 32 by 8 linear map whose texels follow `value(column, row)`.
fn map(space: TextureColorSpace, value: impl Fn(u8, u8) -> [u8; 4]) -> SceneTexture {
    let mut bytes = Vec::new();
    for row in 0..8u8 {
        for column in 0..32u8 {
            bytes.extend_from_slice(&value(column, row));
        }
    }
    texels(TexelFormat::Rgba8Unorm, space, [32, 8], bytes)
}

/// The textures: albedo atlas, normal, roughness and metalness, relief depth, ambient
/// occlusion and self-illumination.
pub(in crate::object) fn textures() -> Vec<SceneTexture> {
    vec![
        atlas(),
        map(TextureColorSpace::Linear, |column, row| {
            [118 + (column % 8) * 3, 140 - row * 3, 250, 255]
        }),
        map(TextureColorSpace::Linear, |column, row| {
            [60 + (column % 8) * 20, 30 + row * 20, 0, 255]
        }),
        map(TextureColorSpace::Linear, |column, _| {
            [(column % 8) * 30, 0, 0, 255]
        }),
        map(TextureColorSpace::Linear, |column, row| {
            [255 - (column % 8) * 10, 255 - row * 8, 255, 255]
        }),
        map(TextureColorSpace::Srgb, |column, _| {
            [if column % 8 > 5 { 200 } else { 0 }, 80, 0, 255]
        }),
    ]
}

fn impostor(relief: bool) -> SceneImpostor {
    let impostor = SceneImpostor::new(SceneImpostorLayout::AzimuthElevation, [4, 1], 0, true)
        .expect("a four by one grid is valid");
    if relief {
        impostor
            .with_relief(TextureHandle::from_raw(3), 310)
            .expect("a nonzero relief scale is valid")
    } else {
        impostor
    }
}

pub(in crate::object) fn material(spec: Spec) -> SceneMaterial {
    let surface = !spec.impostor();
    let mut material = SceneMaterial::new(SceneMaterialParts {
        albedo: TextureHandle::from_raw(0),
        normal: (surface || spec.sampling == Sampling::Relief).then(|| TextureHandle::from_raw(1)),
        roughness_metallic: surface.then(|| TextureHandle::from_raw(2)),
        ambient_occlusion: (surface && spec.double_sided).then(|| TextureHandle::from_raw(4)),
        self_illumination: (surface && spec.masked()).then(|| TextureHandle::from_raw(5)),
        roughness_micro: 800_000,
        metalness_micro: 100_000,
        alpha_mode: if spec.masked() {
            SceneAlphaMode::Mask {
                cutoff_micro: 500_000,
            }
        } else {
            SceneAlphaMode::Opaque
        },
        double_sided: spec.double_sided,
    })
    .expect("the fixture material carries in-range scalars");
    if spec.alpha == Alpha::Coverage {
        material = material
            .with_alpha_to_coverage(true)
            .expect("coverage needs a masked material");
    }
    if spec.impostor() {
        material = material.with_impostor(impostor(spec.sampling == Sampling::Relief));
    }
    if spec.wind {
        let wind = SceneWind::new(
            SceneDirection::new([1_000_000, 0, 0]).expect("a unit direction"),
            200,
            1_000,
            0,
        )
        .expect("the fixture wind is valid");
        material = material
            .with_features(SceneMaterialFeatures {
                tint: SceneTint::WHITE,
                ambient_occlusion: SceneAmbientOcclusion::UNOCCLUDED,
                wind: Some(wind),
                layer: None,
                translucency: None,
            })
            .expect("the fixture features are valid");
    }
    material
}

fn vertex(position: [i64; 3], texcoord: [i64; 2]) -> SceneVertex {
    SceneVertex {
        tangent_micro: None,
        position_millimeters: position,
        normal_micro: [0, -MICRO, 0],
        texcoord_micro: texcoord,
    }
}

/// A two-metre card standing on the ground plane and facing local `-Y`, as the camera sees it.
fn card(billboard: Option<BillboardPolicy>) -> SceneMesh {
    let half = 1_000;
    SceneMesh::new(vec![
        SceneMeshLod::new(
            0,
            vec![
                vertex([-half, 0, 0], [0, MICRO]),
                vertex([half, 0, 0], [MICRO, MICRO]),
                vertex([half, 0, 2 * half], [MICRO, 0]),
                vertex([-half, 0, 2 * half], [0, 0]),
            ],
            vec![0, 1, 2, 0, 2, 3],
            billboard,
        )
        .expect("the fixture card is a valid level"),
    ])
    .expect("a one-level chain is valid")
}

/// Where card `index` of `count` stands, `shift` millimetres along x of its place in the grid.
pub(in crate::object) fn placement(index: usize, count: usize, shift: i64) -> SceneTransform {
    let columns = count.div_ceil(2);
    let (column, row) = (index % columns, index / columns);
    let across =
        i64::try_from(column).expect("a few columns") * 2 - i64::try_from(columns).unwrap() + 1;
    SceneTransform {
        position_millimeters: [
            across * SPACING / 2 + shift,
            0,
            i64::try_from(row).expect("two rows") * ROW,
        ],
        rotation: SceneRotation {
            x: 0,
            y: 0,
            z: 0,
            w: ROTATION_MICRO_SCALE,
        },
        scale_millionths: SCALE,
    }
}

fn effects() -> SceneEffects {
    SceneEffects::new(SceneEffectsParts {
        // The sun shines onto the faces the camera sees, so lighting differs between materials.
        sun: SceneSun {
            direction: SceneDirection::new([360_000, -480_000, 800_000]).expect("a direction"),
            irradiance_micro: 2_000_000,
        },
        ..crate::draw::tests::effect_parts()
    })
    .expect("the fixture effects validate")
}

/// A scene of one card per spec.
pub(in crate::object) fn scene(specs: &[Spec]) -> RenderScene {
    let objects = specs
        .iter()
        .enumerate()
        .map(|(index, spec)| {
            SceneObject::new(
                placement(index, specs.len(), 0),
                MeshHandle::from_raw(u32::from(spec.impostor())),
                MaterialHandle::from_raw(u32::try_from(index).expect("few materials")),
            )
        })
        .collect();
    RenderScene::new(SceneParts {
        resources: SceneResources::new(textures(), specs.iter().copied().map(material).collect())
            .expect("the fixture resources validate"),
        geometry: SceneGeometry::new(
            vec![card(None), card(Some(BillboardPolicy::YawLocked))],
            objects,
        )
        .expect("the fixture geometry validates"),
        terrain: None,
        effects: Some(effects()),
    })
    .expect("the fixture scene validates")
}

/// `count` cards that share one material, and so one batch.
pub(in crate::object) fn shared(spec: Spec, count: usize) -> RenderScene {
    let objects = (0..count)
        .map(|index| {
            SceneObject::new(
                placement(index, count, 0),
                MeshHandle::from_raw(u32::from(spec.impostor())),
                MaterialHandle::from_raw(0),
            )
        })
        .collect();
    RenderScene::new(SceneParts {
        resources: SceneResources::new(textures(), vec![material(spec)])
            .expect("the fixture resources validate"),
        geometry: SceneGeometry::new(
            vec![card(None), card(Some(BillboardPolicy::YawLocked))],
            objects,
        )
        .expect("the fixture geometry validates"),
        terrain: None,
        effects: Some(effects()),
    })
    .expect("the fixture scene validates")
}

const fn lit(alpha: Alpha, double_sided: bool) -> Spec {
    Spec {
        sampling: Sampling::Surface,
        alpha,
        double_sided,
        wind: false,
    }
}

/// Lit objects and foliage: opaque and cutout cards, single and double sided, and the cutout
/// cards with coverage, the double sided ones in wind.
pub(in crate::object) fn surface_specs() -> Vec<Spec> {
    let foliage = |alpha| Spec {
        wind: true,
        ..lit(alpha, true)
    };
    vec![
        lit(Alpha::Opaque, false),
        lit(Alpha::Opaque, true),
        lit(Alpha::Cutout, false),
        lit(Alpha::Coverage, false),
        foliage(Alpha::Cutout),
        foliage(Alpha::Coverage),
    ]
}

/// Impostor cards, with and without relief, opaque and cutout, with coverage on the cutout ones.
pub(in crate::object) fn impostor_specs(relief: bool) -> Vec<Spec> {
    let sampling = if relief {
        Sampling::Relief
    } else {
        Sampling::Impostor
    };
    [
        (Alpha::Opaque, false),
        (Alpha::Opaque, true),
        (Alpha::Cutout, false),
        (Alpha::Cutout, true),
        (Alpha::Coverage, false),
        (Alpha::Coverage, true),
    ]
    .map(|(alpha, double_sided)| Spec {
        sampling,
        alpha,
        double_sided,
        wind: false,
    })
    .to_vec()
}

/// The eight cards of cutoff coverage without relief that a pipeline-index remap carries to
/// every other key: impostor or surface, masked or opaque, single or double sided.
pub(in crate::object) fn base_specs() -> Vec<Spec> {
    (0..8)
        .map(|bits| Spec {
            sampling: if bits & 4 != 0 {
                Sampling::Impostor
            } else {
                Sampling::Surface
            },
            alpha: if bits & 2 != 0 {
                Alpha::Cutout
            } else {
                Alpha::Opaque
            },
            double_sided: bits & 1 != 0,
            wind: false,
        })
        .collect()
}

pub(in crate::object) fn pose() -> CameraPose {
    CameraPose {
        eye_millimeters: [0, -5_000, 1_100],
        target_millimeters: [0, 0, 1_100],
    }
}

/// The frame uniform of `pose` over the `SIZE` by `SIZE` targets.
pub(in crate::object) fn frame(pose: CameraPose) -> FrameUniform {
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
    FrameUniform::new(&view, Some(&effects())).expect("the fixture frame builds")
}
