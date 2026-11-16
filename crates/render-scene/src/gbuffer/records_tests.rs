use super::*;
use crate::pipeline::MaterialUniform;
use worldrunner_render_plan::{
    SceneAlphaMode, SceneMaterial, SceneMaterialParts, SceneTranslucency, SceneTranslucencyParts,
    TextureHandle,
};

fn material() -> SceneMaterial {
    SceneMaterial::new(SceneMaterialParts {
        albedo: TextureHandle::from_raw(0),
        normal: None,
        roughness_metallic: None,
        ambient_occlusion: None,
        self_illumination: None,
        roughness_micro: 500_000,
        metalness_micro: 0,
        alpha_mode: SceneAlphaMode::Opaque,
        double_sided: true,
    })
    .unwrap()
}

fn scattering(distortion_micro: u32) -> SceneTranslucency {
    SceneTranslucency::new(SceneTranslucencyParts {
        distortion_micro,
        falloff_micro: 4_000_000,
        sun_scale_micro: 1_000_000,
        ambient_scale_micro: 0,
        scale_micro: 700_000,
        base_kelvin: 6500,
        warming_kelvin: 300,
        luminance_micro: 200_000,
        saturation_micro: 0,
    })
    .unwrap()
}

fn uniform(material: &SceneMaterial) -> [u8; MATERIAL_UNIFORM_BYTES] {
    MaterialUniform::from_material(material).to_bytes()
}

fn distinct(index: u32) -> LightingRecord {
    let mut values = [0.0; 16];
    values[0] = f32::from(u16::try_from(index + 1).unwrap());
    values[9] = 1.0;
    LightingRecord::from_floats(values)
}

#[test]
fn record_zero_is_the_neutral_standard_model() {
    let mut table = LightingRecordTable::default();
    assert_eq!(table.records(), [LightingRecord::NEUTRAL]);
    assert_eq!(table.distinct(), 0);
    let mut bytes = uniform(&material());
    let before = bytes;
    assert_eq!(
        LightingRecord::from_uniform(&bytes),
        LightingRecord::NEUTRAL
    );
    assert_eq!(table.assign(&mut bytes), 0);
    // The index lane of a neutral material keeps its zero bytes, so its uniform is unchanged.
    assert_eq!(bytes, before);
    assert_eq!(table.to_bytes(), [0; LIGHTING_RECORD_BYTES]);
}

#[test]
fn equal_records_share_an_index_and_the_uniform_carries_it() {
    let mut table = LightingRecordTable::default();
    let leaf = material().with_translucency_mask(scattering(300_000));
    let other = material().with_translucency_mask(scattering(600_000));
    let mut first = uniform(&leaf);
    let mut second = uniform(&leaf);
    let mut third = uniform(&other);
    assert_eq!(table.assign(&mut first), 1);
    assert_eq!(table.assign(&mut second), 1);
    assert_eq!(table.assign(&mut third), 2);
    assert_eq!(table.assign(&mut uniform(&material())), 0);
    assert_eq!(table.distinct(), 2);
    assert_eq!(&first[152..156], &1.0_f32.to_le_bytes());
    assert_eq!(&third[152..156], &2.0_f32.to_le_bytes());
    // The index lane is outside the record, so assigning does not change the record.
    assert_eq!(
        LightingRecord::from_uniform(&first),
        LightingRecord::from_uniform(&uniform(&leaf))
    );
    let before = uniform(&leaf);
    let changed: Vec<_> = (0..MATERIAL_UNIFORM_BYTES)
        .filter(|&byte| first[byte] != before[byte])
        .collect();
    assert_eq!(changed, [154, 155], "only the index lane's float changes");
}

#[test]
fn a_record_copies_the_scattering_and_transmission_lanes_the_shader_reads() {
    let leaf = material()
        .with_translucency_mask(scattering(300_000))
        .with_translucency(250_000)
        .unwrap();
    let bytes = uniform(&leaf);
    let float = |offset: usize| f32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap());
    let record = LightingRecord::from_uniform(&bytes).to_bytes();
    let stored =
        |lane: usize| f32::from_le_bytes(record[lane * 4..lane * 4 + 4].try_into().unwrap());
    for lane in 0..4 {
        assert_eq!(stored(lane).to_bits(), float(112 + lane * 4).to_bits());
        assert_eq!(stored(4 + lane).to_bits(), float(128 + lane * 4).to_bits());
    }
    assert_eq!(stored(8).to_bits(), float(144).to_bits());
    assert_eq!(stored(9).to_bits(), 1.0_f32.to_bits(), "scattering enabled");
    assert_eq!(stored(10).to_bits(), 0);
    assert_eq!(stored(11).to_bits(), 0);
    assert!((stored(12) - 0.25).abs() < 1e-7, "transmission amount");
    assert_eq!(stored(12).to_bits(), float(68).to_bits());
    assert_eq!(&record[52..], &[0; 12]);
}

#[test]
fn the_table_admits_255_records_and_refuses_256_by_name() {
    let mut table = LightingRecordTable::default();
    for index in 0..LIGHTING_RECORD_LIMIT {
        assert_eq!(table.index(distinct(index)), index + 1);
    }
    assert_eq!(table.distinct(), 255);
    assert_eq!(table.admit(), Ok(()));
    assert_eq!(
        table.index(distinct(3)),
        4,
        "an equal record is not added again"
    );
    assert_eq!(table.index(distinct(255)), 256);
    assert_eq!(table.distinct(), 256);
    let refused = table.admit();
    assert_eq!(
        refused,
        Err(OpaqueAdmissionError::RecordOverflow {
            records: 256,
            limit: 255,
        })
    );
    assert_eq!(
        refused.unwrap_err().to_string(),
        "opaque path needs 256 material records, limit is 255"
    );
    // Nothing is dropped or wrapped: every record keeps its own index.
    assert_eq!(table.records().len(), 257);
    assert_eq!(table.to_bytes().len(), 257 * LIGHTING_RECORD_BYTES);
}
