//! Camera-facing cards write the residual of their instance motion and of the turn of their
//! basis toward the moving camera.
//!
//! The card is a 2 m yaw-locked billboard on the ground plane. Its world position is
//! `pivot + basis(camera, pivot) * local`, so a point of the card seen through a pixel has the
//! previous position `pivot_prev + basis(camera_prev, pivot_prev) * local` with the same local
//! offset, which the tests compute on the CPU.
use super::super::foliage::cutout_texture;
use super::*;
use glam::Mat3;
use worldrunner_render_plan::SceneTransform;

const FRAMES: u8 = 12;
/// Camera orbit step in radians: the card edge moves about 2 px per frame in a 64 px target.
const ORBIT_STEP: f32 = 0.12;
const RADIUS_MM: f32 = 2_800.0;

fn at(millimetres: i16) -> SceneTransform {
    SceneTransform {
        position_millimeters: [i64::from(millimetres), 0, 0],
        ..identity()
    }
}

/// The camera `step` orbit steps around the vertical axis through the card, looking at its middle.
fn orbit(step: u8) -> CameraPose {
    let angle = ORBIT_STEP * f32::from(step);
    CameraPose {
        eye_millimeters: [
            (RADIUS_MM * angle.sin()).round() as i64,
            (-RADIUS_MM * angle.cos()).round() as i64,
            2_000,
        ],
        target_millimeters: [0, 0, 1_000],
    }
}

fn scene(mask: bool) -> RenderScene {
    let (texture, alpha) = if mask {
        (
            cutout_texture(),
            SceneAlphaMode::Mask {
                cutoff_micro: 500_000,
            },
        )
    } else {
        (white_texture(), SceneAlphaMode::Opaque)
    };
    let object = SceneObject::new(
        identity(),
        MeshHandle::from_raw(0),
        MaterialHandle::from_raw(0),
    )
    .with_tint(worldrunner_render_plan::SceneTint::new([0; 3]).unwrap());
    RenderScene::new(SceneParts {
        resources: SceneResources::new(vec![texture], vec![material(alpha, true)]).unwrap(),
        geometry: SceneGeometry::new(vec![card(Some(BillboardPolicy::YawLocked))], vec![object])
            .unwrap(),
        terrain: None,
        effects: Some(effects()),
    })
    .unwrap()
}

/// Which pipelines draw the card.
#[derive(Clone, Copy, PartialEq)]
enum Basis {
    /// Velocity pipelines building the previous basis from the previous camera.
    Previous,
    /// Velocity pipelines whose previous basis uses the current camera.
    Current,
    /// No TAA: the unjittered reference.
    Reference,
}

struct Card {
    context: GpuContext,
    gpu: SceneGpu,
    pipelines: ScenePipelines,
    renderer: FrameRenderer,
    color: AttachmentView,
    depth: AttachmentView,
    handle: crate::draw::InstanceHandle,
    frame: u8,
}

impl Card {
    fn new(basis: Basis, samples: MsaaSamples, mask: bool) -> Self {
        let context = context();
        let device = context.device();
        let (scene, effects, pipelines) = if basis == Basis::Reference {
            (
                self::scene(mask),
                effects(),
                ScenePipelines::with_samples(device, HDR, samples.count()).unwrap(),
            )
        } else {
            let (scene, effects) = with_taa(&self::scene(mask));
            let pipelines = if basis == Basis::Previous {
                ScenePipelines::with_taa_velocity(device, HDR, samples.count()).unwrap()
            } else {
                ScenePipelines::with_environment(
                    device,
                    (HDR, samples.count()),
                    &crate::velocity::current_camera_object_source(crate::shader::ENVIRONMENT_WGSL),
                    &crate::velocity::terrain_source(crate::shader::TERRAIN_WGSL),
                    None,
                )
            };
            (scene, effects, pipelines)
        };
        let gpu = SceneGpu::upload(device, context.submission(), &scene, &pipelines).unwrap();
        let (renderer, color, depth) = taa_target(&context, &effects, samples);
        let handle = gpu.instance_handles().next().unwrap();
        Self {
            context,
            gpu,
            pipelines,
            renderer,
            color,
            depth,
            handle,
            frame: 0,
        }
    }

