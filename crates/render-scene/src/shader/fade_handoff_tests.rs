use super::{ENVIRONMENT_WGSL, evaluate_source};
use crate::device::GpuContext;
use crate::geometry::{millimeter_to_meter, pack_object};
use crate::pipeline::MaterialUniform;
use worldrunner_render_plan::{
    MaterialHandle, MeshHandle, SceneAlphaMode, SceneFadeBand, SceneFadeCoverage,
    SceneFadeDirection, SceneFadeQuantization, SceneImpostor, SceneImpostorLayout, SceneMaterial,
    SceneMaterialParts, SceneObject, SceneRotation, SceneTransform, TextureHandle,
};

fn material() -> SceneMaterial {
    SceneMaterial::new(SceneMaterialParts {
        albedo: TextureHandle::from_raw(0),
        normal: None,
        roughness_metallic: None,
        ambient_occlusion: None,
        self_illumination: None,
        roughness_micro: 1_000_000,
        metalness_micro: 0,
        alpha_mode: SceneAlphaMode::Opaque,
        double_sided: false,
    })
    .unwrap()
}

fn word(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap())
}

#[test]
fn mixed_source_fade_bands_partition_hashed_and_byte_threshold_masks() {
    let context = pollster::block_on(GpuContext::request()).unwrap();
    let band = SceneFadeBand::new(16_777_219, 16_781_219).unwrap();
    let (packed, incoming) = mixed_sources(band);
    for threshold in [false, true] {
        assert_masks(&context, &packed, &incoming, threshold, 16_777_220, false);
    }
}

#[test]
fn mixed_source_step_bands_partition_every_hashed_and_r8_threshold() {
    let context = pollster::block_on(GpuContext::request()).unwrap();
    for endpoint in [0, 4000] {
        let (packed, incoming) = mixed_sources(SceneFadeBand::new(endpoint, endpoint).unwrap());
        for distance in [
            i64::from(endpoint) - 1,
            i64::from(endpoint),
            i64::from(endpoint) + 1,
        ] {
            if distance < 0 {
                continue;
            }
            for threshold in [false, true] {
                assert_masks(
                    &context,
                    &packed,
                    &incoming,
                    threshold,
                    distance,
                    distance <= i64::from(endpoint),
                );
            }
        }
    }
}

fn mixed_sources(band: SceneFadeBand) -> (Vec<u8>, SceneMaterial) {
    let object = SceneObject::new(
        SceneTransform {
            position_millimeters: [0; 3],
            rotation: SceneRotation {
                x: 0,
                y: 0,
                z: 0,
                w: 1_000_000,
            },
            scale_millionths: 1_000_000,
        },
        MeshHandle::from_raw(0),
        MaterialHandle::from_raw(0),
    )
    .with_fade(band.with_terminal_fade());
    let packed = pack_object(object);
    let impostor =
        SceneImpostor::new(SceneImpostorLayout::AzimuthElevation, [1, 1], 0, false).unwrap();
    let incoming = material().with_impostor(impostor.with_fade(band));
    let bytes = MaterialUniform::from_material(&incoming).to_bytes();
    if band.millimeters()[0] == band.millimeters()[1] {
        assert_eq!(word(&packed, 12), (-2.0_f32).to_bits());
        assert_eq!(word(&bytes, 280), (-1.0_f32).to_bits());
        assert_eq!(word(&packed, 28), word(&bytes, 284));
    } else {
        assert_eq!(&packed[12..16], &bytes[280..284]);
        assert_eq!(word(&packed, 28) ^ (1 << 31), word(&bytes, 284));
    }
    let terminal = material().with_impostor(impostor.with_fade(band.with_terminal_fade()));
    let terminal = MaterialUniform::from_material(&terminal).to_bytes();
    assert_eq!(&packed[12..16], &terminal[280..284]);
    assert_eq!(&packed[28..32], &terminal[284..288]);
    let default = MaterialUniform::from_material(&material().with_impostor(impostor)).to_bytes();
    assert_eq!(&default[280..288], &[0; 8]);
    (packed, incoming)
}

fn assert_masks(
    context: &GpuContext,
    packed: &[u8],
    incoming: &SceneMaterial,
    threshold: bool,
    distance: i64,
    covered: bool,
) {
    let outgoing = material().with_fade_direction(SceneFadeDirection::Keep);
    let incoming = incoming.with_fade_direction(SceneFadeDirection::Complement);
    let policy = |value: SceneMaterial| {
        if threshold {
            value
                .with_fade_quantization(SceneFadeQuantization::Byte)
                .with_fade_coverage(SceneFadeCoverage::Threshold {
                    texture: TextureHandle::from_raw(1),
                    jitter_pixels: [0, 0],
                })
        } else {
            value
        }
    };
    let a = MaterialUniform::from_material(&policy(outgoing)).to_bytes();
    let b = MaterialUniform::from_material(&policy(incoming)).to_bytes();
    let entry = ENTRY
        .replace("KEEP_FLAGS", &format!("{}u", word(&a, 432)))
        .replace("COMPLEMENT_FLAGS", &format!("{}u", word(&b, 432)))
        .replace("BYTE_MODE", if threshold { "true" } else { "false" })
        .replace("SIDE", if threshold { "16u" } else { "256u" });
    let source = ENVIRONMENT_WGSL.replace(
        "@group(1) @binding(0) var<uniform> material: Material;",
        "var<private> material: Material;",
    );
    // Upload markers and endpoints through the production buffer path.
    let mut table = vec![0.0; 8];
    table.extend([
        f32::from_bits(word(packed, 12)),
        f32::from_bits(word(packed, 28)),
        f32::from_bits(word(&b, 280)),
        f32::from_bits(word(&b, 284)),
        millimeter_to_meter(distance),
        f32::from_bits(16),
    ]);
    table.extend((0_u8..=255).map(|value| f32::from(value) / 255.0));
    assert_eq!(
        evaluate_source(context, &source, &entry, None, Some(&table)),
        vec![0.0, 1.0, f32::from(covered), f32::from(!covered)],
        "distance={distance} threshold={threshold} covered={covered}"
    );
}

