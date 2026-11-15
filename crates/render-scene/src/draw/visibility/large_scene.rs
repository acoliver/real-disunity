//! A synthetic open world for selection equality tests and the selection timing probe.
//!
//! Placements spread over a 2.4 km square, mostly in clusters like woods, with several
//! meshes whose levels draw through material slots, impostor cards with fade bands and
//! per-placement terminal fades. Views follow a walking camera with four shadow cascades.
use glam::{Mat4, Vec3};
use worldrunner_render_plan::*;

use super::super::slice_hull::SliceHull;
use super::tests::scene;

/// Deterministic generator, so every run builds the same world.
pub(super) struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Self(seed)
    }

    pub fn next(&mut self) -> u64 {
        // SplitMix64.
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    pub fn below(&mut self, bound: u32) -> u32 {
        u32::try_from(self.next() % u64::from(bound)).expect("below a u32 bound")
    }

    pub fn index(&mut self, len: usize) -> usize {
        let len = u64::try_from(len).expect("length fits u64");
        usize::try_from(self.next() % len).expect("below a usize length")
    }

    /// Whole millimetres in `[low, low + span)`.
    pub fn millimeters(&mut self, low: i64, span: u32) -> i64 {
        low + i64::from(self.below(span))
    }

    /// Uniform in `[low, high)`, in steps of 1/65536 of the range.
    pub fn range(&mut self, low: f32, high: f32) -> f32 {
        let unit = f32::from(u16::try_from(self.next() >> 48).expect("16 bits")) / 65_536.0;
        low + (high - low) * unit
    }

    pub fn chance(&mut self, percent: u32) -> bool {
        self.below(100) < percent
    }
}

const HALF_EXTENT_MILLIMETERS: i64 = 1_200_000;
/// Yaw rotations in millionths, as `(z, w)` quaternion parts.
const YAWS: [(i64, i64); 8] = [
    (0, 1_000_000),
    (382_683, 923_880),
    (707_107, 707_107),
    (923_880, 382_683),
    (1_000_000, 0),
    (923_880, -382_683),
    (707_107, -707_107),
    (382_683, -923_880),
];
/// Plain, fading-in, terminal and complement materials; see `materials`.
const MATERIALS: u32 = 8;

fn vertex(position: [i64; 3]) -> SceneVertex {
    SceneVertex {
        tangent_micro: None,
        position_millimeters: position,
        normal_micro: [0, 0, 1_000_000],
        texcoord_micro: [0; 2],
    }
}

/// A box-shaped level of `size` millimetres standing on its origin.
fn level(index: u8, size: [i64; 3]) -> SceneMeshLod {
    let [x, y, z] = size;
    SceneMeshLod::new(
        index,
        vec![vertex([-x / 2, -y / 2, 0]), vertex([x / 2, y / 2, z])],
        vec![0, 1, 0, 1, 0, 1],
        None,
    )
    .unwrap()
}

fn slot(material: u32, indices: std::ops::Range<u32>) -> SceneLodMaterialSlot {
    SceneLodMaterialSlot::new(MaterialHandle::from_raw(material), indices).unwrap()
}

/// Mesh `index`: two box levels and a yaw-locked card, with material slots on some meshes.
fn mesh(index: u32, rng: &mut Rng) -> SceneMesh {
    let width = 500 + rng.below(8_000);
    let height = 500 + rng.below(20_000);
    let size = [width, width, height].map(i64::from);
    let mut near = level(0, size);
    let mut middle = level(1, size);
    if index.is_multiple_of(2) {
        near = near
            .with_material_slots(vec![slot(0, 0..3), slot(2 + index % 3, 3..6)])
            .unwrap();
        middle = middle
            .with_material_slots(vec![slot(1, 0..3), slot(5 + index % 3, 3..6)])
            .unwrap();
    }
    let mut far = SceneMeshLod::new(
        2,
        middle.vertices().to_vec(),
        middle.indices().to_vec(),
        Some(BillboardPolicy::YawLocked),
    )
    .unwrap()
    .with_card(SceneCard::new(BillboardPolicy::YawLocked, [width, height], 500_000).unwrap())
    .unwrap();
    if !index.is_multiple_of(3) {
        far = far
            .with_material_slots(vec![slot(2 + index % 4, 0..6)])
            .unwrap();
    }
    let first = 20_000 + u64::from(rng.below(60_000));
    let second = first * 3 + u64::from(rng.below(100_000));
    SceneMesh::with_distances(vec![near, middle, far], vec![first, second, second * 4]).unwrap()
}

