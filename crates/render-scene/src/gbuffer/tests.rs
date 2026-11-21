//! CPU checks of the G-buffer contract: the WGSL interface, the quantisation of the
//! encoding (through the reference mirror) and the slot's depth declaration.
use glam::{Mat3, Vec3};
use worldrunner_render_post::{
    AttachmentDeclaration, AttachmentLoad, FeatureDeclaration, FeaturePass, FrameError,
    PassDeclaration, PassEntry, PassSlot, ResourceRef,
};

use super::reference::{self, Surface, angle_degrees, sphere};
use super::{GBUFFER_FORMATS, GBUFFER_WGSL, GbufferTarget};

/// Shading-normal bounds in degrees by the view-space z of the normal: camera-facing
/// (z >= 0), z >= -0.5, z >= -0.9, and anywhere, including the singular point z = -1.
pub(crate) fn shading_bound(view_z: f32) -> f32 {
    if view_z >= 0.0 {
        0.25
    } else if view_z >= -0.5 {
        0.35
    } else if view_z >= -0.9 {
        0.7
    } else {
        6.0
    }
}
/// Geometric-normal bound of the 7+7 bit octahedral code, in degrees.
pub(crate) const GEOMETRIC_DEGREES: f32 = 1.9;

fn view_rotation() -> Mat3 {
    Mat3::from_euler(glam::EulerRot::ZXY, 0.7, -1.1, 0.3)
}

fn surface(shading_normal: Vec3, geometric_normal: Vec3) -> Surface {
    Surface {
        shading_normal,
        geometric_normal,
        base_colour: Vec3::new(0.2, 0.5, 0.9),
        roughness: 0.6,
        metal: 0.1,
        emission: 0.25,
        ao: 0.8,
        flags: 2,
        lighting_record: 0,
    }
}

#[test]
fn gbuffer_shading_normal_round_trips_over_the_sphere_within_its_bound() {
    for rotation in [Mat3::IDENTITY, view_rotation()] {
        let mut facing = 0.0_f32;
        for normal in sphere(40_000) {
            let codes = reference::encode(&surface(normal, Vec3::Z), rotation);
            let decoded = reference::decode(&codes, rotation.transpose());
            let error = angle_degrees(decoded.shading_normal, normal);
            let view_z = (rotation * normal).z;
            assert!(
                error <= shading_bound(view_z),
                "{normal} z {view_z}: {error}"
            );
            if view_z >= 0.0 {
                facing = facing.max(error);
            }
            assert!((decoded.shading_normal.length() - 1.0).abs() < 1e-5);
        }
        assert!(facing > 0.05, "quantisation measured {facing} degrees");
    }
}

#[test]
fn gbuffer_spheremap_singularity_decodes_to_the_away_facing_normal() {
    let away = Vec3::NEG_Z;
    let exact = reference::spheremap_encode(away);
    assert!(exact.abs_diff_eq(glam::Vec2::new(1.0, 0.5), 0.0));
    assert_eq!(reference::spheremap_decode(exact), away);
    let codes = reference::encode(&surface(away, Vec3::Z), Mat3::IDENTITY);
    assert_eq!(codes.g0[..2], [1023, 512]);
    let decoded = reference::decode(&codes, Mat3::IDENTITY);
    assert!(angle_degrees(decoded.shading_normal, away) < 0.1);
    for exponent in 1..=7 {
        let offset = 10.0_f32.powi(-exponent);
        for azimuth in [0.0_f32, 0.9, 2.2, 3.9, 5.5] {
            let tilt = (offset * (2.0 - offset)).sqrt();
            let normal =
                Vec3::new(tilt * azimuth.cos(), tilt * azimuth.sin(), -1.0 + offset).normalize();
            let decoded = reference::decode(
                &reference::encode(&surface(normal, Vec3::Z), Mat3::IDENTITY),
                Mat3::IDENTITY,
            );
            assert!(decoded.shading_normal.is_finite());
            let error = angle_degrees(decoded.shading_normal, normal);
            assert!(error <= shading_bound(normal.z), "{normal} {error}");
        }
    }
}

