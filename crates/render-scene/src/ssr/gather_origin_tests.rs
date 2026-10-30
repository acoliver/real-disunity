use super::*;

const MAXIMUM_SIZE: u32 = 256;

/// One row per frame size 1..=256 and one column per pixel. Each pixel reports
/// the production `gather_origin`, offset by one so that the origin -1 stays
/// representable in a half float (all values up to 2048 are exact).
#[expect(
    clippy::cast_possible_truncation,
    reason = "the half holds an integer origin + 1 in 0..=257, exact in f64 and i32"
)]
fn origins(context: &GpuContext, extent: &str) -> Vec<(u32, u32, i32)> {
    let shader = format!(
        "{}
@fragment fn fs_screen(@builtin(position) p: vec4<f32>) -> @location(0) vec4<f32> {{
    let size = i32(p.y);
    let pixel = i32(p.x);
    if pixel >= size {{ return vec4<f32>(0.0); }}
    let extent = {extent};
    let exact = gather_origin(vec2<i32>(pixel), vec2<i32>(size), vec2<i32>(extent));
    return vec4<f32>(f32(exact.x + 1), 0.0, 0.0, 1.0);
}}",
        include_str!("gather_origin.wgsl")
    );
    let pipeline = gpu::pipeline(context.device(), &shader, 1, false);
    let target = crate::ssao::passes::texture(
        context.device(),
        [MAXIMUM_SIZE, MAXIMUM_SIZE + 1],
        wgpu::TextureFormat::Rgba16Float,
    );
    let mut encoder = context.submission().begin(context.device()).unwrap();
    worldrunner_render_post::fullscreen_pass(
        &mut encoder,
        None,
        wgpu::RenderPassColorAttachment {
            view: &target.create_view(&wgpu::TextureViewDescriptor::default()),
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                store: wgpu::StoreOp::Store,
            },
        },
        &pipeline,
        &[],
    );
    encoder.finish().submit().unwrap();
    let bytes = read(context, &target);
    let mut rows = Vec::new();
    for size in 1..=MAXIMUM_SIZE {
        for pixel in 0..size {
            let offset = ((size * MAXIMUM_SIZE + pixel) * 8) as usize;
            let origin = super::ao_accuracy_tests::half_value(&bytes[offset..offset + 2]);
            rows.push((size, pixel, origin as i32 - 1));
        }
    }
    rows
}

/// floor((2 pixel + 1) extent / (2 size) - 1/2) with `i64` division.
fn exact_origin(size: u32, pixel: u32, extent: u32) -> i32 {
    let numerator = i64::from((2 * pixel + 1) * extent) - i64::from(size);
    i32::try_from(numerator.div_euclid(2 * i64::from(size))).unwrap()
}

/// The float form floor((pixel + 0.5) / size * extent - 0.5) differs from the
/// exact value in thousands of aligned cases on RADV (including a 3 pixel frame)
/// and in about a thousand on llvmpipe, so this test uses exact integers only.
#[test]
fn gpu_gather_origin_is_exact_integer_floor_division_on_every_grid() {
    let context = context();
    for (label, extent) in [
        ("aligned", "size"),
        ("half", "(size + 1) / 2"),
        ("quarter", "(size + 3) / 4"),
    ] {
        for (size, pixel, origin) in origins(&context, extent) {
            let grid = match label {
                "aligned" => size,
                "half" => size.div_ceil(2),
                _ => size.div_ceil(4),
            };
            assert_eq!(
                origin,
                exact_origin(size, pixel, grid),
                "{label}: pixel {pixel} of {size} on a grid of {grid}"
            );
        }
    }
}
