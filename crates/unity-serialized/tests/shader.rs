#[path = "support/decode.rs"]
mod decode;
mod support;

use decode::decode_one;
use worldrunner_unity_serialized::{
    ClassId, SerializedError,
    object::PPtr,
    shader::{CompiledBlobs, ParsedShader, Shader, ShaderProperty, StagePrograms, SubShader},
    shader_program::{
        BindChannels, Binding, ConstantBuffer, PlayerSubProgram, Program, ProgramParameters,
        SamplerParam, StructParam, SubProgram, TextureParam, UavParam, ValueParam,
    },
    shader_state::{ShaderState, StateValue},
    write::ObjectWriter,
};

const fn ptr(file_id: i32, path_id: i64) -> PPtr {
    PPtr { file_id, path_id }
}

/// Render state values in serialized order: 8 targets of 7 blend values, 8 depth values,
/// 3 stencil operation sets of 4, 3 stencil values, 3 fog values and 4 fog color values.
const STATE_VALUES: u16 = 86;
/// The value whose property is `_ZTest`: the second depth value.
const DEPTH_TEST: u16 = 58;

fn write_state(w: &mut ObjectWriter) {
    w.string("PassState").unwrap();
    for value in 1..=STATE_VALUES {
        if value == 57 {
            // Blend values end; the separate blend flag comes before the depth values.
            w.u8(1).align();
        }
        let property = if value == DEPTH_TEST {
            "_ZTest"
        } else {
            "<noninit>"
        };
        w.f32(f32::from(value)).string(property).unwrap();
    }
    w.string("_FogColor").unwrap().i32(2).i32(77);
    w.i32(1)
        .string("Queue")
        .unwrap()
        .string("Geometry")
        .unwrap();
    w.i32(150).u8(1).align();
}

/// One parameter of each kind, with distinct values; or all eight lists empty.
fn write_parameters(w: &mut ObjectWriter, full: bool) {
    if !full {
        for _ in 0..8 {
            w.i32(0);
        }
        return;
    }
    let value_param = |w: &mut ObjectWriter, name: i32| {
        w.i32(name).i32(name + 1).i32(0).u8(0).u8(4).align();
    };
    w.i32(1);
    value_param(w, 1);
    w.i32(1);
    value_param(w, 3);
    w.i32(1).i32(5).i32(6).i32(7).u8(1).u8(2).align();
    w.i32(1).i32(8).i32(9).i32(10);
    // One constant buffer with no matrices, one vector and one struct.
    w.i32(1).i32(11).i32(0).i32(1);
    value_param(w, 12);
    // The struct has one vector and no matrices.
    w.i32(1).i32(14).i32(15).i32(16).i32(17).i32(1);
    value_param(w, 30);
    w.i32(0);
    w.i32(64).u8(1).align();
    w.i32(1).i32(11).i32(0).i32(1);
    w.i32(1).i32(18).i32(19).i32(20);
    w.i32(1).u32(21).u32(22);
}

/// A program whose keyword state mask holds `marker`, so each stage is told apart; with or
/// without variants and parameters.
fn write_program(w: &mut ObjectWriter, marker: u16, full: bool) {
    if !full {
        w.i32(0).i32(0).i32(0);
        write_parameters(w, false);
        w.i32(1).u16(marker).align();
        return;
    }
    w.i32(1)
        .u32(3)
        .i32(2)
        .bytes(&[0, 1, 2, 3])
        .align()
        .i32(0x1f);
    w.i32(3).u16(1).u16(2).u16(3).align().u8(1).u8(9).align();
    write_parameters(w, true);
    w.i64(-2);
    // One platform with two player variants.
    w.i32(1).i32(2);
    w.u32(4).i32(1).u16(0).align().i64(16).u8(9).align();
    w.u32(5).i32(2).u16(0).u16(1).align().i64(0).u8(9).align();
    w.i32(1).i32(2).u32(6).u32(7);
    write_parameters(w, true);
    w.i32(1).u16(marker).align();
}

