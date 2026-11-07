use super::*;
use worldrunner_render_plan::{
    SceneImpostor, SceneImpostorChannels, SceneImpostorLayout, SceneMaterialChannels,
    SceneValidationError, TexelFormat, TextureChannel,
};

fn mapped_material() -> SceneMaterial {
    SceneMaterial::new(SceneMaterialParts {
        albedo: TextureHandle::from_raw(0),
        normal: None,
        roughness_metallic: Some(TextureHandle::from_raw(1)),
        ambient_occlusion: None,
        self_illumination: None,
        roughness_micro: 800_000,
        metalness_micro: 600_000,
        alpha_mode: SceneAlphaMode::Opaque,
        double_sided: true,
    })
    .unwrap()
}

fn mapped_scene(map: SceneTexture, material: &SceneMaterial, atlas: bool) -> RenderScene {
    RenderScene::new(SceneParts {
        resources: SceneResources::new(vec![white_texture(), map], vec![*material]).unwrap(),
        geometry: SceneGeometry::new(
            vec![if atlas {
                card(Some(BillboardPolicy::YawLocked))
            } else {
                quad()
            }],
            vec![SceneObject::new(
                identity(),
                MeshHandle::from_raw(0),
                MaterialHandle::from_raw(0),
            )],
        )
        .unwrap(),
        terrain: None,
        effects: Some(effects()),
    })
    .unwrap()
}

fn packed_bc1(color: u16) -> SceneTexture {
    SceneTexture::Encoded {
        encoding: TextureEncoding::DdsContainer,
        color_space: TextureColorSpace::Linear,
        bytes: bc1_with_format(color, BC1_UNORM).into(),
    }
}

#[test]
fn native_rb_bc1_matches_default_gb_for_mesh_and_impostor_shading() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    for atlas in [false, true] {
        let mut material = mapped_material();
        let mut camera = pose();
        if atlas {
            material = material.with_impostor(
                SceneImpostor::new(SceneImpostorLayout::AzimuthElevation, [1, 1], 0, false)
                    .unwrap(),
            );
            camera = CameraPose {
                eye_millimeters: [0, -4000, 2000],
                target_millimeters: [0, 0, 1000],
            };
        }
        let selected = material
            .with_channels(SceneMaterialChannels {
                roughness: TextureChannel::R,
                metalness: TextureChannel::B,
                ..SceneMaterialChannels::default()
            })
            .unwrap();
        let frame = FrameUniform {
            background_far_plane_meters: 0.0,
            sun_direction: glam::Vec3::new(0.0, -0.6, 0.8),
            sun_irradiance: glam::Vec3::splat(0.2),
            ambient_sky: glam::Vec3::ZERO,
            ambient_ground: glam::Vec3::ZERO,
            ..frame_from(camera)
        };
        let expected = render_frame(
            &context,
            &mapped_scene(packed_bc1(0x07ea), &material, atlas),
            &frame,
        );
        let actual = render_frame(
            &context,
            &mapped_scene(packed_bc1(0xf80a), &selected, atlas),
            &frame,
        );
        let wrong = render_frame(
            &context,
            &mapped_scene(packed_bc1(0xf80a), &material, atlas),
            &frame,
        );
        assert_eq!(
            actual, expected,
            "identical decoded roughness/metalness, atlas={atlas}"
        );
        assert_ne!(
            actual, wrong,
            "roughness selection must affect shaded pixels"
        );
    }
}

#[test]
fn bc4_and_bc5_reject_missing_scalar_lanes_before_upload() {
    let _session = crate::test_support::SceneTestSession::acquire();
    for (code, payload) in [(80_u32, 8), (83, 16)] {
        let mut bytes = bc1_with_format(0, code);
        bytes.resize(148 + payload, 0);
        let map = SceneTexture::Encoded {
            encoding: TextureEncoding::DdsContainer,
            color_space: TextureColorSpace::Linear,
            bytes: bytes.into(),
        };
        assert!(matches!(
            SceneResources::new(vec![white_texture(), map], vec![mapped_material()]),
            Err(SceneValidationError::MissingTextureChannel { .. })
        ));
    }
}

fn rgba(value: [u8; 4], size: u32) -> SceneTexture {
    SceneTexture::Texels {
        format: TexelFormat::Rgba8Unorm,
        color_space: TextureColorSpace::Linear,
        width: size,
        height: size,
        mips: 1,
        bytes: value.repeat(usize::try_from(size * size).unwrap()).into(),
    }
}

