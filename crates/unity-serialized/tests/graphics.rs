#[path = "support/decode.rs"]
mod decode;
mod support;

use decode::decode_one;
use worldrunner_unity_serialized::{
    ClassId, SerializedError,
    graphics::{
        Material, MeshFilter, MeshRenderer, Mipmaps, Renderer, RendererSettings, SavedProperties,
        StreamingInfo, TexEnv, Texture2D, TextureSettings,
    },
    object::{Layout, PPtr},
    write::ObjectWriter,
};

const fn ptr(file_id: i32, path_id: i64) -> PPtr {
    PPtr { file_id, path_id }
}

fn mesh_filter_bytes(big_endian: bool) -> Vec<u8> {
    let mut w = ObjectWriter::new(big_endian);
    w.pptr(ptr(0, 8)).pptr(ptr(1, 10_210));
    w.finish()
}

fn mesh_renderer_bytes(big_endian: bool) -> Vec<u8> {
    let mut w = ObjectWriter::new(big_endian);
    w.pptr(ptr(0, 8)).u8(1);
    // Twelve byte-sized settings, 1 through 12, then padding.
    w.bytes(&[1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12]).align();
    w.i16(-1).align().f32(0.25).u32(u32::MAX).i32(-3);
    w.u16(65_535)
        .u16(7)
        .floats(&[1.0, 1.0, 0.0, 0.0])
        .floats(&[2.0, 2.0, 0.5, 0.5]);
    w.pptrs(&[ptr(0, 3), ptr(2, -4)]).unwrap();
    w.u16(1)
        .u16(2)
        .pptr(ptr(0, 11))
        .pptr(ptr(0, 12))
        .pptr(ptr(0, 13))
        .align();
    w.i32(9).i16(-2).i16(5).align().i32(1).align();
    w.pptr(ptr(0, 14)).pptr(ptr(0, 0));
    w.finish()
}

fn material_bytes(big_endian: bool) -> Vec<u8> {
    let mut w = ObjectWriter::new(big_endian);
    w.string("Water").unwrap().pptr(ptr(3, 6_704));
    w.i32(1)
        .string("_NORMALMAP")
        .unwrap()
        .i32(1)
        .string("_EMISSION")
        .unwrap();
    w.u32(5).u8(1).u8(0).align().i32(3_000);
    w.i32(1)
        .string("RenderType")
        .unwrap()
        .string("Opaque")
        .unwrap();
    w.i32(1).string("SHADOWCASTER").unwrap();
    w.i32(1)
        .string("_MainTex")
        .unwrap()
        .pptr(ptr(0, 5))
        .floats(&[2.0, 3.0, 0.5, 0.25]);
    w.i32(1).string("_Mode").unwrap().i32(4);
    w.i32(1).string("_Glossiness").unwrap().f32(0.75);
    w.i32(1)
        .string("_Color")
        .unwrap()
        .floats(&[1.0, 0.5, 0.25, 1.0]);
    w.i32(1)
        .string("Terrain")
        .unwrap()
        .string("Albedo")
        .unwrap();
    w.finish()
}

/// A 5-byte inline image, so the image data is followed by padding.
fn texture_bytes(big_endian: bool, inline: bool) -> Vec<u8> {
    let mut w = ObjectWriter::new(big_endian);
    w.string("Hex").unwrap().u8(0).align();
    w.i32(256).i32(128).u32(87_408).i32(1).i32(12).i32(9);
    w.u8(1)
        .u8(0)
        .u8(1)
        .align()
        .string("Terrain")
        .unwrap()
        .u8(1)
        .align()
        .i32(-2);
    w.i32(1).i32(2).i32(1).i32(4).f32(-0.5).i32(0).i32(1).i32(2);
    w.i32(6).i32(1).byte_array(&[]).unwrap();
    if inline {
        w.byte_array(&[1, 2, 3, 4, 5])
            .unwrap()
            .u64(0)
            .u32(0)
            .string("")
            .unwrap();
    } else {
        w.byte_array(&[])
            .unwrap()
            .u64(4_096)
            .u32(87_408)
            .string("sharedassets2.assets.resS")
            .unwrap();
    }
    w.finish()
}

