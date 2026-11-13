//! Full attachment differentials against the normal-dependent receiver producer.
use super::*;

#[test]
fn degenerate_normal_frame_fixture_has_constant_uvs_and_no_authored_tangent() {
    for mapped in [false, true] {
        let source = degenerate_normal_frame(mapped);
        let mesh = source.geometry().meshes().last().unwrap();
        assert!(
            mesh.lods()[0].vertices().iter().all(|vertex| {
                vertex.tangent_micro.is_none() && vertex.texcoord_micro == [0; 2]
            })
        );
        assert_eq!(
            source
                .resources()
                .materials()
                .last()
                .unwrap()
                .normal()
                .is_some(),
            mapped
        );
    }
}

fn opaque_receivers(source: &RenderScene) -> RenderScene {
    let resources = SceneResources::new(
        source.resources().textures().to_vec(),
        source
            .resources()
            .materials()
            .iter()
            .copied()
            .map(|material| material.with_translucency(0).unwrap())
            .collect(),
    )
    .unwrap();
    source
        .with_resources(
            resources,
            source.geometry().clone(),
            source.effects().cloned(),
        )
        .unwrap()
}

fn degenerate_normal_frame(mapped: bool) -> RenderScene {
    let source = sloped(false);
    let vertices = [
        [-2000, 0, 0],
        [2000, 0, 0],
        [-2000, 0, 4000],
        [2000, 0, 4000],
    ]
    .into_iter()
    .map(|position_millimeters| SceneVertex {
        position_millimeters,
        normal_micro: [0, -1_000_000, 0],
        tangent_micro: None,
        texcoord_micro: [0; 2],
    })
    .collect();
    let mesh = SceneMesh::new(vec![
        SceneMeshLod::new(0, vertices, vec![0, 1, 2, 2, 1, 3], None).unwrap(),
    ])
    .unwrap();
    let mut textures = source.resources().textures().to_vec();
    let normal = TextureHandle::from_raw(u32::try_from(textures.len()).unwrap());
    textures.push(SceneTexture::Texels {
        format: TexelFormat::Rgba8Unorm,
        color_space: TextureColorSpace::Linear,
        width: 2,
        height: 2,
        mips: 1,
        bytes: [128, 128, 255, 255].repeat(4).into(),
    });
    let mut materials = source.resources().materials().to_vec();
    let material = MaterialHandle::from_raw(u32::try_from(materials.len()).unwrap());
    materials.push(
        SceneMaterial::new(SceneMaterialParts {
            albedo: TextureHandle::from_raw(0),
            normal: mapped.then_some(normal),
            roughness_metallic: None,
            ambient_occlusion: None,
            self_illumination: None,
            roughness_micro: 700_000,
            metalness_micro: 0,
            alpha_mode: SceneAlphaMode::Opaque,
            double_sided: true,
        })
        .unwrap(),
    );
    let mut meshes = source.geometry().meshes().to_vec();
    let handle = MeshHandle::from_raw(u32::try_from(meshes.len()).unwrap());
    meshes.push(mesh);
    let object = SceneObject::new(
        SceneTransform {
            position_millimeters: [OFFSET[0], OFFSET[1], 300],
            ..identity()
        },
        handle,
        material,
    );
    RenderScene::new(SceneParts {
        resources: SceneResources::new(textures, materials).unwrap(),
        geometry: SceneGeometry::new(meshes, vec![object]).unwrap(),
        terrain: source.terrain().cloned(),
        effects: source.effects().cloned(),
    })
    .unwrap()
}

fn compare(context: &GpuContext, source: &RenderScene, bias: f32, distance: i64, name: &str) {
    let mut pose = receiver_pose();
    pose.eye_millimeters[1] = OFFSET[1] - distance;
    let old = frame_at_pose_motion(
        context,
        source,
        Some(ReceiverReference::MainArithmetic),
        bias,
        pose,
        true,
    );
    let new = frame_at_pose_motion(context, source, None, bias, pose, true);
    assert_eq!(old.len(), new.len());
    for (attachment, (a, b)) in old.iter().zip(&new).enumerate() {
        assert_eq!(a.len(), b.len());
        assert_eq!(
            a.iter().zip(b).filter(|(a, b)| a != b).count(),
            0,
            "{name} bias={bias} distance={distance} attachment={attachment}"
        );
    }
    assert!(new[new.len() - 2].iter().any(|byte| *byte != 0));
    let normal = frame_at_pose_motion(
        context,
        source,
        Some(ReceiverReference::NormalProducer),
        bias,
        pose,
        true,
    );
    assert_eq!(
        normal, new,
        "Normal and selected producers must agree: {name}"
    );
    export(name, bias, distance, [("main", &old), ("position", &new)]);
    println!(
        "PRODUCER_EXACTNESS {name} bias={bias} distance={distance} attachments={} differing_bytes=0",
        new.len()
    );
}