#[test]
fn gbuffer_geometric_normal_round_trips_and_a_covered_pixel_never_stores_zero() {
    let mut worst = 0.0_f32;
    let axes = [
        Vec3::X,
        Vec3::NEG_X,
        Vec3::Y,
        Vec3::NEG_Y,
        Vec3::Z,
        Vec3::NEG_Z,
    ];
    for normal in sphere(40_000).into_iter().chain(axes) {
        for flags in 0..4 {
            let g3 = reference::g3_encode(normal, flags);
            assert_ne!(g3, 0, "{normal} flags {flags}");
            assert_eq!(u32::from(g3) & 3, flags);
            worst = worst.max(angle_degrees(reference::g3_normal(g3), normal));
        }
    }
    assert!(worst <= GEOMETRIC_DEGREES, "worst {worst} degrees");
    let down = reference::g3_encode(Vec3::NEG_Z, 0);
    assert_eq!(down, (127 << 9) | (127 << 2));
    // Near (0, 0, -1) from negative x and y the octahedral code is (0, 0); with no flags
    // that would store 0, so the encoder uses the corner (127, 127), the same normal.
    let corner = Vec3::new(-1e-3, -1e-3, -1.0).normalize();
    let unit = reference::oct_encode(corner) * 0.5 + glam::Vec2::splat(0.5);
    assert!(unit.max_element() * 127.0 < 0.5, "{unit}");
    assert_eq!(reference::g3_encode(corner, 0), (127 << 9) | (127 << 2));
    assert_eq!(reference::g3_encode(corner, 1), 1);
    assert_eq!(reference::g3_normal(down), Vec3::NEG_Z);
    assert_eq!(reference::g3_normal(0), Vec3::NEG_Z);
    assert_eq!(u32::from(reference::g3_encode(Vec3::Z, 7)) & 3, 3);
}

#[test]
fn gbuffer_scalars_and_base_colour_round_trip_within_one_code() {
    let mut values = surface(Vec3::Z, Vec3::Z);
    for step in 0..=100_u8 {
        let value = f32::from(step) / 100.0;
        values.base_colour = Vec3::new(value, 1.0 - value, value * value);
        values.roughness = value;
        values.metal = 1.0 - value;
        values.emission = value;
        values.ao = value;
        values.lighting_record = step.wrapping_mul(37);
        let codes = reference::encode(&values, Mat3::IDENTITY);
        assert_eq!(codes.g1[3], values.lighting_record);
        assert_eq!(codes.g0[2..], [0, 0]);
        let decoded = reference::decode(&codes, Mat3::IDENTITY);
        let half_code = 0.5 / 255.0 + 1e-6;
        for (decoded, expected) in [
            (decoded.roughness, values.roughness),
            (decoded.metal, values.metal),
            (decoded.ao, values.ao),
        ] {
            assert!((decoded - expected).abs() <= half_code);
        }
        // sqrt transfer: half a stored code is at most 1 / 255 of emission.
        assert!((decoded.emission - values.emission).abs() <= 2.0 * half_code);
        // sRGB codes are at most 0.009 apart in linear units, at full intensity.
        assert!(
            (decoded.base_colour - values.base_colour)
                .abs()
                .max_element()
                <= 0.0045
        );
        assert_eq!(decoded.flags, values.flags);
        assert_eq!(decoded.lighting_record, values.lighting_record);
    }
}

fn parse(source: &str) -> naga::Module {
    let module = naga::front::wgsl::parse_str(source).expect("gbuffer WGSL parses");
    naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::all(),
    )
    .validate(&module)
    .expect("gbuffer WGSL validates");
    module
}

