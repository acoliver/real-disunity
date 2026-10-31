use super::*;
use worldrunner_render_plan::{TextureAnisotropy, TextureHandle};

fn mapped_body(level: u16) -> SceneWaterBody {
    let mut body = settings();
    body.north_min_millimeters = -32000;
    body.north_extent_millimeters = 64000;
    body.material.normal = Some(TextureHandle::from_raw(0));
    body.material.normal_mips = true;
    body.material.normal_anisotropy = TextureAnisotropy::new(level).unwrap();
    body.material.tile_millimeters = [8000; 2];
    body
}
fn normal_texel(mip: u32, x: u32) -> u8 {
    let band = |width: u32, amplitude: i32| {
        if ((x << mip) / width).is_multiple_of(2) {
            -amplitude
        } else {
            amplitude
        }
    };
    let coarse = if mip < 6 { band(32, 16) } else { 0 };
    let fine = if mip < 3 { band(4, 48) } else { 0 };
    u8::try_from(128 + coarse + fine).unwrap()
}

fn normals(context: &GpuContext) -> wgpu::TextureView {
    let texture = context.device().create_texture(&wgpu::TextureDescriptor {
        label: Some("synthetic water normal stripes"),
        size: wgpu::Extent3d {
            width: 256,
            height: 256,
            depth_or_array_layers: 1,
        },
        mip_level_count: 9,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rg8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    for mip in 0..9 {
        let size = 256 >> mip;
        let bytes: Vec<_> = (0..size * size)
            .flat_map(|i| [normal_texel(mip, i % size), 128])
            .collect();
        context.queue().write_texture(
            wgpu::TexelCopyTextureInfo {
                mip_level: mip,
                ..texture.as_image_copy()
            },
            &bytes,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(size * 2),
                rows_per_image: Some(size),
            },
            wgpu::Extent3d {
                width: size,
                height: size,
                depth_or_array_layers: 1,
            },
        );
    }
    texture.create_view(&wgpu::TextureViewDescriptor::default())
}

fn frame(grazing: bool) -> FrameUniform {
    let mut frame = FrameUniform::new(&fixture_view(), None).unwrap();
    let eye = if grazing {
        glam::Vec3::new(0.0, -32.0, 2.0)
    } else {
        glam::Vec3::new(0.0, -0.01, 2.0)
    };
    frame.view = glam::camera::rh::view::look_at_mat4(eye, glam::Vec3::ZERO, glam::Vec3::Z);
    frame.view_projection =
        glam::camera::rh::proj::directx::orthographic(-0.55, 0.55, -0.55, 0.55, 0.1, 100.0)
            * frame.view;
    frame.camera_position = eye;
    frame
}

fn render(context: &GpuContext, level: u16, grazing: bool, legacy: bool) -> Vec<u8> {
    let mut renderer = regressions::renderer(context, 1);
    renderer
        .install_water(context.device(), mapped_body(level))
        .unwrap();
    let water = renderer.water.as_mut().unwrap();
    water.surface_inputs[0][0] = Some(normals(context));
    if legacy {
        water.surface_samplers[0] = context.device().create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            ..Default::default()
        });
    }
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
    encode_fixture(
        context,
        &mut renderer,
        (&color, &depth),
        &frame(grazing),
        0.5,
    )
    .finish()
    .submit()
    .unwrap();
    read(context, &renderer.water.as_ref().unwrap().receivers[0][0])
}

fn energy(bytes: &[u8]) -> f64 {
    let value = |offset| {
        let bits = u16::from_le_bytes(bytes[offset..offset + 2].try_into().unwrap());
        let magnitude = numerical::positive_half(&(bits & 0x7fff).to_le_bytes());
        f64::from(magnitude) * if bits & 0x8000 == 0 { 1.0 } else { -1.0 }
    };
    (8..24)
        .flat_map(|y| (8..23).map(move |x| (y * 32 + x) * 8))
        .map(|i| (value(i + 8) - value(i)).powi(2))
        .sum()
}

#[test]
fn grazing_water_normal_anisotropy_preserves_minor_axis_detail() {
    let context = pollster::block_on(GpuContext::request()).unwrap();
    let baseline = render(&context, 1, false, false);
    assert!(
        energy(&baseline) > 1.0,
        "non-degenerate isotropic normal map"
    );
    let one = render(&context, 1, true, false);
    assert_eq!(
        one,
        render(&context, 1, true, true),
        "1x retains original sampler bytes"
    );
    let sixteen = render(&context, 16, true, false);
    let low = energy(&one);
    let high = energy(&sixteen);
    assert!(low > 0.01, "non-degenerate grazing 1x baseline: {low}");
    assert_eq!(one, render(&context, 16, true, true));
    eprintln!("water normal horizontal energy: 1x={low}, 16x={high}");
    if context
        .adapter()
        .get_downlevel_capabilities()
        .flags
        .contains(wgpu::DownlevelFlags::ANISOTROPIC_FILTERING)
    {
        assert!(high > low + 1.0, "1x={low}, 16x={high}");
    } else {
        assert_eq!(sixteen, one, "anisotropy must clamp to 1x");
    }
}

#[test]
fn water_surface_samplers_cache_each_validated_level() {
    let context = pollster::block_on(GpuContext::request()).unwrap();
    let bodies = [
        mapped_body(1),
        mapped_body(16),
        mapped_body(16),
        mapped_body(1),
    ];
    let cached = surface::samplers(context.device(), &bodies);
    assert_eq!(cached[0], cached[3]);
    assert_eq!(cached[1], cached[2]);
    assert_ne!(cached[0], cached[1]);
}
