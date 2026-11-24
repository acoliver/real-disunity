//! Moving instances write the residual of their own transform change, and the TAA resolve uses it.
//!
//! The scene is the 2 m cutout quad on the ground plane, translated along x by editing its
//! instance transform between frames. The camera is fixed, so the previous unjittered
//! view-projection is the current one and only the instance motion contributes.
use super::*;
use crate::test_support::upload_scene;
use worldrunner_render_plan::SceneTransform;

fn at(millimetres: i16) -> SceneTransform {
    SceneTransform {
        position_millimeters: [i64::from(millimetres), 0, 0],
        ..identity()
    }
}

fn metres(millimetres: i16) -> f32 {
    f32::from(millimetres) / 1000.0
}

/// How to treat the previous transform of the instance.
#[derive(Clone, Copy, PartialEq)]
enum Previous {
    /// Velocity pipelines with the tracked previous transform.
    Tracked,
    /// Velocity pipelines whose previous transform is forced equal to the current one.
    Forced,
    /// SSAO pipelines with velocity and the tracked previous transform.
    Occluded,
    /// No TAA: the unjittered reference.
    Reference,
}

struct Moving {
    context: GpuContext,
    gpu: SceneGpu,
    pipelines: ScenePipelines,
    renderer: FrameRenderer,
    color: AttachmentView,
    depth: AttachmentView,
    handle: crate::draw::InstanceHandle,
    frame: u8,
}

