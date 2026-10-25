use super::*;

#[test]
fn final_foreground_depth_blocks_hits_without_moving_the_opaque_receiver() {
    let context = context();
    let center = (45 * 65 + 32) * 8;
    let clear = final_trace(&context, false, 1.0, false);
    let blocked = final_trace(&context, true, 1.0, false);
    assert!(u16::from_le_bytes(clear[center..center + 2].try_into().unwrap()) > 0);
    assert_eq!(&blocked[center..center + 8], &[0; 8]);
}

#[test]
fn receivers_inside_the_camera_near_plane_have_zero_confidence() {
    let context = context();
    let bytes = final_trace(&context, false, 0.01, false);
    let center = (45 * 65 + 32) * 8;
    assert_eq!(&bytes[center..center + 8], &[0; 8]);
}

fn final_trace(context: &GpuContext, foreground: bool, scale: f32, unmatched: bool) -> Vec<u8> {
    let size = [65, 65];
    let mut screen = SceneMetadata::new(context.device(), size, neutral_ao()).unwrap();
    let ssr = Ssr::new(
        context.device(),
        FrameConfig::new(size, wgpu::TextureFormat::Rgba16Float, 1),
        settings(),
        &mut screen,
    )
    .unwrap();
    let (mut depths, normals, materials, colors) =
        mirror_data(frame(), 0.0, glam::Vec2::new(0.65, 0.35), false);
    for depth in &mut depths {
        *depth *= scale;
    }
    let opaque_depths = depths.clone();
    if unmatched {
        for depth in &mut depths {
            if depth.to_bits() == 5.0f32.to_bits() {
                *depth = 4.5;
            }
        }
    }
    if foreground {
        for y in 33..36 {
            for x in 0..65 {
                depths[y * 65 + x] = 1.0;
            }
        }
    }
    upload_hierarchy(context, &ssr, size, depths);
    upload(context, &ssr.source_depth, &opaque_depths);
    upload(context, &ssr.normals, &normals);
    upload(context, &ssr.material, &materials);
    let opaque =
        crate::ssao::passes::texture(context.device(), size, wgpu::TextureFormat::Rgba16Float);
    upload(context, &opaque, &colors);
    draw_trace(context, &ssr, frame(), &opaque)
}

#[test]
fn final_depth_hierarchy_is_declared_after_water_and_before_trace() {
    let context = context();
    let mut renderer = renderer(&context);
    let ssr = renderer.ssr.as_mut().unwrap();
    let color = AttachmentView::new(ResourceRef::new("scene color"), &ssr.material).unwrap();
    let feature = ReflectionFeature {
        water: None,
        ssr,
        frame: frame(),
        color,
        groups: Vec::new(),
    };
    let mut passes = Vec::new();
    let mut views = Vec::new();
    let declaration = feature.schedule(&mut passes, &mut views).unwrap();
    let first = declaration
        .passes
        .iter()
        .find(|p| p.slot == PassSlot::ReflectionDepthPyramid(0))
        .unwrap();
    assert_eq!(first.reads, vec![FrameProduct::final_depth(1).resource()]);
    assert!(PassSlot::AfterWaterDepth < first.slot);
    assert!(first.slot < PassSlot::ReflectionTrace);
    let trace = declaration
        .passes
        .iter()
        .find(|p| p.slot == PassSlot::ReflectionTrace)
        .unwrap();
    assert!(
        trace
            .reads
            .contains(&FrameProduct::reflection_depth(0, 1).resource())
    );
    assert!(
        !trace
            .reads
            .contains(&FrameProduct::linear_depth(2).resource())
    );
}

#[test]
fn final_depth_partial_bins_are_conservative_at_every_level() {
    let context = context();
    let size = [9, 5];
    let mut screen = SceneMetadata::new(context.device(), size, neutral_ao()).unwrap();
    let ssr = Ssr::new(
        context.device(),
        FrameConfig::new(size, wgpu::TextureFormat::Rgba16Float, 1),
        settings(),
        &mut screen,
    )
    .unwrap();
    let input = crate::ssao::passes::texture(context.device(), size, wgpu::TextureFormat::R32Float);
    let mut values = vec![9.0; 45];
    values[44] = 2.0;
    upload(&context, &input, &values);
    let mut encoder = context.submission().begin(context.device()).unwrap();
    ssr.final_depth
        .encode_for_test(context.device(), &mut encoder, &input);
    encoder.finish().submit().unwrap();
    for texture in ssr.final_depth.textures() {
        let bytes = read(&context, texture);
        let actual: Vec<_> = bytes
            .chunks_exact(4)
            .map(|v| f32::from_le_bytes(v.try_into().unwrap()))
            .collect();
        for (index, depth) in actual.iter().enumerate() {
            let expected = if index + 1 == actual.len() {
                2.0f32
            } else {
                9.0f32
            };
            assert_eq!(
                depth.to_bits(),
                expected.to_bits(),
                "partial footprint at level extent {:?}, bin {index}",
                texture.size()
            );
        }
    }
}

#[test]
fn final_depth_without_matching_surface_metadata_does_not_sample_the_opaque_bed() {
    let context = context();
    let bytes = final_trace(&context, false, 1.0, true);
    let center = (45 * 65 + 32) * 8;
    assert_eq!(&bytes[center..center + 8], &[0; 8]);
}

#[test]
fn final_depth_metadata_retains_the_forward_sample_domain() {
    use crate::frame::metadata::MetadataSampling;
    for samples in [1, 4] {
        for level in 0..7 {
            let product = FrameProduct::reflection_depth(level, samples);
            assert_eq!(
                product.sampling(),
                MetadataSampling::ConservativeResolve(samples)
            );
            assert_eq!(product.sampling().samples(), 1);
        }
    }
}