    /// Draws one frame with the card at `millimetres` along x, to `view` when given.
    fn draw(&mut self, (pose, millimetres): (CameraPose, i16), view: Option<&wgpu::TextureView>) {
        let device = self.context.device();
        self.gpu
            .set_instance_transform(self.handle, at(millimetres))
            .unwrap();
        self.renderer.select_frame_slot((self.frame % 3).into());
        self.renderer
            .set_frame_time(f32::from(self.frame) * 0.03125)
            .unwrap();
        let mut encoder = self.context.submission().begin(device).unwrap();
        crate::compositor::encode_scene(
            &mut self.renderer,
            device,
            &mut encoder,
            (&mut self.gpu, &self.pipelines),
            (&self.color, &self.depth),
            frame_from(pose),
        )
        .unwrap();
        if let Some(view) = view {
            self.renderer.encode_output(&mut encoder, view).unwrap();
        }
        encoder.finish().submit().unwrap();
        self.frame += 1;
    }

    fn velocity(&self) -> Vec<[f32; 2]> {
        let texture = self.renderer.targets().velocity_texture().unwrap().clone();
        read_velocity(&self.context, &texture)
    }
}

fn residuals_after(frames: &[(CameraPose, i16)]) -> Vec<[f32; 2]> {
    let mut card = Card::new(Basis::Previous, MsaaSamples::X1, false);
    for &frame in frames {
        card.draw(frame, None);
    }
    card.velocity()
}

fn position(pose: CameraPose, millimetres: i16) -> (Vec3, Vec3) {
    let millis = |values: [i64; 3]| Vec3::from_array(values.map(|value| value as f32 / 1000.0));
    (
        millis(pose.eye_millimeters),
        Vec3::new(f32::from(millimetres) / 1000.0, 0.0, 0.0),
    )
}

/// The yaw-locked basis of `billboard_basis` in the shader: (right, away, up).
fn yaw_basis((camera, pivot): (Vec3, Vec3)) -> Mat3 {
    let horizontal = (camera - pivot).truncate().normalize();
    let away = Vec3::new(-horizontal.x, -horizontal.y, 0.0);
    Mat3::from_cols(away.cross(Vec3::Z), away, Vec3::Z)
}

/// The analytic residual of the card point seen through pixel `(x, y)` and that point's local
/// offset on the card, for `now` and `before` given as `(camera pose, x in millimetres)`.
fn expected_card(
    (x, y): (u32, u32),
    now: (CameraPose, i16),
    before: (CameraPose, i16),
) -> ([f32; 2], Vec3) {
    let [size, column, row] = [TARGET, x, y].map(|value| f32::from(u16::try_from(value).unwrap()));
    let vp = frame_from(now.0).view_projection;
    let ndc = |depth| {
        Vec4::new(
            (column + 0.5) / size * 2.0 - 1.0,
            1.0 - (row + 0.5) / size * 2.0,
            depth,
            1.0,
        )
    };
    let inverse = vp.inverse();
    let unproject = |depth| {
        let point = inverse * ndc(depth);
        point.truncate() / point.w
    };
    let (near, far) = (unproject(0.0), unproject(1.0));
    let (camera, pivot) = position(now.0, now.1);
    let basis = yaw_basis((camera, pivot));
    let away = basis.y_axis;
    let point = near + (far - near) * ((pivot - near).dot(away) / (far - near).dot(away));
    let local = basis.transpose() * (point - pivot);
    let (previous_camera, previous_pivot) = position(before.0, before.1);
    let previous = previous_pivot + yaw_basis((previous_camera, previous_pivot)) * local;
    let previous_vp = frame_from(before.0).view_projection;
    let (a, b) = (uv(previous_vp, previous), uv(previous_vp, point));
    ([a[0] - b[0], a[1] - b[1]], local)
}

/// Asserts every interior texel of the card matches the analytic residual and returns the
/// largest component compared.
fn assert_matches_analytic(
    velocity: &[[f32; 2]],
    now: (CameraPose, i16),
    before: (CameraPose, i16),
) -> f32 {
    let (mut compared, mut largest) = (0, 0.0_f32);
    for y in 0..TARGET {
        for x in 0..TARGET {
            let (residual, local) = expected_card((x, y), now, before);
            if local.x.abs() > 0.85 || !(0.15..1.85).contains(&local.z) {
                continue;
            }
            let got = velocity[(y * TARGET + x) as usize];
            for lane in 0..2 {
                assert!(
                    (got[lane] - residual[lane]).abs() <= TOLERANCE_UV,
                    "pixel {x},{y}: got {got:?}, expected {residual:?}"
                );
            }
            largest = largest.max(residual[0].abs()).max(residual[1].abs());
            compared += 1;
        }
    }
    assert!(compared > 200, "only {compared} pixels compared");
    largest
}