#[test]
fn packed_atlas_admission_checks_its_layout_and_any_mesh_fallback() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let dual = || SceneTexture::Texels {
        format: TexelFormat::Rg8Unorm,
        color_space: TextureColorSpace::Linear,
        width: 4,
        height: 4,
        mips: 1,
        bytes: vec![128; 32].into(),
    };
    let resources = SceneResources::new(
        vec![white_texture(), dual(), dual(), rgba([255; 4], 4)],
        vec![auxiliary_material(true, false)],
    )
    .unwrap();
    for billboard in [false, true] {
        let geometry = mapped_scene(rgba([255; 4], 4), &mapped_material(), billboard)
            .geometry()
            .clone();
        let result = RenderScene::new(SceneParts {
            resources: resources.clone(),
            geometry,
            terrain: None,
            effects: None,
        });
        if billboard {
            assert!(result.is_ok());
        } else {
            assert!(matches!(
                result,
                Err(SceneValidationError::MissingTextureChannel { .. })
            ));
        }
    }
    let invalid = auxiliary_material(true, true);
    assert!(matches!(
        SceneResources::new(
            vec![white_texture(), dual(), dual(), rgba([255; 4], 4)],
            vec![invalid],
        ),
        Err(SceneValidationError::MissingTextureChannel { .. })
    ));
}

fn auxiliary_material(packed: bool, custom: bool) -> SceneMaterial {
    let mut material = SceneMaterial::new(SceneMaterialParts {
        albedo: TextureHandle::from_raw(0),
        normal: None,
        roughness_metallic: Some(TextureHandle::from_raw(1)),
        ambient_occlusion: Some(TextureHandle::from_raw(2)),
        self_illumination: Some(TextureHandle::from_raw(3)),
        roughness_micro: 800_000,
        metalness_micro: 600_000,
        alpha_mode: SceneAlphaMode::Opaque,
        double_sided: true,
    })
    .unwrap();
    if custom {
        material = material
            .with_channels(SceneMaterialChannels {
                roughness: TextureChannel::A,
                metalness: TextureChannel::R,
                ambient_occlusion: TextureChannel::B,
                self_illumination: TextureChannel::G,
            })
            .unwrap();
    }
    if packed {
        let mut atlas = SceneImpostor::new(SceneImpostorLayout::AzimuthElevation, [1, 1], 0, false)
            .unwrap()
            .with_channels(SceneImpostorChannels::RoughTranslucencyEmissionOcclusion);
        if custom {
            atlas = atlas
                .with_material_channels(SceneMaterialChannels {
                    roughness: TextureChannel::A,
                    metalness: TextureChannel::B,
                    ambient_occlusion: TextureChannel::A,
                    self_illumination: TextureChannel::B,
                })
                .unwrap();
        }
        material = material.with_impostor(atlas);
    }
    material
}

fn auxiliary_scene(packed: bool, custom: bool) -> RenderScene {
    let rm = match (custom, packed) {
        (true, _) => [51, 7, 51, 153],
        (false, true) => [153, 51, 7, 9],
        (false, false) => [7, 153, 51, 9],
    };
    let ao = match (custom, packed) {
        (true, true) => [7, 9, 102, 204],
        (true, false) => [7, 9, 204, 11],
        (false, true) => [102, 204, 7, 9],
        (false, false) => [204, 7, 9, 11],
    };
    let si = if custom {
        [7, 102, 9, 11]
    } else {
        [102, 7, 9, 11]
    };
    let template = mapped_scene(rgba(rm, 4), &mapped_material(), packed);
    RenderScene::new(SceneParts {
        resources: SceneResources::new(
            vec![white_texture(), rgba(rm, 4), rgba(ao, 4), rgba(si, 4)],
            vec![auxiliary_material(packed, custom)],
        )
        .unwrap(),
        geometry: template.geometry().clone(),
        terrain: None,
        effects: None,
    })
    .unwrap()
}

#[test]
fn auxiliary_and_packed_impostor_selectors_sample_authored_lanes() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let layouts = ScenePipelines::new(context.device(), COLOR_FORMAT);
    let probe = format!(
        "{}\n@fragment fn fs_channels(input: VertexOutput) -> @location(0) vec4<f32> {{ let s = scene_surface_channels(input); let a = scene_auxiliary_samples(input); return vec4<f32>(s, a); }}",
        crate::shader::ENVIRONMENT_WGSL
    );
    for packed in [false, true] {
        let mut results = Vec::new();
        for custom in [false, true] {
            let source = auxiliary_scene(packed, custom);
            let gpu = SceneGpu::upload(context.device(), context.submission(), &source, &layouts)
                .unwrap();
            let camera = if packed {
                CameraPose {
                    eye_millimeters: [0, -4000, 2000],
                    target_millimeters: [0, 0, 1000],
                }
            } else {
                pose()
            };
            gpu.set_frame(context.queue(), frame_from(camera));
            results.push(super::foliage::pass_color(
                &context,
                &gpu,
                &layouts,
                &probe,
                "fs_channels",
            ));
        }
        assert_eq!(results[0], results[1], "packed={packed}");
        assert!(
            results[0]
                .chunks_exact(4)
                .any(|texel| texel[0] != 0 && texel[1] != 0 && texel[2] != 0)
        );
    }
}