#[test]
fn gbuffer_wgsl_validates_with_no_bindings_and_the_agreed_interface() {
    let module = parse(GBUFFER_WGSL);
    assert!(module.global_variables.iter().next().is_none());
    let function = |name: &str| {
        module
            .functions
            .iter()
            .find(|(_, function)| function.name.as_deref() == Some(name))
            .map_or_else(|| panic!("{name} is missing"), |(_, function)| function)
    };
    let arguments = |name: &str| {
        function(name)
            .arguments
            .iter()
            .map(|argument| argument.name.clone().unwrap())
            .collect::<Vec<_>>()
    };
    assert_eq!(
        arguments("scene_gbuffer_encode"),
        ["surface", "view_from_scene"]
    );
    assert_eq!(
        arguments("scene_gbuffer_decode"),
        ["g0", "g1", "g2", "g3", "scene_from_view"]
    );
    let members = |name: &str| {
        module
            .types
            .iter()
            .find(|(_, ty)| ty.name.as_deref() == Some(name))
            .map_or_else(
                || panic!("{name} is missing"),
                |(_, ty)| match &ty.inner {
                    naga::TypeInner::Struct { members, .. } => members
                        .iter()
                        .map(|member| (member.name.clone().unwrap(), member.binding.clone()))
                        .collect::<Vec<_>>(),
                    other => panic!("{name} is {other:?}"),
                },
            )
    };
    let surface: Vec<_> = members("SceneGbufferSurface")
        .into_iter()
        .map(|(name, _)| name)
        .collect();
    assert_eq!(
        surface,
        [
            "shading_normal",
            "geometric_normal",
            "base_colour",
            "roughness",
            "metal",
            "emission",
            "ao",
            "flags",
            "lighting_record"
        ]
    );
    let targets = members("SceneGbufferTargets");
    for (location, (name, binding)) in targets.iter().enumerate() {
        assert_eq!(name, &format!("g{location}"));
        assert!(matches!(
            binding,
            Some(naga::Binding::Location { location: l, .. }) if *l as usize == location
        ));
    }
    assert_eq!(targets.len(), 4);
}

#[test]
fn gbuffer_wgsl_writes_all_four_targets_from_a_fragment_entry_point() {
    let source = format!(
        "{GBUFFER_WGSL}
@fragment
fn fs_gbuffer_probe(@builtin(position) position: vec4<f32>) -> SceneGbufferTargets {{
    let n = normalize(vec3<f32>(position.xy, 1.0));
    let surface = SceneGbufferSurface(n, n, vec3<f32>(0.5), 0.5, 0.0, 0.0, 1.0, 1u, 7u);
    let targets = scene_gbuffer_encode(surface, mat3x3<f32>(vec3<f32>(1.0, 0.0, 0.0),
        vec3<f32>(0.0, 1.0, 0.0), vec3<f32>(0.0, 0.0, 1.0)));
    let decoded = scene_gbuffer_decode(targets.g0, targets.g1, targets.g2, targets.g3,
        mat3x3<f32>(vec3<f32>(1.0, 0.0, 0.0), vec3<f32>(0.0, 1.0, 0.0), vec3<f32>(0.0, 0.0, 1.0)));
    return SceneGbufferTargets(targets.g0, vec4<f32>(decoded.base_colour, 0.0), targets.g2, targets.g3);
}}
"
    );
    let module = parse(&source);
    let entry = module
        .entry_points
        .iter()
        .find(|entry| entry.name == "fs_gbuffer_probe")
        .expect("probe entry point");
    assert_eq!(entry.stage, naga::ShaderStage::Fragment);
    assert_eq!(
        GBUFFER_FORMATS,
        [
            wgpu::TextureFormat::Rgb10a2Unorm,
            wgpu::TextureFormat::Rgba8UnormSrgb,
            wgpu::TextureFormat::Rgba8Unorm,
            wgpu::TextureFormat::R16Uint,
        ]
    );
}

fn attachment(
    name: &'static str,
    format: wgpu::TextureFormat,
    load: AttachmentLoad,
) -> AttachmentDeclaration {
    AttachmentDeclaration {
        resource: ResourceRef::new(name),
        format,
        size: [8, 8],
        samples: 1,
        load,
        store: wgpu::StoreOp::Store,
        read_only: false,
    }
}

/// The slot's pass as `Gbuffer::schedule` declares it, without textures.
fn gbuffer_pass() -> PassDeclaration {
    let mut pass = PassDeclaration::new(PassSlot::OpaqueGbuffer);
    let [g0, g1, g2, g3] = GbufferTarget::ALL.map(|target| {
        attachment(
            target.resource_name(),
            target.format(),
            AttachmentLoad::ClearColor(wgpu::Color::TRANSPARENT),
        )
    });
    pass.color = Some(g0);
    pass.additional_colors = vec![g1, g2, g3];
    pass
}

fn frame(prepass: bool) -> Vec<PassDeclaration> {
    let depth = wgpu::TextureFormat::Depth32FloatStencil8;
    let mut passes = Vec::new();
    if prepass {
        let mut pass = PassDeclaration::new(PassSlot::DepthNormalPrepass);
        pass.depth = Some(attachment(
            "scene depth",
            depth,
            AttachmentLoad::ClearDepth(1.0),
        ));
        passes.push(pass);
    }
    passes.push(gbuffer_pass());
    let mut opaque = PassDeclaration::new(PassSlot::Opaque);
    opaque.color = Some(attachment(
        "scene color",
        wgpu::TextureFormat::Rgba16Float,
        AttachmentLoad::ClearColor(wgpu::Color::BLACK),
    ));
    opaque.depth = Some(attachment(
        "scene depth",
        depth,
        if prepass {
            AttachmentLoad::Load
        } else {
            AttachmentLoad::ClearDepth(1.0)
        },
    ));
    passes.push(opaque);
    passes
}