const ENTRY: &str = r"
@group(0) @binding(1) var<storage, read_write> result: array<vec4<f32>>;
@compute @workgroup_size(1) fn evaluate_math() {
    let distance = contact_heights.heights[4];
    let outgoing_band = vec2(contact_heights.heights[0], contact_heights.heights[1]);
    var outgoing = scene_distance_coverage(distance, outgoing_band);
    let incoming_band = vec2(contact_heights.heights[2], contact_heights.heights[3]);
    var incoming = scene_distance_coverage(distance, scene_complement_band(incoming_band));
    if BYTE_MODE {
        outgoing = scene_partition_byte_coverage(outgoing);
        incoming = scene_partition_byte_coverage(incoming);
    }
    material.fade_policy.y = 257u;
    var failures = 0u;
    if any(scene_complement_band(outgoing_band) != incoming_band) { failures++; }
    for (var y = 0u; y < SIDE; y++) {
        for (var x = 0u; x < SIDE; x++) {
            let pixel = vec2<f32>(f32(x), f32(y));
            material.fade_policy.x = KEEP_FLAGS;
            let a = scene_fade_covered(pixel, outgoing);
            material.fade_policy.x = COMPLEMENT_FLAGS;
            let b = scene_fade_covered(pixel, incoming);
            if a == b { failures++; }
        }
    }
    let probe = select(vec2<f32>(216.0,123.0), vec2<f32>(14.0,15.0), BYTE_MODE);
    material.fade_policy.x = KEEP_FLAGS;
    let a = scene_fade_covered(probe, outgoing);
    material.fade_policy.x = COMPLEMENT_FLAGS;
    let b = scene_fade_covered(probe, incoming);
    result[0] = vec4(f32(failures), select(0.0,1.0,bitcast<u32>(outgoing) == bitcast<u32>(incoming)), select(0.0,1.0,a), select(0.0,1.0,b));
}
";

#[test]
fn positive_width_fade_coverage_preserves_float_bits_including_rounded_endpoints() {
    let context = pollster::block_on(GpuContext::request()).unwrap();
    for (start, end) in [
        (0, 1),
        (2000, 6000),
        (7000, 7500),
        (16_777_219, 16_777_220),
        (u32::MAX - 1, u32::MAX),
    ] {
        let band = SceneFadeBand::new(start, end).unwrap();
        let meters = [start, end].map(|v| millimeter_to_meter(i64::from(v)));
        for terminal in [false, true] {
            let band = if terminal {
                band.with_terminal_fade()
            } else {
                band
            };
            let encoded = crate::geometry::fade_band_meters(band);
            assert_eq!(encoded[0].to_bits(), meters[0].to_bits());
            assert_eq!(
                encoded[1].to_bits(),
                if terminal { -meters[1] } else { meters[1] }.to_bits()
            );
            let entry = POSITIVE_WIDTH_ENTRY
                .replace("START_BITS", &format!("{}u", encoded[0].to_bits()))
                .replace("END_BITS", &format!("{}u", encoded[1].to_bits()));
            assert_eq!(
                evaluate_source(&context, ENVIRONMENT_WGSL, &entry, None, None),
                vec![0.0; 4]
            );
        }
    }
}

const POSITIVE_WIDTH_ENTRY: &str = r"
fn legacy_distance_coverage(distance: f32, band: vec2<f32>) -> f32 {
    if band.y < 0.0 {
        if distance <= band.x { return 1.0; }
        return max(0.0, (-band.y-distance)/max(1.0,-band.y-band.x));
    }
    if distance < band.x { return 0.0; }
    if distance > band.y { return 1.0; }
    return 1.0 - (band.y-distance)/max(1.0,band.y-band.x);
}
@group(0) @binding(1) var<storage, read_write> result: array<vec4<f32>>;
@compute @workgroup_size(1) fn evaluate_math() {
    let band = vec2<f32>(bitcast<f32>(START_BITS), bitcast<f32>(END_BITS));
    let end = abs(band.y);
    let distances = array<f32, 7>(band.x-0.001, band.x, band.x+0.001,
        (band.x+end)*0.5, end-0.001, end, end+0.001);
    var failures = 0u;
    for (var i = 0u; i < 7u; i++) {
        let actual = scene_distance_coverage(distances[i], band);
        let expected = legacy_distance_coverage(distances[i], band);
        if bitcast<u32>(actual) != bitcast<u32>(expected) { failures++; }
    }
    result[0] = vec4<f32>(f32(failures), 0.0, 0.0, 0.0);
}
";
