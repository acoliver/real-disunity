use super::*;

#[test]
fn corner_colours_split_shared_geometry_groups() {
    let first = DynamicQuad {
        overlay: true,
        ..DynamicQuad::default()
    };
    let mut second = first;
    second.corner_colors = Some([[0.5; 4]; 4]);
    assert!(!compatible(&first, &second));
    assert!(!compatible(&second, &first));
    assert!(compatible(&second, &second));
    second.tint = [0.25; 4];
    second.position = [2., 0., 0.];
    assert!(compatible(
        &DynamicQuad {
            tint: [1.; 4],
            position: [0.; 3],
            ..second
        },
        &second
    ));
}

#[test]
fn overlay_corner_stream_packs_white_and_explicit_quad_and_triangle_values() {
    for triangle_uv in [None, Some([[0.; 2]; 3])] {
        let count = if triangle_uv.is_some() { 3 } else { 4 };
        let mut quad = DynamicQuad {
            overlay: true,
            triangle_uv,
            ..DynamicQuad::default()
        };
        assert_eq!(
            pack_corner_colors(&quad),
            [1f32.to_le_bytes(); 16][..count * 4].concat()
        );
        let colors = std::array::from_fn(|i| {
            std::array::from_fn(|c| f32::from(u8::try_from(i * 4 + c).unwrap()) / 16.)
        });
        quad.corner_colors = Some(colors);
        let bytes = pack_corner_colors(&quad);
        assert_eq!(bytes.len(), count * 16);
        let actual: Vec<_> = bytes
            .chunks_exact(4)
            .map(|b| f32::from_le_bytes(b.try_into().unwrap()))
            .collect();
        assert_eq!(
            actual,
            colors[..count]
                .iter()
                .flatten()
                .copied()
                .collect::<Vec<_>>()
        );
        assert_eq!(
            pack_quad(&quad).len(),
            count * 56,
            "shared geometry stride is unchanged"
        );
    }
}

#[test]
fn adjacent_corner_groups_preserve_submission_order_and_instance_tint() {
    let a = DynamicQuad {
        overlay: true,
        corner_colors: Some([[0.25; 4]; 4]),
        ..DynamicQuad::default()
    };
    let b = DynamicQuad {
        corner_colors: Some([[0.5; 4]; 4]),
        ..a
    };
    let quads = [
        a,
        DynamicQuad {
            tint: [0.1; 4],
            ..a
        },
        b,
        a,
    ];
    let groups: Vec<_> = quads
        .chunk_by(compatible)
        .map(<[DynamicQuad]>::len)
        .collect();
    assert_eq!(groups, [2, 1, 1]);
}

#[test]
fn only_overlay_entries_consume_corner_colour_location() {
    for depth in [false, true] {
        let module =
            naga::front::wgsl::parse_str(&crate::renderer::overlay::source(depth)).unwrap();
        naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::all(),
        )
        .validate(&module)
        .unwrap();
        let entry = module
            .entry_points
            .iter()
            .find(|e| e.name == "vs_ui_overlay")
            .unwrap();
        let corner = entry
            .function
            .arguments
            .iter()
            .find(|arg| {
                matches!(
                    arg.binding,
                    Some(naga::Binding::Location { location: 12, .. })
                )
            })
            .unwrap();
        assert!(matches!(
            module.types[corner.ty].inner,
            naga::TypeInner::Vector {
                size: naga::VectorSize::Quad,
                scalar: naga::Scalar {
                    kind: naga::ScalarKind::Float,
                    width: 4
                }
            }
        ));
        let scene = module
            .types
            .iter()
            .find(|(_, ty)| ty.name.as_deref() == Some("VertexInput"))
            .unwrap()
            .1;
        let naga::TypeInner::Struct { members, .. } = &scene.inner else {
            panic!("scene vertex struct")
        };
        assert!(!members.iter().any(|m| matches!(
            m.binding,
            Some(naga::Binding::Location { location: 12, .. })
        )));
        assert_eq!(crate::geometry::VERTEX_STRIDE, 56);
    }
}