fn band(start_meters: u32, width_meters: u32, terminal: bool) -> SceneFadeBand {
    let band =
        SceneFadeBand::new(start_meters * 1000, (start_meters + width_meters) * 1000).unwrap();
    if terminal {
        band.with_terminal_fade()
    } else {
        band
    }
}

fn atlas(band: SceneFadeBand) -> SceneImpostor {
    SceneImpostor::new(SceneImpostorLayout::AzimuthElevation, [1, 1], 0, false)
        .unwrap()
        .with_fade(band)
}

fn materials(base: &SceneMaterial) -> Vec<SceneMaterial> {
    let base = *base;
    vec![
        base,
        base,
        base.with_impostor(atlas(band(30, 20, false))),
        base.with_impostor(atlas(band(120, 30, true))),
        base.with_impostor(atlas(band(250, 50, true)))
            .with_fade_direction(SceneFadeDirection::Keep),
        base.with_impostor(atlas(band(80, 40, false)))
            .with_fade_direction(SceneFadeDirection::Complement),
        base.with_impostor(atlas(band(400, 100, true))),
        base.with_impostor(atlas(band(60, 0, true))),
    ]
}

fn position(rng: &mut Rng, clusters: &[[i64; 2]]) -> [i64; 3] {
    let [x, y] = if rng.chance(70) {
        let [x, y] = clusters[rng.index(clusters.len())];
        [
            rng.millimeters(x - 40_000, 80_000),
            rng.millimeters(y - 40_000, 80_000),
        ]
    } else {
        [(); 2].map(|()| rng.millimeters(-HALF_EXTENT_MILLIMETERS, 2_400_000))
    };
    [x, y, rng.millimeters(0, 30_000)]
}

fn placement(rng: &mut Rng, clusters: &[[i64; 2]], meshes: u32) -> SceneObject {
    let (z, w) = YAWS[rng.index(YAWS.len())];
    let transform = SceneTransform {
        position_millimeters: position(rng, clusters),
        rotation: SceneRotation { x: 0, y: 0, z, w },
        scale_millionths: 500_000 + u64::from(rng.below(1_500_000)),
    };
    let mesh = rng.below(meshes);
    // Each mesh draws with a handful of materials, as a game's prefabs do.
    let material = (mesh * 3 + rng.below(3)) % MATERIALS;
    let mut object = SceneObject::new(
        transform,
        MeshHandle::from_raw(mesh),
        MaterialHandle::from_raw(material),
    )
    .with_casts_shadow(rng.chance(85));
    if rng.chance(40) {
        let start = 40 + rng.below(360);
        object = object.with_fade(band(start, 5 + rng.below(40), rng.chance(80)));
    }
    if rng.chance(20) {
        object = object
            .with_lod_distance_scale_millionths(500_000 + rng.below(1_500_000))
            .unwrap();
    }
    object
}