#[test]
fn bc7_upload_samples_a_known_mode_six_block() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    // Mode six: equal 7-bit endpoints, zero p-bits and zero interpolation indices.
    // The endpoints therefore decode to (254, 0, 0, 254) for all sixteen texels.
    let mut block = 1_u128 << 6;
    for (index, endpoint) in [127_u128, 127, 0, 0, 0, 0, 127, 127]
        .into_iter()
        .enumerate()
    {
        block |= endpoint << (7 + index * 7);
    }
    for (code, space) in [
        (98_u32, TextureColorSpace::Linear),
        (99, TextureColorSpace::Srgb),
    ] {
        let mut bytes = bc1_with_format(0, code);
        bytes.truncate(148);
        bytes.extend_from_slice(&block.to_le_bytes());
        let map = SceneTexture::Encoded {
            encoding: TextureEncoding::DdsContainer,
            color_space: space,
            bytes: bytes.into(),
        };
        let mut reference = rgba([254, 0, 0, 254], 4);
        if let SceneTexture::Texels { color_space, .. } = &mut reference {
            *color_space = space;
        }
        let scene_for = |albedo| {
            RenderScene::new(SceneParts {
                resources: SceneResources::new(
                    vec![albedo],
                    vec![material(SceneAlphaMode::Opaque, false)],
                )
                .unwrap(),
                geometry: SceneGeometry::new(
                    vec![quad()],
                    vec![SceneObject::new(
                        identity(),
                        MeshHandle::from_raw(0),
                        MaterialHandle::from_raw(0),
                    )],
                )
                .unwrap(),
                terrain: None,
                effects: Some(effects()),
            })
            .unwrap()
        };
        let mut frame = frame_from(pose());
        frame.unlit = true;
        assert_eq!(
            render_frame(&context, &scene_for(map), &frame),
            render_frame(&context, &scene_for(reference), &frame)
        );
    }
}

#[test]
fn shared_r8_and_bc4_channels_match_separate_rgba_lanes_in_shading() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let shared = mapped_material()
        .with_channels(SceneMaterialChannels {
            roughness: TextureChannel::R,
            metalness: TextureChannel::R,
            ..SceneMaterialChannels::default()
        })
        .unwrap();
    let r8 = SceneTexture::Texels {
        format: TexelFormat::R8Unorm,
        color_space: TextureColorSpace::Linear,
        width: 4,
        height: 4,
        mips: 1,
        bytes: vec![128; 16].into(),
    };
    let mut bytes = bc1_with_format(0, 80);
    bytes.truncate(148);
    bytes.extend_from_slice(&[128, 128, 0, 0, 0, 0, 0, 0]);
    let bc4 = SceneTexture::Encoded {
        encoding: TextureEncoding::DdsContainer,
        color_space: TextureColorSpace::Linear,
        bytes: bytes.into(),
    };
    for atlas in [false, true] {
        let mut baseline = mapped_material();
        let mut selected = shared;
        let mut frame = frame_from(pose());
        if atlas {
            let impostor =
                SceneImpostor::new(SceneImpostorLayout::AzimuthElevation, [1, 1], 0, false)
                    .unwrap();
            baseline = baseline.with_impostor(impostor);
            selected = selected.with_impostor(impostor);
            frame = frame_from(CameraPose {
                eye_millimeters: [0, -4000, 2000],
                target_millimeters: [0, 0, 1000],
            });
        }
        let expected = render_frame(
            &context,
            &mapped_scene(rgba([0, 128, 128, 255], 4), &baseline, atlas),
            &frame,
        );
        for map in [r8.clone(), bc4.clone()] {
            assert_eq!(
                render_frame(&context, &mapped_scene(map, &selected, atlas), &frame),
                expected
            );
        }
    }
}

#[test]
fn packed_impostor_scalars_can_share_both_maps_channels() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let baseline = auxiliary_material(true, false);
    let atlas = baseline
        .impostor()
        .unwrap()
        .with_material_channels(SceneMaterialChannels {
            roughness: TextureChannel::R,
            metalness: TextureChannel::R,
            ambient_occlusion: TextureChannel::R,
            self_illumination: TextureChannel::R,
        })
        .unwrap();
    let scene = auxiliary_scene(true, false);
    let build = |material| {
        scene
            .with_resources(
                SceneResources::new(
                    vec![
                        white_texture(),
                        rgba([128; 4], 4),
                        rgba([128; 4], 4),
                        rgba([128; 4], 4),
                    ],
                    vec![material],
                )
                .unwrap(),
                scene.geometry().clone(),
                None,
            )
            .unwrap()
    };
    let frame = frame_from(CameraPose {
        eye_millimeters: [0, -4000, 2000],
        target_millimeters: [0, 0, 1000],
    });
    assert_eq!(
        render_frame(&context, &build(baseline), &frame),
        render_frame(&context, &build(baseline.with_impostor(atlas)), &frame)
    );
}

#[path = "material_channel_bits.rs"]
mod bits;