#[test]
fn decodes_mesh_filters_in_both_byte_orders() {
    for big_endian in [false, true] {
        let filter: MeshFilter = decode_one(
            ClassId::MESH_FILTER,
            mesh_filter_bytes(big_endian),
            big_endian,
        )
        .unwrap();
        assert_eq!(
            filter,
            MeshFilter {
                game_object: ptr(0, 8),
                mesh: ptr(1, 10_210)
            }
        );
    }
    // Hand-written bytes, independent of ObjectWriter.
    let mut bytes = vec![0, 0, 0, 0, 8, 0, 0, 0, 0, 0, 0, 0];
    bytes.extend([1, 0, 0, 0, 2, 0, 0, 0, 0, 0, 0, 0]);
    let filter: MeshFilter = decode_one(ClassId::MESH_FILTER, bytes, false).unwrap();
    assert_eq!(filter.mesh, ptr(1, 2));
}

#[test]
fn decodes_mesh_renderers_in_both_byte_orders() {
    let settings = RendererSettings {
        cast_shadows: 1,
        receive_shadows: 2,
        dynamic_occludee: 3,
        static_shadow_caster: 4,
        motion_vectors: 5,
        light_probe_usage: 6,
        reflection_probe_usage: 7,
        ray_tracing_mode: 8,
        ray_trace_procedural: 9,
        ray_tracing_build_flags_override: 10,
        ray_tracing_build_flags: 11,
        small_mesh_culling: 12,
    };
    let expected = MeshRenderer {
        renderer: Renderer {
            game_object: ptr(0, 8),
            enabled: true,
            settings,
            force_mesh_lod: -1,
            mesh_lod_selection_bias: 0.25,
            rendering_layer_mask: u32::MAX,
            renderer_priority: -3,
            lightmap_index: 65_535,
            lightmap_index_dynamic: 7,
            lightmap_tiling_offset: [1.0, 1.0, 0.0, 0.0],
            lightmap_tiling_offset_dynamic: [2.0, 2.0, 0.5, 0.5],
            materials: vec![ptr(0, 3), ptr(2, -4)],
            static_batch: [1, 2],
            static_batch_root: ptr(0, 11),
            probe_anchor: ptr(0, 12),
            light_probe_volume_override: ptr(0, 13),
            sorting_layer_id: 9,
            sorting_layer: -2,
            sorting_order: 5,
            mask_interaction: 1,
        },
        additional_vertex_streams: ptr(0, 14),
        enlighten_vertex_stream: ptr(0, 0),
    };
    for big_endian in [false, true] {
        let bytes = mesh_renderer_bytes(big_endian);
        let decoded: MeshRenderer = decode_one(ClassId::MESH_RENDERER, bytes, big_endian).unwrap();
        assert_eq!(decoded, expected);
    }
}

fn pair(a: &str, b: &str) -> (String, String) {
    (a.to_owned(), b.to_owned())
}

#[test]
fn decodes_materials_with_every_property_kind() {
    let main_tex = TexEnv {
        texture: ptr(0, 5),
        scale: [2.0, 3.0],
        offset: [0.5, 0.25],
    };
    let expected = Material {
        name: "Water".into(),
        shader: ptr(3, 6_704),
        valid_keywords: vec!["_NORMALMAP".into()],
        invalid_keywords: vec!["_EMISSION".into()],
        lightmap_flags: 5,
        enable_instancing_variants: true,
        double_sided_gi: false,
        custom_render_queue: 3_000,
        tags: vec![pair("RenderType", "Opaque")],
        disabled_shader_passes: vec!["SHADOWCASTER".into()],
        properties: SavedProperties {
            textures: vec![("_MainTex".into(), main_tex)],
            ints: vec![("_Mode".into(), 4)],
            floats: vec![("_Glossiness".into(), 0.75)],
            colors: vec![("_Color".into(), [1.0, 0.5, 0.25, 1.0])],
        },
        build_texture_stacks: vec![pair("Terrain", "Albedo")],
    };
    for big_endian in [false, true] {
        let bytes = material_bytes(big_endian);
        let decoded: Material = decode_one(ClassId::MATERIAL, bytes, big_endian).unwrap();
        assert_eq!(decoded, expected);
    }
}