fn write_pass(w: &mut ObjectWriter) {
    w.i32(2)
        .string("_MainTex")
        .unwrap()
        .i32(0)
        .string("_Color")
        .unwrap()
        .i32(1);
    w.i32(0);
    write_state(w);
    w.u32(3);
    write_program(w, 1, true);
    for marker in 2..=6 {
        write_program(w, marker, false);
    }
    w.u8(1)
        .u8(0)
        .align()
        .string("Hidden/Base/CASTER")
        .unwrap()
        .string("ForwardLit")
        .unwrap()
        .string("_GrabTexture")
        .unwrap();
    w.i32(1)
        .string("LightMode")
        .unwrap()
        .string("UniversalForward")
        .unwrap();
}

fn write_parsed(w: &mut ObjectWriter) {
    w.i32(1)
        .string("_MainTex")
        .unwrap()
        .string("Albedo")
        .unwrap();
    w.i32(1).string("NoScaleOffset").unwrap();
    w.i32(4)
        .u32(2)
        .floats(&[1.0, 2.0, 3.0, 4.0])
        .string("white")
        .unwrap()
        .i32(2);
    w.i32(1).i32(1);
    write_pass(w);
    w.i32(1)
        .string("RenderPipeline")
        .unwrap()
        .string("UniversalPipeline")
        .unwrap()
        .i32(300);
    w.i32(2).string("_FOG").unwrap().string("_ALPHA").unwrap();
    w.byte_array(&[1, 0]).unwrap();
    w.string("Hidden/Test")
        .unwrap()
        .string("Editor")
        .unwrap()
        .string("Diffuse")
        .unwrap();
    w.i32(1)
        .string("BaseMap")
        .unwrap()
        .string("Hidden/Base")
        .unwrap();
    w.i32(1)
        .string("ShaderGUI")
        .unwrap()
        .string("Pipeline")
        .unwrap();
    w.u8(1).align();
}

fn shader_bytes(big_endian: bool) -> Vec<u8> {
    let mut w = ObjectWriter::new(big_endian);
    w.string("Test").unwrap();
    write_parsed(&mut w);
    // Two platforms; the second has no compiled chunks.
    w.i32(2).u32(21).u32(5);
    for pair in [[0, 40], [40, 20], [100, 50]] {
        w.i32(2).i32(2).u32(pair[0]).u32(pair[1]).i32(0);
    }
    w.byte_array(&[9; 7]).unwrap().i32(2).u32(2).u32(1);
    w.pptrs(&[ptr(1, 9)]).unwrap();
    w.i32(1).string("_Noise").unwrap().pptr(ptr(0, 4));
    w.u8(1).align();
    let guid: Vec<u8> = (0..16).collect();
    w.bytes(&guid).i64(-5);
    w.finish()
}

fn decoded(big_endian: bool) -> Shader {
    decode_one(ClassId::SHADER, shader_bytes(big_endian), big_endian).unwrap()
}

fn pair(a: &str, b: &str) -> (String, String) {
    (a.to_owned(), b.to_owned())
}

/// The parsed shader `write_parsed` writes, with its sub-shader's passes left out.
fn expected_declarations() -> ParsedShader {
    let property = ShaderProperty {
        name: "_MainTex".into(),
        description: "Albedo".into(),
        attributes: vec!["NoScaleOffset".into()],
        kind: 4,
        flags: 2,
        default_value: [1.0, 2.0, 3.0, 4.0],
        default_texture: "white".into(),
        texture_dimension: 2,
    };
    ParsedShader {
        properties: vec![property],
        sub_shaders: vec![SubShader {
            passes: Vec::new(),
            tags: vec![pair("RenderPipeline", "UniversalPipeline")],
            lod: 300,
        }],
        keyword_names: vec!["_FOG".into(), "_ALPHA".into()],
        keyword_flags: vec![1, 0],
        name: "Hidden/Test".into(),
        custom_editor: "Editor".into(),
        fallback: "Diffuse".into(),
        dependencies: vec![pair("BaseMap", "Hidden/Base")],
        pipeline_editors: vec![pair("ShaderGUI", "Pipeline")],
        disable_no_sub_shaders_message: true,
    }
}