/// A world of `placements` objects drawn with `meshes` meshes and eight materials.
pub(super) fn world(placements: usize, meshes: u32, seed: u64) -> RenderScene {
    let base = scene();
    let mut rng = Rng::new(seed);
    let clusters: Vec<[i64; 2]> = (0..400)
        .map(|_| [(); 2].map(|()| rng.millimeters(-HALF_EXTENT_MILLIMETERS, 2_400_000)))
        .collect();
    let mesh_table: Vec<SceneMesh> = (0..meshes).map(|index| mesh(index, &mut rng)).collect();
    let objects = (0..placements)
        .map(|_| placement(&mut rng, &clusters, meshes))
        .collect();
    RenderScene::new(SceneParts {
        resources: SceneResources::new(
            base.resources().textures().to_vec(),
            materials(&base.resources().materials()[0]),
        )
        .unwrap(),
        geometry: SceneGeometry::new(mesh_table, objects).unwrap(),
        terrain: None,
        effects: None,
    })
    .unwrap()
}

/// The camera and cascade inputs of one frame.
pub(super) struct FrameViews {
    pub eye_millimeters: [i64; 3],
    pub fade_eye: Vec3,
    pub camera: Mat4,
    /// Receiver matrices, before the caster depth fit.
    pub receivers: [Mat4; 4],
    /// World corners of each cascade's camera slice.
    pub slices: [[Vec3; 8]; 4],
}

const SPLITS: [f32; 5] = [0.1, 15.0, 60.0, 220.0, 700.0];
const ASPECT: f32 = 16.0 / 9.0;
const FOV_Y: f32 = 1.05;

/// A camera at `eye` looking along `yaw` (radians from east) and `pitch`.
pub(super) fn views(eye: Vec3, yaw: f32, pitch: f32) -> FrameViews {
    let direction = Vec3::new(
        yaw.cos() * pitch.cos(),
        yaw.sin() * pitch.cos(),
        pitch.sin(),
    );
    let view = glam::camera::rh::view::look_to_mat4(eye, direction, Vec3::Z);
    let camera =
        glam::camera::rh::proj::directx::perspective(FOV_Y, ASPECT, SPLITS[0], 3000.0) * view;
    let world = view.inverse();
    let light = Vec3::new(0.35, 0.25, -0.9).normalize();
    let mut receivers = [Mat4::IDENTITY; 4];
    let mut slices = [[Vec3::ZERO; 8]; 4];
    for cascade in 0..4 {
        let corners: [Vec3; 8] = std::array::from_fn(|corner| {
            let depth = SPLITS[cascade + usize::from(corner >= 4)];
            let half_y = depth * (FOV_Y * 0.5).tan();
            let x = if corner & 1 == 0 { -1.0 } else { 1.0 } * half_y * ASPECT;
            let y = if corner & 2 == 0 { -1.0 } else { 1.0 } * half_y;
            world.transform_point3(Vec3::new(x, y, -depth))
        });
        let center = corners.iter().copied().sum::<Vec3>() / 8.0;
        let radius = corners
            .iter()
            .map(|corner| corner.distance(center))
            .fold(0.0, f32::max);
        let distance = radius + 600.0;
        receivers[cascade] =
            glam::camera::rh::proj::directx::orthographic(
                -radius,
                radius,
                -radius,
                radius,
                0.0,
                distance + radius,
            ) * glam::camera::rh::view::look_to_mat4(center - light * distance, light, Vec3::Z);
        slices[cascade] = corners;
    }
    FrameViews {
        eye_millimeters: (eye * 1000.0).round().as_i64vec3().to_array(),
        fade_eye: eye,
        camera,
        receivers,
        slices,
    }
}

/// Frame `index` of a walk across the woods that turns as it goes.
pub(super) fn walk(index: u16) -> FrameViews {
    let step = f32::from(index);
    views(
        Vec3::new(-300.0 + step * 1.5, -200.0 + step * 0.8, 2.0),
        0.4 + step * 0.03,
        -0.05,
    )
}

/// The slice hull cascade `index` culls with when drawn with `matrix`.
pub(super) fn slice_hull(views: &FrameViews, index: usize, matrix: Mat4) -> SliceHull {
    SliceHull::new(&views.slices[index], matrix, 0.01)
}
