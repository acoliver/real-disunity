use super::*;
use glam::{DVec3, Vec3};

#[test]
fn cached_anisotropy_opposing_normals_preserve_first_tap_direction() {
    let context = context();
    let mut fixture = Fixture::new_with(&context, |_, uv| {
        let mut samples = stripes(uv);
        samples[0].normal = [if uv[0] < 0.5 { i16::MAX } else { -i16::MAX }; 3];
        samples
    });
    for key in fixture.leaves {
        fixture.cache.evict(key);
    }
    fixture.images.retain(|(key, _)| *key == PageKey::ROOT);
    let base = &fixture.images[0].1[0];
    assert_eq!(&base.texels()[4 * 40 + 19].normal_roughness[..3], &[201; 3]);
    assert_eq!(&base.texels()[4 * 40 + 20].normal_roughness[..3], &[54; 3]);
    let points = [16.0, 16.125].map(|x| [x / 32.0, 0.25, 2.0 / 32.0, 1.0 / 16.0]);
    let bytes = fixture.sample(
        &context,
        &points,
        Mode {
            normals: true,
            ..mode(2)
        },
    );
    for (point, actual) in points.into_iter().zip(bytes.chunks_exact(16)) {
        assert_direction(actual);
        let (_, keys, normal) = oracle::sample(&fixture, point, 2);
        assert_eq!(keys, [PageKey::ROOT; 2]);
        assert!(normal.is_finite());
        assert!((normal.length() - 1.0).abs() < 1e-12);
        assert!(normal.distance(DVec3::ONE.normalize()) < 1e-12);
    }
    let one = Mode {
        normals: true,
        ..mode(1)
    };
    assert_eq!(
        fixture.sample(&context, &points, one),
        fixture.sample(
            &context,
            &points,
            Mode {
                baseline: true,
                ..one
            }
        )
    );
}

fn assert_direction(bytes: &[u8]) {
    let values: Vec<_> = bytes
        .chunks_exact(4)
        .map(|value| f32::from_le_bytes(value.try_into().unwrap()))
        .collect();
    let normal = Vec3::new(values[0], values[1], values[2]);
    assert_eq!(
        values[3].to_bits(),
        1.0_f32.to_bits(),
        "surface must be finite: {values:?}"
    );
    assert!(normal.is_finite(), "normal={normal:?}");
    assert!((normal.length() - 1.0).abs() < 1e-6, "normal={normal:?}");
    assert!(
        normal.distance(Vec3::ONE.normalize()) < 1e-6,
        "normal={normal:?}"
    );
}