#[test]
fn decodes_shader_declarations_and_compiled_tables() {
    for big_endian in [false, true] {
        let mut shader = decoded(big_endian);
        shader.parsed.sub_shaders[0].passes.clear();
        assert_eq!(shader.name, "Test");
        assert_eq!(shader.parsed, expected_declarations());
        let blob_start = shader.compiled.blob.start;
        let compiled = CompiledBlobs {
            platforms: vec![21, 5],
            offsets: vec![vec![0, 40], Vec::new()],
            compressed_lengths: vec![vec![40, 20], Vec::new()],
            decompressed_lengths: vec![vec![100, 50], Vec::new()],
            blob: blob_start..blob_start + 7,
            stage_counts: vec![2, 1],
        };
        assert_eq!(shader.compiled, compiled);
        assert_eq!(shader.dependencies, [ptr(1, 9)]);
        assert_eq!(
            shader.non_modifiable_textures,
            [(String::from("_Noise"), ptr(0, 4))]
        );
        assert!(shader.is_baked);
        let guid: Vec<u8> = (0..16).collect();
        assert_eq!(
            (shader.asset_guid.as_slice(), shader.asset_local_identifier),
            (guid.as_slice(), -5)
        );
    }
}

/// The state's values in serialized order.
fn state_values(state: &ShaderState) -> Vec<&StateValue> {
    let mut values = Vec::new();
    for blend in &state.blend {
        values.extend([
            &blend.src,
            &blend.dst,
            &blend.src_alpha,
            &blend.dst_alpha,
            &blend.op,
            &blend.op_alpha,
            &blend.color_mask,
        ]);
    }
    let depth = &state.depth;
    values.extend([
        &depth.clip,
        &depth.test,
        &depth.write,
        &depth.cull,
        &depth.conservative,
        &depth.offset_factor,
        &depth.offset_units,
        &depth.alpha_to_mask,
    ]);
    let stencil = &state.stencil;
    for ops in [&stencil.both, &stencil.front, &stencil.back] {
        values.extend([&ops.pass, &ops.fail, &ops.z_fail, &ops.compare]);
    }
    values.extend([&stencil.read_mask, &stencil.write_mask, &stencil.reference]);
    values.extend([&state.fog.start, &state.fog.end, &state.fog.density]);
    values.extend(&state.fog.color);
    values
}

#[test]
fn decodes_pass_render_state_in_serialized_order() {
    for big_endian in [false, true] {
        let shader = decoded(big_endian);
        let pass = &shader.parsed.sub_shaders[0].passes[0];
        assert_eq!(
            pass.name_indices,
            [("_MainTex".to_owned(), 0), ("_Color".to_owned(), 1)]
        );
        assert_eq!(
            (pass.name.as_str(), pass.program_mask, pass.kind),
            ("ForwardLit", 3, 0)
        );
        assert_eq!(
            (pass.use_name.as_str(), pass.texture_name.as_str()),
            ("Hidden/Base/CASTER", "_GrabTexture")
        );
        assert_eq!(
            pass.tags,
            [("LightMode".to_owned(), "UniversalForward".to_owned())]
        );
        assert!(pass.has_instancing_variant && !pass.has_procedural_instancing_variant);
        let state = &pass.state;
        let values: Vec<u32> = state_values(state)
            .iter()
            .map(|v| v.value.to_bits())
            .collect();
        let expected: Vec<u32> = (1..=STATE_VALUES).map(|v| f32::from(v).to_bits()).collect();
        assert_eq!(values, expected);
        assert_eq!(state.name, "PassState");
        assert_eq!(state.depth.test.property.as_deref(), Some("_ZTest"));
        let with_property = state_values(state)
            .iter()
            .filter(|v| v.property.is_some())
            .count();
        assert_eq!(with_property, 1, "only the depth test reads a property");
        assert!(state.separate_blend && state.lighting);
        assert_eq!(
            (
                state.fog.color_property.as_deref(),
                state.fog.mode,
                state.gpu_program_id,
                state.lod
            ),
            (Some("_FogColor"), 2, 77, 150)
        );
        assert_eq!(state.tags, [("Queue".to_owned(), "Geometry".to_owned())]);
    }
}