/// The texture [`texture_bytes`] writes. Its image data starts at byte 108.
fn expected_texture(inline: bool) -> Texture2D {
    let stream = if inline {
        StreamingInfo {
            offset: 0,
            size: 0,
            path: String::new(),
        }
    } else {
        StreamingInfo {
            offset: 4_096,
            size: 87_408,
            path: "sharedassets2.assets.resS".into(),
        }
    };
    Texture2D {
        name: "Hex".into(),
        is_alpha_channel_optional: false,
        width: 256,
        height: 128,
        complete_image_size: 87_408,
        format: 12,
        is_readable: true,
        is_pre_processed: false,
        mipmaps: Mipmaps {
            stripped: 1,
            count: 9,
            ignore_limit: true,
            limit_group: "Terrain".into(),
            streaming: true,
            streaming_priority: -2,
        },
        image_count: 1,
        dimension: 2,
        settings: TextureSettings {
            filter_mode: 1,
            aniso: 4,
            mip_bias: -0.5,
            wrap: [0, 1, 2],
        },
        lightmap_format: 6,
        color_space: 1,
        platform_blob: 104..104,
        image_data: if inline { 108..113 } else { 108..108 },
        stream,
    }
}

#[test]
fn decodes_streamed_and_inline_textures() {
    for big_endian in [false, true] {
        for inline in [false, true] {
            let bytes = texture_bytes(big_endian, inline);
            let decoded: Texture2D =
                decode_one(ClassId::TEXTURE_2D, bytes.clone(), big_endian).unwrap();
            assert_eq!(decoded, expected_texture(inline));
            let image = &bytes[decoded.image_data];
            assert_eq!(image, if inline { &[1, 2, 3, 4, 5][..] } else { &[] });
        }
    }
}

fn every_prefix_fails<T: Layout + std::fmt::Debug>(class: ClassId, bytes: &[u8]) {
    for end in 0..bytes.len() {
        let error = decode_one::<T>(class, bytes[..end].to_vec(), false).unwrap_err();
        assert!(
            matches!(
                error,
                SerializedError::Truncated { .. } | SerializedError::InvalidCount { .. }
            ),
            "{class} prefix {end}: {error}"
        );
    }
}

#[test]
fn every_truncated_graphics_object_fails() {
    every_prefix_fails::<MeshFilter>(ClassId::MESH_FILTER, &mesh_filter_bytes(false));
    every_prefix_fails::<MeshRenderer>(ClassId::MESH_RENDERER, &mesh_renderer_bytes(false));
    every_prefix_fails::<Material>(ClassId::MATERIAL, &material_bytes(false));
    every_prefix_fails::<Texture2D>(ClassId::TEXTURE_2D, &texture_bytes(false, true));
}

#[test]
fn rejects_counts_and_lengths_the_object_cannot_hold() {
    // A material whose valid keyword count claims more strings than bytes remain.
    let mut bytes = material_bytes(false);
    // Name "Water" (4 + 5 + 3 padding) and the shader reference precede the keyword count.
    bytes[24..28].copy_from_slice(&1_000_000_i32.to_le_bytes());
    let error = decode_one::<Material>(ClassId::MATERIAL, bytes, false).unwrap_err();
    assert!(
        matches!(
            error,
            SerializedError::InvalidCount {
                what: "valid keyword",
                ..
            }
        ),
        "{error}"
    );

    let mut bytes = mesh_renderer_bytes(false);
    let error = decode_one::<MeshRenderer>(
        ClassId::MESH_RENDERER,
        {
            bytes.push(0);
            bytes
        },
        false,
    )
    .unwrap_err();
    assert!(
        matches!(error, SerializedError::TrailingBytes { .. }),
        "{error}"
    );
}
