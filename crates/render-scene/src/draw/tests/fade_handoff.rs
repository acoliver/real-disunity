use super::foliage::{geometry_sources, pass_depth};
use super::*;
use worldrunner_render_plan::{
    SceneFadeBand, SceneFadeCoverage, SceneFadeDirection, SceneFadeQuantization,
    SceneFadeReference, SceneImpostor, SceneImpostorLayout, TexelFormat,
};

pub(super) fn handoff_scene(
    hashed: bool,
    impostor: bool,
    directions: &[SceneFadeDirection],
) -> RenderScene {
    let base = card_scene(Some(BillboardPolicy::YawLocked), identity().rotation);
    let threshold = SceneTexture::Texels {
        format: TexelFormat::R8Unorm,
        color_space: TextureColorSpace::Linear,
        width: 2,
        height: 2,
        mips: 1,
        bytes: vec![0, 127, 128, 255].into(),
    };
    let mut material =
        base.resources().materials()[0].with_fade_reference(SceneFadeReference::new([0, 0, 1000]));
    if !hashed {
        material = material.with_fade_coverage(SceneFadeCoverage::Threshold {
            texture: TextureHandle::from_raw(1),
            jitter_pixels: [0, 0],
        });
    }
    if impostor {
        material = material
            .with_fade_quantization(SceneFadeQuantization::Byte)
            .with_impostor(
                SceneImpostor::new(SceneImpostorLayout::AzimuthElevation, [1, 1], 0, false)
                    .unwrap(),
            );
    }
    let materials = directions
        .iter()
        .map(|d| material.with_fade_direction(*d))
        .collect();
    let objects = directions
        .iter()
        .enumerate()
        .map(|(i, direction)| {
            let band = SceneFadeBand::new(2000, 6000).unwrap();
            let band = if *direction == SceneFadeDirection::Keep {
                band.with_terminal_fade()
            } else {
                band
            };
            SceneObject::new(
                identity(),
                MeshHandle::from_raw(0),
                MaterialHandle::from_raw(u32::try_from(i).unwrap()),
            )
            .with_fade(band)
        })
        .collect();
    RenderScene::new(SceneParts {
        resources: SceneResources::new(vec![white_texture(), threshold], materials).unwrap(),
        geometry: SceneGeometry::new(base.geometry().meshes().to_vec(), objects).unwrap(),
        terrain: None,
        effects: base.effects().cloned(),
    })
    .unwrap()
}

fn threshold(x: u32, y: u32, hashed: bool) -> f64 {
    if !hashed {
        return f64::from([0_u8, 127, 128, 255][((y % 2) * 2 + x % 2) as usize]) / 255.0;
    }
    let mut hash = x
        .wrapping_mul(1_664_525)
        .wrapping_add(y.wrapping_mul(1_013_904_223));
    hash = (hash ^ (hash >> 16)).wrapping_mul(2_246_822_519);
    (f64::from(hash & 65535) + 0.5) / 65536.0
}

pub(super) fn mask(
    context: &GpuContext,
    layouts: &ScenePipelines,
    scene: &RenderScene,
    source: &str,
    shadow: bool,
    distance: i64,
) -> Vec<u8> {
    let gpu = SceneGpu::upload(context.device(), context.submission(), scene, layouts).unwrap();
    gpu.set_frame(
        context.queue(),
        frame_from(CameraPose {
            eye_millimeters: [0, -distance, 1000],
            target_millimeters: [0, 0, 1000],
        }),
    );
    pass_depth(
        context,
        &gpu,
        layouts,
        source,
        if shadow {
            "fs_depth_opaque"
        } else {
            "fs_opaque"
        },
        !shadow,
    )
}

#[test]
fn separate_objects_partition_forward_and_shadow_without_holes_or_overlap() {
    let context = context();
    let layouts = ScenePipelines::new(context.device(), COLOR_FORMAT);
    for (hashed, impostor) in [(true, false), (false, false), (false, true)] {
        let lower = handoff_scene(hashed, impostor, &[SceneFadeDirection::Keep]);
        let upper = handoff_scene(hashed, impostor, &[SceneFadeDirection::Complement]);
        let both = handoff_scene(
            hashed,
            impostor,
            &[SceneFadeDirection::Keep, SceneFadeDirection::Complement],
        );
        for distance in [1000, 2000, 4000, 6000, 7000] {
            let boundary =
                ((6000.0 - f64::from(i32::try_from(distance).unwrap())) / 4000.0).clamp(0.0, 1.0);
            let boundary = if impostor {
                (boundary * 255.0).trunc() / 255.0
            } else {
                boundary
            };
            for pass in [0, 3] {
                let a = mask(
                    &context,
                    &layouts,
                    &lower,
                    &geometry_sources()[pass],
                    pass == 3,
                    distance,
                );
                let b = mask(
                    &context,
                    &layouts,
                    &upper,
                    &geometry_sources()[pass],
                    pass == 3,
                    distance,
                );
                let union = mask(
                    &context,
                    &layouts,
                    &both,
                    &geometry_sources()[pass],
                    pass == 3,
                    distance,
                );
                assert_partition_masks(&a, &b, &union, hashed, boundary);
            }
        }
    }
}