fn value(name: i32) -> ValueParam {
    ValueParam {
        name_index: name,
        index: name + 1,
        array_size: 0,
        kind: 0,
        dimension: 4,
    }
}

/// The parameters `write_parameters` writes in full.
fn expected_parameters() -> ProgramParameters {
    ProgramParameters {
        vectors: vec![value(1)],
        matrices: vec![value(3)],
        textures: vec![TextureParam {
            name_index: 5,
            index: 6,
            sampler_index: 7,
            multisampled: true,
            dimension: 2,
        }],
        buffers: vec![Binding {
            name_index: 8,
            index: 9,
            array_size: 10,
        }],
        constant_buffers: vec![ConstantBuffer {
            name_index: 11,
            matrices: Vec::new(),
            vectors: vec![value(12)],
            structs: vec![StructParam {
                name_index: 14,
                index: 15,
                array_size: 16,
                struct_size: 17,
                vectors: vec![value(30)],
                matrices: Vec::new(),
            }],
            size: 64,
            partial: true,
        }],
        constant_buffer_bindings: vec![Binding {
            name_index: 11,
            index: 0,
            array_size: 1,
        }],
        uavs: vec![UavParam {
            name_index: 18,
            index: 19,
            original_index: 20,
        }],
        samplers: vec![SamplerParam {
            sampler: 21,
            bind_point: 22,
        }],
    }
}

/// The program `write_program` writes in full.
fn expected_program() -> Program {
    let variant = |blob_index, keyword_indices: &[u16], requirements| PlayerSubProgram {
        blob_index,
        keyword_indices: keyword_indices.to_vec(),
        requirements,
        gpu_program_type: 9,
    };
    Program {
        sub_programs: vec![SubProgram {
            blob_index: 3,
            channels: BindChannels {
                channels: vec![[0, 1], [2, 3]],
                source_map: 0x1f,
            },
            keyword_indices: vec![1, 2, 3],
            hardware_tier: 1,
            gpu_program_type: 9,
            parameters: expected_parameters(),
            requirements: -2,
        }],
        player_sub_programs: vec![vec![variant(4, &[0], 16), variant(5, &[0, 1], 0)]],
        parameter_blob_indices: vec![vec![6, 7]],
        common_parameters: expected_parameters(),
        keyword_state_mask: vec![1],
    }
}

#[test]
fn decodes_programs_variants_and_parameters() {
    for big_endian in [false, true] {
        let shader = decoded(big_endian);
        let programs = &shader.parsed.sub_shaders[0].passes[0].programs;
        let marked = |marker| Program {
            keyword_state_mask: vec![marker],
            ..Program::default()
        };
        let expected = StagePrograms {
            vertex: expected_program(),
            fragment: marked(2),
            geometry: marked(3),
            hull: marked(4),
            domain: marked(5),
            ray_tracing: marked(6),
        };
        assert_eq!(programs, &expected);
    }
}

#[test]
fn every_truncated_shader_fails() {
    let bytes = shader_bytes(false);
    for end in 0..bytes.len() {
        let error =
            decode_one::<Shader>(ClassId::SHADER, bytes[..end].to_vec(), false).unwrap_err();
        assert!(
            matches!(
                error,
                SerializedError::Truncated { .. } | SerializedError::InvalidCount { .. }
            ),
            "prefix {end}: {error}"
        );
    }
}

#[test]
fn rejects_variant_counts_the_shader_cannot_hold() {
    let mut bytes = shader_bytes(false);
    // The trailing 16-byte GUID and 8-byte identifier follow the baked flag, the
    // non-modifiable texture and the dependency; the stage counts precede them.
    let stage_count = bytes.len() - 24 - 4 - (4 + 4 + 8 + 12) - (4 + 12) - 12;
    assert_eq!(bytes[stage_count..stage_count + 4], 2_i32.to_le_bytes());
    bytes[stage_count..stage_count + 4].copy_from_slice(&i32::MAX.to_le_bytes());
    let error = decode_one::<Shader>(ClassId::SHADER, bytes, false).unwrap_err();
    assert!(
        matches!(
            error,
            SerializedError::InvalidCount {
                what: "stage count",
                ..
            }
        ),
        "{error}"
    );
}