#[test]
fn a_translated_billboard_writes_the_analytic_residual() {
    let still = orbit(0);
    let velocity = residuals_after(&[(still, 0), (still, 0), (still, 250)]);
    let largest = assert_matches_analytic(&velocity, (still, 250), (still, 0));
    assert!(largest > 0.01, "the residual was never large: {largest}");
}

#[test]
fn a_billboard_under_an_orbiting_camera_writes_the_residual_of_its_turning_basis() {
    let velocity = residuals_after(&[(orbit(0), 0), (orbit(0), 0), (orbit(1), 0)]);
    let largest = assert_matches_analytic(&velocity, (orbit(1), 0), (orbit(0), 0));
    assert!(largest > 0.01, "the residual was never large: {largest}");
}

#[test]
fn a_billboard_with_static_camera_and_instance_writes_exactly_zero() {
    let still = orbit(3);
    assert!(
        residuals_after(&[(still, 0), (still, 0), (still, 0)])
            .iter()
            .all(|texel| *texel == [0.0, 0.0])
    );
    // The camera moved once and then stopped: previous converges to current.
    assert!(
        residuals_after(&[(orbit(0), 0), (still, 0), (still, 0), (still, 0)])
            .iter()
            .all(|texel| *texel == [0.0, 0.0])
    );
}

fn output(basis: Basis, samples: MsaaSamples) -> Vec<u8> {
    let mut card = Card::new(basis, samples, true);
    let output = output_texture(card.context.device());
    let view = output.create_view(&TextureViewDescriptor::default());
    let frames = if basis == Basis::Reference {
        FRAMES - 1..FRAMES
    } else {
        0..FRAMES
    };
    for frame in frames {
        card.draw((orbit(frame), 0), Some(&view));
    }
    let device = card.context.device();
    Readback::new(
        device,
        FrameConfig::new([TARGET, TARGET], COLOR_FORMAT, 1),
        1 << 20,
    )
    .unwrap()
    .read(device, card.context.submission(), &output)
    .unwrap()
}

/// Mean error over the screen box of the card at the last frame, plus a margin of 4 px.
fn band_error(image: &[u8], reference: &[u8]) -> f64 {
    let last = orbit(FRAMES - 1);
    let (camera, pivot) = position(last, 0);
    let basis = yaw_basis((camera, pivot));
    let vp = frame_from(last).view_projection;
    let size = f32::from(u16::try_from(TARGET).unwrap());
    let corners = [-1.0, 1.0]
        .into_iter()
        .flat_map(|x| [0.0, 2.0].into_iter().map(move |z| Vec3::new(x, 0.0, z)));
    let (mut low, mut high) = ([size; 2], [0.0_f32; 2]);
    for corner in corners {
        let screen = uv(vp, pivot + basis * corner);
        for lane in 0..2 {
            low[lane] = low[lane].min(screen[lane] * size - 4.0);
            high[lane] = high[lane].max(screen[lane] * size + 4.0);
        }
    }
    let (mut sum, mut count) = (0_u32, 0_u32);
    for row in 0..TARGET {
        for pixel in 0..TARGET {
            let centre = [pixel, row].map(|value| f32::from(u16::try_from(value).unwrap()));
            if (0..2).any(|lane| centre[lane] < low[lane] || centre[lane] >= high[lane]) {
                continue;
            }
            for lane in 0..4 {
                let index = usize::try_from((row * TARGET + pixel) * 4 + lane).unwrap();
                sum += u32::from(image[index].abs_diff(reference[index]));
                count += 1;
            }
        }
    }
    f64::from(sum) / f64::from(count) / 255.0
}

fn errors(samples: MsaaSamples) -> (f64, f64) {
    let reference = output(Basis::Reference, samples);
    let error = |basis| band_error(&output(basis, samples), &reference);
    (error(Basis::Current), error(Basis::Previous))
}

#[test]
fn a_billboard_under_an_orbiting_camera_resolves_closer_to_the_unjittered_frame() {
    let (without, with) = errors(MsaaSamples::X1);
    println!("orbiting billboard band error: without {without}, with {with}");
    assert!(
        with < 0.75 * without,
        "with the previous camera basis {with}, without {without}"
    );
}

#[test]
fn four_sample_msaa_with_an_orbiting_billboard_does_not_resolve_worse() {
    let context = context();
    if !crate::msaa_tests::supported_samples(&context).contains(&4) {
        return;
    }
    drop(context);
    let (without, with) = errors(MsaaSamples::X4);
    println!("orbiting billboard band error at 4x: without {without}, with {with}");
    assert!(with <= without, "with {with}, without {without} at 4x");
}