#[test]
fn default_direction_keeps_uniform_storage_instance_and_render_bytes() {
    let context = context();
    let base = card_scene(Some(BillboardPolicy::YawLocked), identity().rotation);
    let material = base.resources().materials()[0];
    let explicit = material.with_fade_direction(SceneFadeDirection::Inclusive);
    assert_eq!(
        crate::pipeline::MaterialUniform::from_material(&material).to_bytes(),
        crate::pipeline::MaterialUniform::from_material(&explicit).to_bytes()
    );
    assert_eq!(
        crate::geometry::pack_object(base.geometry().objects()[0]).len(),
        80
    );
    let mut storage = Vec::new();
    super::super::fade::append(&base, &mut storage);
    assert!(storage.is_empty());
    assert_eq!(
        super::super::fade::uniform(&base, &material),
        super::super::fade::uniform(&base, &explicit)
    );
    let expected = render(&context, &base);
    let scene = RenderScene::new(SceneParts {
        resources: SceneResources::new(base.resources().textures().to_vec(), vec![explicit])
            .unwrap(),
        geometry: base.geometry().clone(),
        terrain: None,
        effects: base.effects().cloned(),
    })
    .unwrap();
    assert_eq!(expected, render(&context, &scene));
}

#[test]
fn local_reference_rotates_scales_and_clamps_eye_at_literal_fade_boundaries() {
    let context = context();
    let layouts = ScenePipelines::new(context.device(), COLOR_FORMAT);
    for (scale, height, north) in [(500_000, 500, 1000), (1_500_000, 1500, 3000)] {
        for rotated in [false, true] {
            let placement = SceneTransform {
                scale_millionths: scale,
                rotation: if rotated {
                    SceneRotation {
                        x: 0,
                        y: 0,
                        z: 1_000_000,
                        w: 0,
                    }
                } else {
                    identity().rotation
                },
                ..identity()
            };
            let local = if rotated {
                [-1000, -2000, 1000]
            } else {
                [1000, 2000, 1000]
            };
            let reference = SceneFadeReference::new(local);
            for (distance, below, clamp, expected) in [
                (2000, 0, false, 0.0),
                (4000, 0, false, 0.5),
                (6000, 0, false, 1.0),
                (3000, 4000, false, 0.75),
                (3000, 4000, true, 0.25),
            ] {
                let reference = if clamp {
                    reference.with_eye_vertical_clamp(0).unwrap()
                } else {
                    reference
                };
                assert_reference_mask(
                    &context,
                    &layouts,
                    placement,
                    reference,
                    [height, north - distance, height - below],
                    expected,
                );
            }
        }
    }
}

fn assert_reference_mask(
    context: &GpuContext,
    layouts: &ScenePipelines,
    placement: SceneTransform,
    reference: SceneFadeReference,
    eye: [i64; 3],
    expected: f64,
) {
    let base = handoff_scene(false, false, &[SceneFadeDirection::Keep]);
    let material = base.resources().materials()[0].with_fade_reference(reference);
    let object = base.geometry().objects()[0]
        .with_transform(placement)
        .with_fade(SceneFadeBand::new(2000, 6000).unwrap());
    let scene = RenderScene::new(SceneParts {
        resources: SceneResources::new(base.resources().textures().to_vec(), vec![material])
            .unwrap(),
        geometry: SceneGeometry::new(base.geometry().meshes().to_vec(), vec![object]).unwrap(),
        terrain: None,
        effects: base.effects().cloned(),
    })
    .unwrap();
    let gpu = SceneGpu::upload(context.device(), context.submission(), &scene, layouts).unwrap();
    let height = i64::try_from(placement.scale_millionths).unwrap() / 1000;
    gpu.set_frame(
        context.queue(),
        frame_from(CameraPose {
            eye_millimeters: eye,
            target_millimeters: [0, 0, height],
        }),
    );
    let pixels = pass_depth(
        context,
        &gpu,
        layouts,
        crate::shader::ENVIRONMENT_WGSL,
        "fs_opaque",
        true,
    );
    for y in 31..33 {
        for x in 31..33 {
            let offset = ((y * TARGET + x) * 4) as usize;
            let actual = f32::from_le_bytes(pixels[offset..offset + 4].try_into().unwrap()) < 1.0;
            assert_eq!(
                actual,
                threshold(x, y, false) < expected,
                "eye={eye:?} placement={placement:?} reference={reference:?} pixel={x},{y}"
            );
        }
    }
}

pub(super) fn assert_partition_masks(
    a: &[u8],
    b: &[u8],
    union: &[u8],
    hashed: bool,
    boundary: f64,
) {
    for y in 30..34 {
        for x in 30..34 {
            let offset = ((y * TARGET + x) * 4) as usize;
            let covered = |bytes: &[u8]| {
                f32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap()) < 1.0
            };
            let expected = threshold(x, y, hashed) < boundary;
            assert_eq!(
                covered(a),
                expected,
                "lower pixel={x},{y} boundary={boundary}"
            );
            assert_eq!(
                covered(b),
                !expected,
                "upper pixel={x},{y} boundary={boundary}"
            );
            assert!(covered(union));
        }
    }
}
