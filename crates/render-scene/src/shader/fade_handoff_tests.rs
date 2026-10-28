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
    assert_eq!(&packed[12..16], &bytes[280..284]);
    assert_eq!(word(&packed, 28) ^ (1 << 31), word(&bytes, 284));
    let terminal = material().with_impostor(impostor.with_fade(band.with_terminal_fade()));
    assert_eq!(
        &packed[28..32],
        &MaterialUniform::from_material(&terminal).to_bytes()[284..288]
    );
    let default = MaterialUniform::from_material(&material().with_impostor(impostor)).to_bytes();
    assert_eq!(&default[280..288], &[0; 8]);
    for threshold in [false, true] {
        assert_masks(&context, &packed, &incoming, threshold);
    }
}

fn assert_masks(context: &GpuContext, packed: &[u8], incoming: &SceneMaterial, threshold: bool) {
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
        .replace("OBJECT_START", &format!("{}u", word(packed, 12)))
        .replace("OBJECT_END", &format!("{}u", word(packed, 28)))
        .replace("MATERIAL_START", &format!("{}u", word(&b, 280)))
        .replace("MATERIAL_END", &format!("{}u", word(&b, 284)))
        .replace(
            "DISTANCE_BITS",
            &format!("{}u", millimeter_to_meter(16_777_220).to_bits()),
        )
        .replace("KEEP_FLAGS", &format!("{}u", word(&a, 432)))
        .replace("COMPLEMENT_FLAGS", &format!("{}u", word(&b, 432)))
        .replace("BYTE_MODE", if threshold { "true" } else { "false" })
        .replace("SIDE", if threshold { "16u" } else { "256u" });
    let source = ENVIRONMENT_WGSL.replace(
        "@group(1) @binding(0) var<uniform> material: Material;",
        "var<private> material: Material;",
    );
    let mut table = vec![f32::from_bits(16)];
    table.extend((0_u8..=255).map(|value| f32::from(value) / 255.0));
    assert_eq!(
        evaluate_source(context, &source, &entry, None, Some(&table)),
        vec![0.0, 1.0, 0.0, 1.0]
    );
}

const ENTRY: &str = r"
@group(0) @binding(1) var<storage, read_write> result: array<vec4<f32>>;
@compute @workgroup_size(1) fn evaluate_math() {
    let distance = bitcast<f32>(DISTANCE_BITS);
    var outgoing = scene_distance_coverage(distance, vec2(bitcast<f32>(OBJECT_START), bitcast<f32>(OBJECT_END)));
    var incoming = scene_distance_coverage(distance, vec2(bitcast<f32>(MATERIAL_START), -bitcast<f32>(MATERIAL_END)));
    if BYTE_MODE {
        outgoing = scene_partition_byte_coverage(outgoing);
        incoming = scene_partition_byte_coverage(incoming);
    }
    material.fade_policy.y = 257u;
    var failures = 0u;
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