impl Moving {
    fn new(previous: Previous, samples: MsaaSamples) -> Self {
        let context = context();
        let device = context.device();
        let (scene, effects, pipelines) = if previous == Previous::Reference {
            (
                still(),
                effects(),
                ScenePipelines::new(device, HDR, samples.count()).unwrap(),
            )
        } else if previous == Previous::Occluded {
            let (scene, effects) = super::screen_space::with_occlusion(&still());
            let pipelines =
                ScenePipelines::with_ssao_taa_velocity(device, HDR, samples.count()).unwrap();
            (scene, effects, pipelines)
        } else {
            let (scene, effects) = with_taa(&still());
            let pipelines =
                ScenePipelines::with_taa_velocity(device, HDR, samples.count()).unwrap();
            (scene, effects, pipelines)
        };
        let gpu = upload_scene(device, context.submission(), &scene, &pipelines).unwrap();
        let renderer = FrameRenderer::hdr_antialiased(
            &context,
            [TARGET, TARGET],
            COLOR_FORMAT,
            AntiAliasing::new(samples, PostResolve::None),
            ToneMap::Reinhard,
            Some(&effects),
        )
        .unwrap();
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

    /// Moves the instance to `metres` and draws one frame, to `view` when given.
    fn draw(&mut self, millimetres: i16, forced: bool, view: Option<&wgpu::TextureView>) {
        let device = self.context.device();
        self.gpu
            .set_instance_transform(self.handle, at(millimetres))
            .unwrap();
        if forced {
            self.gpu.reset_velocity_history();
        }
        self.renderer.select_frame_slot((self.frame % 3).into());
        self.renderer
            .set_frame_time(std::time::Duration::from_secs_f32(
                f32::from(self.frame) * 0.03125,
            ));
        let mut encoder = self.context.submission().begin(device).unwrap();
        self.renderer
            .encoding()
            .encode_scene(
                device,
                &mut encoder,
                (&mut self.gpu, &self.pipelines),
                crate::compositor::ColorDepth {
                    color: &self.color,
                    depth: &self.depth,
                },
                frame_from(pose()),
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

/// The residual of the ground point seen through pixel `(x, y)` for a quad that moved by
/// `shift` metres along x since the previous frame, and that point's offset from the quad centre
/// at `centre` metres.
fn expected_shift(x: u32, y: u32, (centre, shift): (f32, f32), vp: Mat4) -> ([f32; 2], Vec3) {
    let [size, column, row] = [TARGET, x, y].map(|value| f32::from(u16::try_from(value).unwrap()));
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
    let ground = near + (far - near) * (near.z / (near.z - far.z));
    let previous = ground - Vec3::new(shift, 0.0, 0.0);
    let (a, b) = (uv(vp, previous), uv(vp, ground));
    (
        [a[0] - b[0], a[1] - b[1]],
        ground - Vec3::new(centre, 0.0, 0.0),
    )
}

fn residuals_after(previous: Previous, offsets: &[i16]) -> Vec<[f32; 2]> {
    let mut moving = Moving::new(previous, MsaaSamples::X1);
    for &offset in offsets {
        moving.draw(offset, false, None);
    }
    moving.velocity()
}

/// Checks the residual of a quad that moved by `shift_mm` to `centre_mm` along x.
fn assert_translation_residual(velocity: &[[f32; 2]], centre_mm: i16, shift_mm: i16) {
    let (centre, shift) = (metres(centre_mm), metres(shift_mm));
    let vp = frame_from(pose()).view_projection;
    let (mut compared, mut largest) = (0, 0.0_f32);
    for y in 0..TARGET {
        for x in 0..TARGET {
            let (residual, local) = expected_shift(x, y, (centre, shift), vp);
            let got = velocity[(y * TARGET + x) as usize];
            // The cutout leaves holes that keep the cleared zero, and edges are dilated.
            if local.x.abs() > 0.9 || local.y.abs() > 0.9 || got == [0.0, 0.0] {
                continue;
            }
            for lane in 0..2 {
                assert!(
                    (got[lane] - residual[lane]).abs() <= TOLERANCE_UV,
                    "pixel {x},{y}: got {got:?}, expected {residual:?}"
                );
            }
            largest = largest.max(residual[0].abs());
            compared += 1;
        }
    }
    assert!(compared > 200, "only {compared} pixels compared");
    assert!(largest > 0.01, "the residual was never large: {largest}");
}

/// Whether any pixel outside the quad centred at `centre_mm`, dilation included, has a residual.
fn residual_outside_quad(velocity: &[[f32; 2]], centre_mm: i16) -> bool {
    let vp = frame_from(pose()).view_projection;
    (0..TARGET).any(|y| {
        (0..TARGET).any(|x| {
            let (_, local) = expected_shift(x, y, (metres(centre_mm), 0.0), vp);
            let outside = local.x.abs() > 1.25 || local.y.abs() > 1.25;
            outside && velocity[(y * TARGET + x) as usize] != [0.0, 0.0]
        })
    })
}

fn all_zero(velocity: &[[f32; 2]]) -> bool {
    velocity.iter().all(|texel| *texel == [0.0, 0.0])
}

#[test]
fn a_translated_instance_writes_the_analytic_residual() {
    let _session = crate::test_support::SceneTestSession::acquire();
    assert_translation_residual(&residuals_after(Previous::Tracked, &[0, 0, 250]), 250, 250);
}

#[test]
fn a_translated_instance_in_an_ssao_scene_writes_the_analytic_residual() {
    let _session = crate::test_support::SceneTestSession::acquire();
    assert_translation_residual(&residuals_after(Previous::Occluded, &[0, 0, 250]), 250, 250);
}

#[test]
fn a_static_instance_and_a_settled_instance_write_exactly_zero() {
    let _session = crate::test_support::SceneTestSession::acquire();
    assert!(
        residuals_after(Previous::Tracked, &[0, 0, 0])
            .iter()
            .all(|texel| *texel == [0.0, 0.0])
    );
    // Moved once, then untouched: previous converges to current and the residual vanishes.
    assert!(
        residuals_after(Previous::Tracked, &[0, 250, 250, 250])
            .iter()
            .all(|texel| *texel == [0.0, 0.0])
    );
}

#[test]
fn a_velocity_reset_writes_zero_residual_until_the_instance_moves_again() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let mut moving = Moving::new(Previous::Tracked, MsaaSamples::X1);
    for offset in [0, 0, 250] {
        moving.draw(offset, false, None);
    }
    assert!(
        !all_zero(&moving.velocity()),
        "the move before the reset left no residual"
    );
    // The quad keeps moving through the reset frames, so only the reset can zero them.
    for (frame, offset) in [375, 500].into_iter().enumerate() {
        moving.draw(offset, true, None);
        assert!(
            all_zero(&moving.velocity()),
            "reset frame {frame} wrote a residual"
        );
    }
    moving.draw(750, false, None);
    assert_translation_residual(&moving.velocity(), 750, 250);
}

/// A second quad, 3 m north of the first and clear of it in the view.
fn north(millimetres: i16) -> SceneTransform {
    SceneTransform {
        position_millimeters: [0, 3_000 + i64::from(millimetres), 0],
        ..identity()
    }
}

#[test]
fn a_velocity_reset_after_a_removal_leaves_residual_only_on_the_surviving_instance() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let mut moving = Moving::new(Previous::Tracked, MsaaSamples::X1);
    let object = SceneObject::new(
        north(0),
        MeshHandle::from_raw(0),
        MaterialHandle::from_raw(0),
    );
    let second = moving.gpu.add_instance(object, true).unwrap();
    for (step, offset) in [0, 0, 250].into_iter().enumerate() {
        let step = i16::try_from(step).unwrap();
        moving
            .gpu
            .set_instance_transform(second, north(150 * step))
            .unwrap();
        moving.draw(offset, false, None);
    }
    assert!(
        residual_outside_quad(&moving.velocity(), 250),
        "the second instance wrote no residual before its removal"
    );
    moving.gpu.remove_instance(second).unwrap();
    // The surviving quad moves in the reset frame, so only the reset can zero it.
    moving.draw(375, true, None);
    assert!(
        all_zero(&moving.velocity()),
        "the reset frame wrote a residual"
    );
    moving.draw(625, false, None);
    let velocity = moving.velocity();
    assert_translation_residual(&velocity, 625, 250);
    assert!(
        !residual_outside_quad(&velocity, 625),
        "the removed instance left a residual"
    );
}

const FRAMES: u8 = 12;
/// About 1.8 px a frame at this camera (the sway tests move about 15 px per metre).
const STEP_MM: i16 = 120;
const START_MM: i16 = -700;

fn position(frame: u8) -> i16 {
    START_MM + STEP_MM * i16::from(frame)
}

fn output(previous: Previous, samples: MsaaSamples) -> Vec<u8> {
    let mut moving = Moving::new(previous, samples);
    let output = output_texture(moving.context.device());
    let view = output.create_view(&TextureViewDescriptor::default());
    let frames = if previous == Previous::Reference {
        FRAMES - 1..FRAMES
    } else {
        0..FRAMES
    };
    for frame in frames {
        moving.draw(position(frame), previous == Previous::Forced, Some(&view));
    }
    let device = moving.context.device();
    Readback::new(
        device,
        FrameConfig::new([TARGET, TARGET], COLOR_FORMAT, 1),
        1 << 20,
    )
    .unwrap()
    .read(device, moving.context.submission(), &output)
    .unwrap()
}

/// Mean error over the columns from four frames of travel behind the quad's trailing edge to
/// its leading edge at the last frame: the band where a resolve that ignores the motion leaves
/// its ghost.
fn band_error(image: &[u8], reference: &[u8]) -> f64 {
    let vp = frame_from(pose()).view_projection;
    let size = f32::from(u16::try_from(TARGET).unwrap());
    let column = |millimetres: i16| uv(vp, Vec3::new(metres(millimetres), 0.0, 0.0))[0] * size;
    let last = position(FRAMES - 1);
    let (first, end) = (column(last - 1000 - 4 * STEP_MM), column(last + 1000));
    let (mut sum, mut count) = (0_u32, 0_u32);
    for row in 0..TARGET {
        for pixel in 0..TARGET {
            let centre = f32::from(u16::try_from(pixel).unwrap());
            if centre < first || centre >= end {
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
    let reference = output(Previous::Reference, samples);
    let error = |previous| band_error(&output(previous, samples), &reference);
    (error(Previous::Forced), error(Previous::Tracked))
}

#[test]
fn a_moving_instance_under_a_fixed_camera_resolves_closer_to_the_unjittered_frame() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let (without, with) = errors(MsaaSamples::X1);
    println!("moving instance band error: without {without}, with {with}");
    assert!(
        with < 0.75 * without,
        "with the instance previous transform {with}, without {without}"
    );
}

#[test]
fn four_sample_msaa_with_a_moving_instance_does_not_resolve_worse() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    if !crate::msaa_tests::supported_samples(&context).contains(&4) {
        return;
    }
    drop(context);
    let (without, with) = errors(MsaaSamples::X4);
    println!("moving instance band error at 4x: without {without}, with {with}");
    assert!(with <= without, "with {with}, without {without} at 4x");
}
