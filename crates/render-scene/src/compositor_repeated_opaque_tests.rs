//! A role may appear more than once in the frame's pass list, and a borrowed feature draws in
//! the entry it names. The extra colour formats the compositor gives a feature's Opaque pass
//! are those of that pass's own entry, so two Opaque entries with different layouts both
//! encode.
use super::{FrameRenderer, attachment};
use crate::{CameraPose, SceneView, Viewport, device::GpuContext, frame::FrameUniform};
use std::{cell::RefCell, rc::Rc};
use worldrunner_render_gpu::submission::PrepareError;
use worldrunner_render_plan::SceneCamera;
use worldrunner_render_post::{
    AttachmentView, BorrowedFeature, DrawContext, FeatureDeclaration, FeaturePass, FrameError,
    PassDeclaration, PassEntry, PassSlot, PrepareContext, PreparedFeature, ResourceRef, ToneMap,
    color_texture,
};

const SIZE: u32 = 32;
const OUTPUT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;

/// Records the entry of every pass it is asked to draw in, and draws nothing.
struct Recorder(Rc<RefCell<Vec<PassEntry>>>);

impl PreparedFeature for Recorder {
    fn prepare(&mut self, _: &mut PrepareContext<'_>) -> Result<(), FrameError> {
        Ok(())
    }
    fn draw(&self, context: &DrawContext, _: &mut wgpu::RenderPass<'_>) {
        self.0.borrow_mut().push(context.entry);
    }
}

fn frame() -> FrameUniform {
    let view = SceneView::new(
        CameraPose {
            eye_millimeters: [0, -10_000, 1_700],
            target_millimeters: [0, 0, 1_700],
        },
        SceneCamera {
            vertical_fov_millidegrees: 60_000,
            near_plane_millimeters: 100,
            far_plane_millimeters: 100_000,
        },
        Viewport {
            width: SIZE,
            height: SIZE,
        },
    )
    .unwrap();
    FrameUniform::new(&view, None).unwrap()
}

/// Encodes a frame of two Opaque entries, whose extra colour format after location zero is
/// `layouts[0]` and `layouts[1]` (none for no extra), and one borrowed
/// feature that declares a pass in each, naming no formats of its own. Returns the entries
/// the feature drew in.
fn encode_two_opaque_entries(
    context: &GpuContext,
    layouts: [Option<wgpu::TextureFormat>; 2],
) -> Result<Vec<PassEntry>, FrameError> {
    let device = context.device();
    let mut renderer =
        FrameRenderer::hdr(context, [SIZE, SIZE], OUTPUT, 1, ToneMap::Reinhard, None).unwrap();
    let prepared = renderer.prepare_uniform(frame()).unwrap();
    let scene_format = renderer.targets().scene().texture().format();
    let scene = AttachmentView::new(
        ResourceRef::new("scene color"),
        renderer.targets().scene().texture(),
    )?;
    let depth = AttachmentView::new(
        ResourceRef::new("scene depth"),
        renderer.targets().depth().texture(),
    )?;
    let second_texture = color_texture(device, "second color", [SIZE, SIZE], scene_format);
    let second = AttachmentView::new(ResourceRef::new("second color"), &second_texture)?;
    let names = ["first extra", "second extra"];
    let extra_textures: Vec<_> = layouts
        .iter()
        .zip(names)
        .map(|(format, name)| {
            format.map(|format| color_texture(device, name, [SIZE, SIZE], format))
        })
        .collect();
    let extra_views: Vec<_> = extra_textures
        .iter()
        .zip(names)
        .map(|(texture, name)| {
            texture
                .as_ref()
                .map(|texture| AttachmentView::new(ResourceRef::new(name), texture))
                .transpose()
        })
        .collect::<Result<_, _>>()?;

    let mut first = PassDeclaration::nth(PassSlot::Opaque, 0);
    first.color = Some(attachment(scene.resource(), &scene, false));
    first.depth = Some(attachment(depth.resource(), &depth, true));
    let mut next = PassDeclaration::nth(PassSlot::Opaque, 1);
    next.color = Some(attachment(second.resource(), &second, false));
    for (pass, view) in [&mut first, &mut next].into_iter().zip(&extra_views) {
        pass.additional_colors = view
            .iter()
            .map(|view| attachment(view.resource(), view, false))
            .collect();
    }
    let mut views = vec![scene, depth, second];
    views.extend(extra_views.into_iter().flatten());

    let draws = Rc::new(RefCell::new(Vec::new()));
    let mut recorder = Recorder(draws.clone());
    let mut features = [BorrowedFeature {
        declaration: FeatureDeclaration {
            passes: vec![
                FeaturePass::nth(PassSlot::Opaque, 0),
                FeaturePass::nth(PassSlot::Opaque, 1),
            ],
            effects: Vec::new(),
        },
        prepared: &mut recorder,
    }];
    let transaction = context
        .submission()
        .prepare(device, |encoder, _| {
            renderer.encode_frame(
                device,
                encoder,
                (&[first, next], &views),
                None,
                &mut features,
                &(prepared, None),
            )
        })
        .map_err(|error| match error {
            PrepareError::Preparation(error) => error,
            other => panic!("the transaction did not start: {other:?}"),
        })?;
    transaction.submit().unwrap();
    Ok(draws.take())
}

#[test]
fn each_repeated_opaque_keeps_its_own_extra_colour_formats() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = GpuContext::shared_for_test().unwrap();
    let layouts: [[Option<wgpu::TextureFormat>; 2]; 3] = [
        [None, Some(wgpu::TextureFormat::Rg16Float)],
        [
            Some(wgpu::TextureFormat::Rgba8Unorm),
            Some(wgpu::TextureFormat::Rg16Float),
        ],
        [Some(wgpu::TextureFormat::Rg16Float), None],
    ];
    for layout in layouts {
        let drawn = encode_two_opaque_entries(&context, layout)
            .unwrap_or_else(|error| panic!("{layout:?}: the frame was refused: {error:?}"));
        assert_eq!(
            drawn,
            [
                PassEntry::nth(PassSlot::Opaque, 0),
                PassEntry::nth(PassSlot::Opaque, 1)
            ],
            "{layout:?}: the feature draws once in each entry it names"
        );
    }
}