fn producer(depth: bool) -> FeatureDeclaration {
    let mut declaration = FeatureDeclaration::new(PassSlot::OpaqueGbuffer);
    let pass: &mut FeaturePass = &mut declaration.passes[0];
    pass.color_format = Some(GBUFFER_FORMATS[0]);
    pass.additional_color_formats = GBUFFER_FORMATS[1..].to_vec();
    pass.depth_format = depth.then_some(wgpu::TextureFormat::Depth32FloatStencil8);
    declaration
}

#[test]
fn gbuffer_slot_validates_before_opaque_without_changing_depth_ownership() {
    for prepass in [false, true] {
        let mut passes = frame(prepass);
        let before = passes.clone();
        super::attach_producer_depth(&mut passes, &[producer(false)]).unwrap();
        assert_eq!(format!("{passes:?}"), format!("{before:?}"));
        let slot = passes
            .iter()
            .find(|pass| pass.slot == PassSlot::OpaqueGbuffer)
            .unwrap();
        assert!(slot.depth.is_none());
        worldrunner_render_post::validate_frame(&passes, &[producer(false)], &[]).unwrap();
    }
}

#[test]
fn gbuffer_producer_depth_is_the_completed_scene_depth_read_only_or_refused() {
    let mut passes = frame(true);
    super::attach_producer_depth(&mut passes, &[producer(true)]).unwrap();
    let slot = passes
        .iter()
        .find(|pass| pass.slot == PassSlot::OpaqueGbuffer)
        .unwrap();
    let depth = slot.depth.as_ref().unwrap();
    assert_eq!(depth.resource, ResourceRef::new("scene depth"));
    assert_eq!(depth.load, AttachmentLoad::Load);
    assert!(depth.read_only);
    worldrunner_render_post::validate_frame(&passes, &[producer(true)], &[]).unwrap();
    let opaque = passes
        .iter()
        .find(|pass| pass.slot == PassSlot::Opaque)
        .unwrap();
    assert_eq!(opaque.depth.as_ref().unwrap().load, AttachmentLoad::Load);

    let mut cleared = frame(false);
    assert_eq!(
        super::attach_producer_depth(&mut cleared, &[producer(true)]),
        Err(FrameError::AttachmentContract(PassEntry::new(
            PassSlot::OpaqueGbuffer
        )))
    );
}

#[test]
fn gbuffer_targets_are_allocated_only_for_the_observation_path() {
    use crate::compositor::RendererConfiguration;
    let mut settings = RendererConfiguration::legacy([64, 32], wgpu::TextureFormat::Rgba8UnormSrgb);
    let limits = wgpu::Limits::default();
    let forward = settings.allocations().validate(&limits).unwrap();
    settings.opaque_path = worldrunner_render_plan::SceneOpaquePath::GBUFFER_OBSERVATION;
    let observed = settings.allocations().validate(&limits).unwrap();
    // Fourteen G-buffer bytes and the lighting colour target in the frame's own format.
    let colour = settings.frame.format.block_copy_size(None).unwrap();
    assert_eq!(colour, 4);
    assert_eq!(observed - forward, 64 * 32 * (14 + u64::from(colour)));
    // A refused multisampled observation allocates nothing extra; `validate` reports it.
    settings.frame.samples = 4;
    let refused = settings.allocations().validate(&limits).unwrap();
    settings.opaque_path = worldrunner_render_plan::SceneOpaquePath::Forward;
    assert_eq!(refused, settings.allocations().validate(&limits).unwrap());
}

#[test]
fn scene_producer_requires_opaque_to_load_completed_prepass_depth() {
    use crate::renderer::depth::DepthMode;

    assert!(super::producer_depth_available(DepthMode::Reuse));
    for depth in [
        DepthMode::Independent,
        DepthMode::Shared,
        DepthMode::Prepass,
    ] {
        assert!(!super::producer_depth_available(depth), "{depth:?}");
    }
}
