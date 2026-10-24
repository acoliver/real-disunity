use super::*;
use worldrunner_render_post::{BorrowedFeature, DrawContext, PrepareContext};

pub(super) struct Probe(pub(super) usize);
impl PreparedFeature for Probe {
    fn prepare(&mut self, _: &mut PrepareContext<'_>) -> Result<(), FrameError> {
        self.0 += 1;
        Ok(())
    }
    fn draw(&self, _: &DrawContext, _: &mut wgpu::RenderPass<'_>) {}
}
pub(super) fn renderer(context: &GpuContext, samples: u32) -> crate::compositor::FrameRenderer {
    crate::compositor::FrameRenderer::hdr(
        context,
        [32, 32],
        wgpu::TextureFormat::Rgba8UnormSrgb,
        samples,
        ToneMap::Reinhard,
        None,
    )
    .unwrap()
}
fn source_texture(context: &GpuContext, depth: bool, mip: u32, layer: u32) -> wgpu::Texture {
    context.device().create_texture(&wgpu::TextureDescriptor {
        label: Some("checked snapshot source"),
        size: wgpu::Extent3d {
            width: 32 << mip,
            height: 32 << mip,
            depth_or_array_layers: layer + 1,
        },
        mip_level_count: mip + 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: if depth {
            wgpu::TextureFormat::Depth32Float
        } else {
            wgpu::TextureFormat::Rgba16Float
        },
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    })
}
pub(super) fn opaque(
    color: Option<&AttachmentView>,
    depth: Option<&AttachmentView>,
    red: f64,
    z: f32,
) -> PassDeclaration {
    let mut pass = PassDeclaration::new(PassSlot::Opaque);
    pass.color = color.map(|view| {
        let mut attachment = crate::compositor::attachment(view.resource(), view, false);
        attachment.load = AttachmentLoad::ClearColor(wgpu::Color {
            r: red,
            g: 0.125,
            b: 0.0625,
            a: 1.0,
        });
        attachment
    });
    pass.depth = depth.map(|view| {
        let mut attachment = crate::compositor::attachment(view.resource(), view, true);
        attachment.load = AttachmentLoad::ClearDepth(z);
        attachment
    });
    pass
}
pub(super) fn consumer(
    pass: &PassDeclaration,
    reads: &[&'static str],
    samples: u32,
) -> (PassDeclaration, FeatureDeclaration) {
    let mut output = pass.clone();
    output.slot = PassSlot::ScreenSpaceAfterTransparent;
    for attachment in output.color.iter_mut().chain(&mut output.depth) {
        attachment.load = AttachmentLoad::Load;
    }
    let mut feature = FeatureDeclaration::new(output.slot);
    feature.passes[0].samples = samples;
    feature.passes[0].reads = reads.iter().map(|name| ResourceRef::new(name)).collect();
    (output, feature)
}

#[test]
fn checked_mips_layers_and_versions_reach_all_snapshot_producers() {
    let context = pollster::block_on(GpuContext::request()).unwrap();
    for (mip, layer) in [(1, 0), (0, 1), (0, 0), (1, 1)] {
        let mut renderer = renderer(&context, 1);
        let mut views = Vec::new();
        for version in [0, 1] {
            for (depth, name) in [(false, "scene color"), (true, "scene depth")] {
                let texture = source_texture(&context, depth, mip, layer);
                views.push(
                    AttachmentView::new(
                        ResourceRef {
                            name,
                            version,
                            mip,
                            layer,
                        },
                        &texture,
                    )
                    .unwrap(),
                );
            }
        }
        let mut earlier = opaque(Some(&views[0]), Some(&views[1]), 0.125, 0.25);
        earlier.slot = PassSlot::DepthNormalPrepass;
        let source = opaque(Some(&views[2]), Some(&views[3]), 0.5, 0.75);
        let (output, declaration) = consumer(
            &source,
            &[
                "opaque snapshot color",
                "opaque snapshot depth",
                "post-water depth",
            ],
            1,
        );
        let mut probe = Probe(0);
        let frame = FrameUniform::new(&fixture_view(), None).unwrap();
        let mut encoder = context.submission().begin(context.device()).unwrap();
        renderer
            .encode_frame(
                context.device(),
                &mut encoder,
                (&[earlier, source, output], &views),
                &mut [BorrowedFeature {
                    declaration,
                    prepared: &mut probe,
                }],
                (frame, None),
            )
            .unwrap();
        encoder.finish().submit().unwrap();
        assert_eq!(probe.0, 1);
        assert_checked_snapshot(&context, &mut renderer, frame);
    }
}

fn assert_checked_snapshot(
    context: &GpuContext,
    renderer: &mut crate::compositor::FrameRenderer,
    frame: FrameUniform,
) {
    let (color, depth) = renderer.opaque_snapshot().unwrap();
    assert_eq!(
        read(context, color.texture()),
        contracts::bytes_for_color(0x3800)
    );
    let actual = read(context, depth.texture());
    let z = f32::from_le_bytes(actual[0..4].try_into().unwrap());
    let world = frame.view_projection.inverse() * glam::Vec4::new(-0.96875, 0.96875, 0.75, 1.0);
    let expected = -(frame.view * (world / world.w)).z;
    assert!((z - expected).abs() < 0.0001, "{z} versus {expected}");
    assert_eq!(
        read(context, renderer.post_water_depth().unwrap().texture()),
        actual
    );
    renderer.select_frame_slot(1);
    assert!(renderer.opaque_snapshot().is_none());
    assert!(renderer.post_water_depth().is_none());
    renderer.select_frame_slot(0);
    assert!(renderer.opaque_snapshot().is_none());
    assert!(renderer.post_water_depth().is_none());
    renderer.resize(context.device(), [16, 16]).unwrap();
    assert!(renderer.post_water_depth().is_none());
}

#[test]
fn independent_readers_omit_unused_attachments_and_producers() {
    let context = pollster::block_on(GpuContext::request()).unwrap();
    for samples in [1, 4] {
        for name in [
            "opaque snapshot color",
            "opaque snapshot depth",
            "post-water depth",
        ] {
            let mut renderer = renderer(&context, samples);
            let depth = name != "opaque snapshot color";
            let view = AttachmentView::new(
                ResourceRef::new(if depth { "scene depth" } else { "scene color" }),
                if depth {
                    renderer.targets().depth().texture()
                } else {
                    renderer.targets().scene().texture()
                },
            )
            .unwrap();
            let source = opaque((!depth).then_some(&view), depth.then_some(&view), 0.5, 0.75);
            let (output, declaration) = consumer(&source, &[name], samples);
            let mut passes = vec![source, output];
            let mut views = vec![view];
            let frame = FrameUniform::new(&fixture_view(), None).unwrap();
            let mut probe = Probe(0);
            let mut encoder = context.submission().begin(context.device()).unwrap();
            renderer
                .encode_frame(
                    context.device(),
                    &mut encoder,
                    (&passes, &views),
                    &mut [BorrowedFeature {
                        declaration,
                        prepared: &mut probe,
                    }],
                    (frame, None),
                )
                .unwrap();
            encoder.finish().submit().unwrap();
            assert_eq!(probe.0, 1);
            assert_only_producer(
                &context,
                renderer.targets().config(),
                frame,
                (&mut passes, &mut views),
                name,
            );

            assert!(renderer.opaque_snapshot().is_none());
            assert_eq!(
                renderer.post_water_depth().is_some(),
                name == "post-water depth"
            );
        }
    }
}

fn assert_only_producer(
    context: &GpuContext,
    config: FrameConfig,
    frame: FrameUniform,
    plan: (&mut Vec<PassDeclaration>, &mut Vec<AttachmentView>),
    name: &'static str,
) {
    let mut snapshots = None;
    let (_, feature) = planning::snapshot_feature(
        context.device(),
        &mut snapshots,
        (config, frame, 0),
        plan,
        &[ResourceRef::new(name)],
        false,
    )
    .unwrap();
    let declaration = feature.unwrap().0;
    assert_eq!(declaration.passes.len(), 1);
    assert_eq!(
        declaration.passes[0].slot,
        match name {
            "opaque snapshot color" => PassSlot::AfterOpaqueSnapshot,
            "opaque snapshot depth" => PassSlot::AfterOpaqueDepth,
            _ => PassSlot::AfterWaterDepth,
        }
    );
}

#[test]
fn malformed_water_plans_return_errors_before_preparation_or_uploads() {
    let context = pollster::block_on(GpuContext::request()).unwrap();
    let mut renderer = renderer(&context, 1);
    renderer
        .install_water(context.device(), settings())
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
    let frame = FrameUniform::new(&fixture_view(), None).unwrap();
    let encoder = encode_fixture(&context, &mut renderer, (&color, &depth), frame, 0.5);
    encoder.finish().submit().unwrap();
    draw_water_slot(&context, &renderer, 0);
    let before = read(&context, renderer.targets().texture());
    let cases = invalid_plans(&color, &depth);
    renderer.set_frame_time(99.0).unwrap();
    let mut invalid_frame = frame;
    invalid_frame.ambient_sky = glam::Vec3::splat(0.75);
    invalid_frame.sun_irradiance = glam::Vec3::splat(10.0);
    for (passes, expected) in cases {
        let mut probe = Probe(0);
        let mut encoder = context.submission().begin(context.device()).unwrap();
        let result = renderer.encode_frame(
            context.device(),
            &mut encoder,
            (&passes, &[color.clone(), depth.clone()]),
            &mut [BorrowedFeature {
                declaration: FeatureDeclaration::new(PassSlot::Opaque),
                prepared: &mut probe,
            }],
            (invalid_frame, None),
        );
        assert_eq!(result, Err(expected));
        assert_eq!(probe.0, 0);
        encoder.finish().submit().unwrap();
        assert_eq!(read(&context, renderer.targets().texture()), before);
        assert!(renderer.opaque_snapshot().is_none());
        assert!(renderer.post_water_depth().is_none());
    }
    draw_water_slot(&context, &renderer, 0);
    assert_eq!(
        read(&context, renderer.targets().texture()),
        before,
        "rejected frames must not change resident water uniforms"
    );
}

fn invalid_plans(
    color: &AttachmentView,
    depth: &AttachmentView,
) -> Vec<(Vec<PassDeclaration>, FrameError)> {
    let original = opaque(Some(color), Some(depth), 0.125, 1.0);
    let mut cases = vec![(Vec::new(), FrameError::MissingSlot(PassSlot::Opaque))];
    for missing_color in [false, true] {
        let mut pass = original.clone();
        if missing_color {
            pass.color = None;
        } else {
            pass.depth = None;
        }
        cases.push((vec![pass], FrameError::AttachmentContract(PassSlot::Opaque)));
    }
    for field in 0..6 {
        let mut pass = original.clone();
        let attachment = pass.depth.as_mut().unwrap();
        let expected = match field {
            0 => {
                attachment.size = [16, 32];
                FrameError::AttachmentContract(PassSlot::Opaque)
            }
            1 => {
                attachment.samples = 4;
                FrameError::AttachmentContract(PassSlot::Opaque)
            }
            2 => {
                attachment.format = wgpu::TextureFormat::R32Float;
                FrameError::AttachmentContract(PassSlot::Opaque)
            }
            3 => {
                attachment.resource.version = 9;
                FrameError::AttachmentView(attachment.resource)
            }
            4 => {
                attachment.store = wgpu::StoreOp::Discard;
                FrameError::LoadAfterDiscard(attachment.resource)
            }
            _ => {
                attachment.load = AttachmentLoad::ClearColor(wgpu::Color::BLACK);
                FrameError::AttachmentContract(PassSlot::Opaque)
            }
        };
        cases.push((vec![pass], expected));
    }
    cases
}