fn export(name: &str, bias: f32, distance: i64, variants: [(&str, &[Vec<u8>]); 2]) {
    if let Some(directory) = std::env::var_os("WR_RECEIVER_FIXTURE_OUTPUT") {
        let directory = std::path::PathBuf::from(directory);
        std::fs::create_dir_all(&directory).unwrap();
        for (variant, products) in variants {
            for (index, bytes) in products.iter().enumerate() {
                std::fs::write(
                    directory.join(format!(
                        "{name}-{:08x}-{distance}-{variant}-{index}.bin",
                        bias.to_bits()
                    )),
                    bytes,
                )
                .unwrap();
            }
        }
    }
}

#[test]
fn gpu_zero_bias_receiver_producer_preserves_fixture_payload_attachment_and_output_bits() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    for (name, source) in [
        ("slope", sloped(false)),
        ("occlusion-relief", opaque_receivers(&sloped(true))),
        ("transmission-relief", sloped(true)),
    ] {
        for bias in [0.0, -0.0, 0.012] {
            compare(&context, &source, bias, 8_000, name);
        }
    }
    let source = opaque_receivers(&card_rejection::fixture());
    for distance in [8_000, 16_000, 31_997] {
        for bias in [0.0, 0.012] {
            compare(
                &context,
                &source,
                bias,
                distance,
                "holes-fades-decals-mips-cascades",
            );
        }
    }
}

/// A clear changes only texels no producer won. Requiring every downstream byte
/// unchanged checks ownership even for helper lanes that execute payload loads;
/// the payload itself must change so a missing clear cannot make the test pass.
#[test]
fn gpu_receiver_consumers_do_not_publish_uncovered_payload_values() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let source = opaque_receivers(&card_rejection::fixture());
    let cleared = frame(&context, &source, None, 0.0);
    let poison = frame(
        &context,
        &source,
        Some(ReceiverReference::PoisonUncovered),
        0.0,
    );
    let payload = cleared.len() - 2;
    for (index, (a, b)) in cleared.iter().zip(&poison).enumerate() {
        let differences = a.iter().zip(b).filter(|(a, b)| a != b).count();
        println!("UNCOVERED_RECEIVER attachment={index} differing_bytes={differences}");
    }
    export(
        "uncovered-payload",
        0.0,
        8_000,
        [("main", &cleared), ("position", &poison)],
    );
    assert!(
        cleared[payload..] != poison[payload..],
        "control must change uncovered payload texels"
    );
    assert!(
        cleared[..payload] == poison[..payload],
        "uncovered payload must not affect any downstream attachment or output"
    );
}

/// Keep the repeatability control separate so the poison test retains its cold
/// two-frame ordering and cannot pass merely because an extra draw warmed a cache.
#[test]
fn gpu_receiver_zero_clear_frames_are_deterministic() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let source = opaque_receivers(&card_rejection::fixture());
    let cleared = frame(&context, &source, None, 0.0);
    let repeated = frame(&context, &source, None, 0.0);
    export(
        "uncovered-payload-repeat",
        0.0,
        8_000,
        [("main", &cleared), ("position", &repeated)],
    );
    assert!(
        cleared == repeated,
        "the uncovered-payload fixture must be deterministic before changing its clear"
    );
}

/// Zero-bias payloads must be finite dP and agree across producer paths even when
/// mapped normals are nonfinite. WGSL permits backend-dependent nonfinite arithmetic;
/// Metal need not reproduce NaN payloads from main's normal-dependent expression.
#[test]
fn gpu_degenerate_normal_frame_zero_bias_has_finite_position_derivatives() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let source = degenerate_normal_frame(true);
    let geometric = degenerate_normal_frame(false);
    for bias in [0.0, -0.0] {
        let old = frame(
            &context,
            &source,
            Some(ReceiverReference::MainArithmetic),
            bias,
        );
        let new = frame(&context, &source, None, bias);
        let normal = frame(
            &context,
            &source,
            Some(ReceiverReference::NormalProducer),
            bias,
        );
        let expected = frame(&context, &geometric, None, bias);
        let payload = new.len() - 2;
        assert_eq!(new, normal, "zero-bias Normal and Position must agree");
        assert_eq!(
            new[payload..],
            expected[payload..],
            "derivatives must depend only on position"
        );
        let nonfinite = |products: &[Vec<u8>]| {
            products[payload..]
                .iter()
                .flat_map(|bytes| bytes.chunks_exact(4))
                .filter(|bytes| !f32::from_le_bytes((*bytes).try_into().unwrap()).is_finite())
                .count()
        };
        let main_nonfinite = nonfinite(&old);
        assert_eq!(nonfinite(&new), 0);
        assert!(
            new[2]
                .chunks_exact(2)
                .any(|bytes| u16::from_le_bytes(bytes.try_into().unwrap()) & 0x7c00 == 0x7c00),
            "fixture must reach nonfinite mapped normals"
        );
        for (index, (a, b)) in old.iter().zip(&new).enumerate() {
            let differences = a.iter().zip(b).filter(|(a, b)| a != b).count();
            println!(
                "DEGENERATE_NORMAL_FRAME bias={:08x} attachment={index} differing_bytes={differences} main_nonfinite_payload={main_nonfinite}",
                bias.to_bits()
            );
        }
        export(
            "degenerate-normal-frame",
            bias,
            8_000,
            [("main", &old), ("position", &new)],
        );
    }
    compare(
        &context,
        &source,
        0.012,
        8_000,
        "degenerate-normal-frame-nonzero",
    );
}
