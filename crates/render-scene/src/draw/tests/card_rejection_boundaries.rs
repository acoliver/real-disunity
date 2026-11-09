use super::*;

fn flat_boundary_source(offsets: &[i64]) -> RenderScene {
    let source = fixture();
    let mut textures = source.resources().textures().to_vec();
    let first = textures.len() - 5;
    let SceneTexture::Texels { bytes, .. } = &mut textures[first] else {
        unreachable!()
    };
    bytes.fill(255);
    let mesh = u32::try_from(source.geometry().meshes().len() - 1).unwrap();
    let material = source.geometry().objects()[0].material().raw() + 1;
    let objects = offsets
        .iter()
        .map(|&dy| {
            SceneObject::new(
                SceneTransform {
                    position_millimeters: [OFFSET[0], OFFSET[1] + dy, 500],
                    ..identity()
                },
                MeshHandle::from_raw(mesh),
                MaterialHandle::from_raw(material),
            )
        })
        .collect();
    RenderScene::new(SceneParts {
        resources: SceneResources::new(textures, source.resources().materials().to_vec()).unwrap(),
        geometry: SceneGeometry::new(source.geometry().meshes().to_vec(), objects).unwrap(),
        terrain: source.terrain().cloned(),
        effects: source.effects().cloned(),
    })
    .unwrap()
}

fn compare(context: &GpuContext, source: &RenderScene, pose: CameraPose) -> Vec<Vec<u8>> {
    let old = frame_at_pose(
        context,
        source,
        Some(ReceiverReference::LateCardRejection),
        0.012,
        pose,
    );
    let new = frame_at_pose(context, source, None, 0.012, pose);
    assert_eq!(old.len(), new.len());
    for (index, (old, new)) in old.iter().zip(&new).enumerate() {
        assert_eq!(old.len(), new.len());
        assert_eq!(
            old.iter().zip(new).filter(|(a, b)| a != b).count(),
            0,
            "attachment={index}"
        );
    }
    new
}

fn adjacent_frame(context: &GpuContext, offsets: &[i64]) -> Vec<Vec<u8>> {
    let source = flat_boundary_source(offsets);
    let render = |reference| {
        frame_with_view(context, &source, reference, 0.012, false, |viewport| {
            SceneView::orthographic(
                CameraPose {
                    eye_millimeters: [OFFSET[0], OFFSET[1] - 8_193_000, 500],
                    target_millimeters: [OFFSET[0], OFFSET[1], 500],
                },
                crate::view::OrthographicLens {
                    vertical_half_extent_meters: 2.0,
                    near_meters: 1.0,
                    far_meters: 16_385.0,
                },
                viewport,
            )
            .unwrap()
        })
    };
    let old = render(Some(ReceiverReference::LateCardRejection));
    let new = render(None);
    assert!(old == new, "offsets={offsets:?}");
    new
}

#[test]
fn gpu_card_rejection_preserves_one_ulp_depth_neighbours_and_ties() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    // At north=887m, the integral-mm transforms round 1mm to 2^-10m.
    // The orthographic scale 2^-14 maps that to exactly one ULP above 0.5.
    // Constant plane depth, w=1 and Depth32FloatStencil8 avoid interpolation
    // and depth-format quantization changing those explicitly represented values.
    let north = crate::geometry::millimeter_to_meter(OFFSET[1]);
    let neighbour = crate::geometry::millimeter_to_meter(OFFSET[1] + 1);
    assert_eq!((neighbour - north).to_bits(), 2.0_f32.powi(-10).to_bits());
    let a = adjacent_frame(&context, &[0]);
    let b = adjacent_frame(&context, &[1]);
    let adjacent = a[1]
        .chunks_exact(4)
        .zip(b[1].chunks_exact(4))
        .filter(|(a, b)| {
            let a = u32::from_le_bytes((*a).try_into().unwrap());
            let b = u32::from_le_bytes((*b).try_into().unwrap());
            a == 0.5_f32.to_bits() && b == a + 1
        })
        .count();
    assert!(
        adjacent > 0,
        "fixture must contain depths one f32 ULP apart"
    );
    adjacent_frame(&context, &[0, 1, 0]);
    println!("CARD_REJECTION_ONE_ULP_PIXELS {adjacent}");
}

#[test]
fn gpu_card_rejection_preserves_zero_width_fade_endpoints_from_both_faces() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let base = flat_boundary_source(&[0]);
    let object = base.geometry().objects()[0];
    for terminal in [false, true] {
        let band = SceneFadeBand::new(8_000, 8_000).unwrap();
        let band = if terminal {
            band.with_terminal_fade()
        } else {
            band
        };
        let source = base
            .with_objects_and_effects(vec![object.with_fade(band)], base.effects().cloned())
            .unwrap();
        for distance in [7_999, 8_000, 8_001, -8_000] {
            let pose = CameraPose {
                eye_millimeters: [OFFSET[0], OFFSET[1] - distance, 500],
                target_millimeters: [OFFSET[0], OFFSET[1], 500],
            };
            compare(&context, &source, pose);
        }
    }
}
