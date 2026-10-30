use super::*;
use worldrunner_render_plan::{
    SceneWaterFlowCoordinates, SceneWaterSeedCoordinates, TextureHandle,
};

fn field() -> Vec<u8> {
    (0u16..17)
        .flat_map(|y| {
            (0u16..17).flat_map(move |x| {
                [
                    u8::try_from((x * 11 + y * 3 + x * y) % 251).unwrap(),
                    u8::try_from((x * 2 + y * 13 + x * y * 2) % 253).unwrap(),
                ]
            })
        })
        .collect()
}

// Sum the lattice's separable tent basis directly, independently of GPU UV math.
pub(super) fn oracle(point: [f64; 2]) -> [f64; 2] {
    let position = point.map(|v| v.clamp(0.0, 1.0) * 16.0);
    let bytes = field();
    std::array::from_fn(|channel| {
        (0u16..17)
            .flat_map(|y| (0u16..17).map(move |x| (x, y)))
            .map(|(x, y)| {
                let weight = (1.0 - (position[0] - f64::from(x)).abs()).max(0.0)
                    * (1.0 - (position[1] - f64::from(y)).abs()).max(0.0);
                weight * f64::from(bytes[(usize::from(y) * 17 + usize::from(x)) * 2 + channel])
                    / 255.0
            })
            .sum()
    })
}

pub(super) fn body(mode: SceneWaterSeedCoordinates) -> SceneWaterBody {
    let mut body = settings();
    body.material.flow = Some(TextureHandle::from_raw(0));
    body.material.flow_coordinates = Some(SceneWaterFlowCoordinates {
        origin_millimeters: [-5000, -2000],
        tile_millimeters: [10_000, 7000],
        mode,
    });
    body
}

pub(super) fn install_field(context: &GpuContext, water: &mut Water) {
    let texture = context.device().create_texture(&wgpu::TextureDescriptor {
        label: Some("asymmetric endpoint seed lattice"),
        size: wgpu::Extent3d {
            width: 17,
            height: 17,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rg8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    context.queue().write_texture(
        texture.as_image_copy(),
        &field(),
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(34),
            rows_per_image: Some(17),
        },
        texture.size(),
    );
    water.surface_inputs[0][1] = Some(texture.create_view(&wgpu::TextureViewDescriptor::default()));
}

fn probe(context: &GpuContext, mode: SceneWaterSeedCoordinates) -> Vec<u8> {
    let mut body = body(mode);
    let positions = [
        [-5000, -2000],
        [5000, 5000],
        [-4999, 4999],
        [4999, -1999],
        [-2500, 3250],
        [2310, 51],
        [-6000, 800],
        [6000, 2200],
        [-1000, -2700],
        [1000, 5700],
    ];
    body.surface = worldrunner_render_plan::SceneWaterSurface::Mesh {
        positions_millimeters: Box::new(positions),
        indices: (0..10).flat_map(|index| [index; 6]).collect(),
    };
    seed_geometry_tests::probe_geometry(context, body)
}

pub(super) fn render_probe(
    context: &GpuContext,
    renderer: &mut crate::compositor::FrameRenderer,
) -> Vec<u8> {
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
    encode_fixture(context, renderer, (&color, &depth), &frame, 0.5)
        .finish()
        .submit()
        .unwrap();
    read(context, renderer.targets().texture())
}

#[test]
fn endpoint_seed_gpu_matches_independent_clamped_lattice_and_repeat_does_not() {
    let context = pollster::block_on(GpuContext::request()).unwrap();
    let actual = probe(&context, SceneWaterSeedCoordinates::EndpointClamped);
    let repeated = probe(&context, SceneWaterSeedCoordinates::Repeat);
    let points = [
        [0.0, 0.0],
        [1.0, 1.0],
        [1.0 / 10_000.0, 6999.0 / 7000.0],
        [9999.0 / 10_000.0, 1.0 / 7000.0],
        [0.25, 0.75],
        [0.731, 0.293],
        [-0.1, 0.4],
        [1.1, 0.6],
        [0.4, -0.1],
        [0.6, 1.1],
    ];
    let mut repeat_error = 0.0f64;
    for (index, point) in points.into_iter().enumerate() {
        for (channel, expected) in oracle(point).into_iter().enumerate() {
            let offset = (16 * 32 + index) * 8 + channel * 2;
            let value = f64::from(numerical::positive_half(&actual[offset..offset + 2]));
            // One half-float store step, plus 128 f32 rounding steps including UV scaling.
            let tolerance = 1.0 / 1024.0 + 128.0 * f64::from(f32::EPSILON);
            assert!(
                (value - expected).abs() <= tolerance,
                "point {point:?} channel {channel}: {value} vs {expected}"
            );
            let wrong = f64::from(numerical::positive_half(&repeated[offset..offset + 2]));
            repeat_error = repeat_error.max((wrong - expected).abs());
        }
    }
    assert!(repeat_error > 0.1, "repeat must fail the endpoint oracle");
}

#[test]
fn repeat_seed_mode_preserves_default_uniform_bytes() {
    assert_eq!(
        SceneWaterSeedCoordinates::default(),
        SceneWaterSeedCoordinates::Repeat
    );
    let body = body(SceneWaterSeedCoordinates::Repeat);
    let data = terms::uniforms(&body, 0);
    let mut expected = [0.0f32; 32];
    expected[10] = 1.0;
    expected[16..20].copy_from_slice(&[-5.0, -2.0, 0.1, 1.0 / 7.0]);
    assert_eq!(data.map(f32::to_bits), expected.map(f32::to_bits));
    assert_eq!(terms::mask(std::slice::from_ref(&body)), 64);
    let mut clamped = body;
    clamped.material.flow_coordinates.as_mut().unwrap().mode =
        SceneWaterSeedCoordinates::EndpointClamped;
    expected[10] = 2.0;
    assert_eq!(
        terms::uniforms(&clamped, 0).map(f32::to_bits),
        expected.map(f32::to_bits)
    );
    assert_eq!(terms::mask(&[clamped]), 64);
}
