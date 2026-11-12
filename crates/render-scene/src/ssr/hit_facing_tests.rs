use super::*;
use worldrunner_render_plan::{SceneSsrGridDomain, SceneSsrHitFacing, SceneSsrTraversal};

#[derive(Clone, Copy, Debug)]
enum Contact {
    Covered,
    Uncovered,
    WrongOwner,
    Foreground,
}

fn capture(context: &GpuContext, policy: SceneSsr, contact: Contact) -> Vec<u8> {
    let size = [65, 65];
    let mut screen = SceneMetadata::new(context.device(), size, neutral_ao()).unwrap();
    let ssr = Ssr::new(
        context.device(),
        FrameConfig::new(size, wgpu::TextureFormat::Rgba16Float, 1),
        policy,
        &mut screen,
    )
    .unwrap();
    let frame = frame();
    let (depths, mut normals, materials, colors) = mirror_data(
        &frame,
        0.0,
        glam::Vec2::new(0.65, 0.35),
        matches!(contact, Contact::Foreground),
    );
    // The visible single-sided target is the z=-5 plane, facing +Z. Only its
    // published shading normal is reversed; coverage and geometric depth stay fixed.
    for (i, depth) in depths.iter().enumerate() {
        if depth.to_bits() == 5.0f32.to_bits() {
            let coverage = f32::from(u8::from(!matches!(contact, Contact::Uncovered)));
            normals[i * 4..i * 4 + 4].copy_from_slice(&[0.0, 0.0, -1.0, coverage]);
        }
    }
    upload(context, &ssr.normals, &normals);
    upload(context, &ssr.material, &materials);
    upload_hierarchy(context, &ssr, size, depths.clone());
    if matches!(contact, Contact::WrongOwner) {
        let owned: Vec<_> = depths
            .iter()
            .map(|&d| {
                if d.to_bits() == 5.0f32.to_bits() {
                    4.0
                } else {
                    d
                }
            })
            .collect();
        upload(context, &ssr.source_depth, &owned);
    }
    let opaque =
        crate::ssao::passes::texture(context.device(), size, wgpu::TextureFormat::Rgba16Float);
    upload(context, &opaque, &colors);
    draw_trace(context, &ssr, &frame, &opaque)
}

fn receiver(bytes: &[u8], coefficient: u32) -> &[u8] {
    let size = 65u32.div_ceil(coefficient);
    let y = if coefficient == 2 { 23 } else { 45 };
    let offset = usize::try_from((y * size + size / 2) * 8).unwrap();
    &bytes[offset..offset + 8]
}

fn policies() -> impl Iterator<Item = (SceneSsr, u32)> {
    [SceneSsrTraversal::Conservative, SceneSsrTraversal::Grid]
        .into_iter()
        .flat_map(|traversal| {
            [1, 2].map(move |coefficient| {
                (
                    SceneSsr {
                        traversal,
                        grid_domain: SceneSsrGridDomain::ReflectionOutput,
                        resolution_scale_micro: 1_000_000 / coefficient,
                        ..settings()
                    },
                    coefficient,
                )
            })
        })
}

fn assert_facing(traversal: SceneSsrTraversal, coefficient: u32) {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let policy = policies()
        .find(|(p, c)| p.traversal == traversal && *c == coefficient)
        .unwrap()
        .0;
    let rejected = capture(&context, policy, Contact::Covered);
    assert_eq!(receiver(&rejected, coefficient), [0; 8]);
    let admitted = capture(
        &context,
        SceneSsr {
            hit_facing: SceneSsrHitFacing::Any,
            ..policy
        },
        Contact::Covered,
    );
    let pixel = receiver(&admitted, coefficient);
    let confidence = ::half::f16::from_le_bytes(pixel[6..8].try_into().unwrap()).to_f32();
    assert!(confidence > 0.0, "{policy:?}, coefficient {coefficient}");
    assert_eq!(&pixel[..2], &pixel[6..8], "red target times confidence");
    assert_eq!(&pixel[2..6], [0; 4]);
}

#[test]
fn gpu_hit_facing_conservative_full() {
    assert_facing(SceneSsrTraversal::Conservative, 1);
}

#[test]
fn gpu_hit_facing_conservative_half() {
    assert_facing(SceneSsrTraversal::Conservative, 2);
}

#[test]
fn gpu_hit_facing_grid_full() {
    assert_facing(SceneSsrTraversal::Grid, 1);
}

#[test]
fn gpu_hit_facing_grid_half() {
    assert_facing(SceneSsrTraversal::Grid, 2);
}

#[test]
fn gpu_hit_facing_any_preserves_coverage_ownership_and_foreground_rejection() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    for (policy, coefficient) in policies() {
        let policy = SceneSsr {
            hit_facing: SceneSsrHitFacing::Any,
            ..policy
        };
        for contact in [Contact::Uncovered, Contact::WrongOwner, Contact::Foreground] {
            let bytes = capture(&context, policy, contact);
            assert_eq!(
                receiver(&bytes, coefficient),
                [0; 8],
                "{contact:?}, {policy:?}, coefficient {coefficient}"
            );
        }
    }
}

#[test]
fn default_hit_facing_source_is_byte_identical_to_parent_generator() {
    for samples in [1, 4] {
        for water in [false, true] {
            let parent = gpu::trace_source(samples, water);
            assert_eq!(
                parent
                    .matches("|| dot(hit_normal.xyz, direction) >= 0.0")
                    .count(),
                2
            );
            let generated = grid::trace_source(samples, water, &SceneSsr::default(), [65, 65]);
            assert_eq!(generated.as_bytes(), parent.as_bytes());
        }
    }
}

#[test]
fn hit_facing_any_changes_only_the_two_direction_tests() {
    for (policy, _) in policies() {
        for samples in [1, 4] {
            for water in [false, true] {
                let shading = grid::trace_source(samples, water, &policy, [65, 65]);
                let predicate = " || dot(hit_normal.xyz, direction) >= 0.0";
                assert_eq!(shading.matches(predicate).count(), 2);
                let any = grid::trace_source(
                    samples,
                    water,
                    &SceneSsr {
                        hit_facing: SceneSsrHitFacing::Any,
                        ..policy
                    },
                    [65, 65],
                );
                assert_eq!(any.as_bytes(), shading.replace(predicate, "").as_bytes());
            }
        }
    }
}
