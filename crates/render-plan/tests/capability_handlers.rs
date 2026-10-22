//! Focused tests for the caller-configured fail-closed capability handlers.
//!
//! All URIs, identifiers, shapes, and limits are synthetic caller-authored
//! strings and values; no dataset identity is compiled here.

use worldrunner_asset_id::{
    ArtifactKind, AssetVariant, LogicalAssetId, ProvenanceClassification, SourceProvenance,
};
use worldrunner_material::{AlphaMode, MaterialArtifact};
use worldrunner_mesh::{MeshPrimitive, MeshVertex, StaticMeshArtifact};
use worldrunner_package::{
    AssetInput, PackageBuilder, PackageKind, PackageLimits, PackageManifest,
};
use worldrunner_render_plan::{
    CapabilityConfigError, ElementPolicy, ExactBytes, ExactCapabilityRoute, GridShape,
    MaterialPayload, PackagePayload, StaticMeshPayload, TexturePayload, U8Grid, U16LeGrid,
    exact_bytes_handler, material_handler, package_handler, static_mesh_handler, texture_handler,
    u8_grid_handler, u16_le_grid_handler,
};
use worldrunner_texture::{
    MipDescriptor, TextureArtifact, TextureDescriptor, TextureFormat, TextureSemantic,
};
use worldrunner_world_model::{ContentDigest, ResourceBinding};
use worldrunner_world_runtime::{
    CapabilityHandler, CapabilitySessionError, HandlerDescriptor, ResourceContribution,
};

const CAPABILITY_BYTES: &str = "test:capability:bytes";
const CAPABILITY_PACKAGE: &str = "test:capability:package";
const CAPABILITY_MESH: &str = "test:capability:mesh";
const CAPABILITY_MATERIAL: &str = "test:capability:material";
const CAPABILITY_TEXTURE: &str = "test:capability:texture";
const CAPABILITY_GRID_U8: &str = "test:capability:grid-u8";
const CAPABILITY_GRID_U16: &str = "test:capability:grid-u16";
const ELEMENT_ROCK: &str = "test:element:rock";

fn digest() -> ContentDigest {
    ContentDigest::from_hex(&"ab".repeat(32)).unwrap()
}

fn resource_binding(
    capability_uri: &str,
    element_uri: Option<&str>,
    byte_length: u64,
) -> ResourceBinding {
    ResourceBinding::new(
        "test:binding:0",
        capability_uri,
        digest(),
        byte_length,
        element_uri,
    )
    .unwrap()
}

fn bare_route(capability_uri: &str, handler_id: &str) -> ExactCapabilityRoute {
    ExactCapabilityRoute::new(
        capability_uri,
        "test:payload:contract",
        handler_id,
        1,
        ElementPolicy::bare(),
    )
    .unwrap()
}

fn stream(
    handler: &dyn CapabilityHandler,
    binding: &ResourceBinding,
    content: &[u8],
) -> Result<ResourceContribution, CapabilitySessionError> {
    let mut session = handler.start_session(binding)?;
    for chunk in content.chunks(5) {
        session.write_chunk(chunk)?;
    }
    session.finish()
}

fn rejected(reason: &str) -> CapabilitySessionError {
    CapabilitySessionError::Rejected {
        reason: reason.to_owned(),
    }
}

fn mesh_artifact() -> StaticMeshArtifact {
    let vertex = |position: [f32; 3], uv: [f32; 2]| MeshVertex {
        position,
        normal: [0.0, 1.0, 0.0],
        tangent: [1.0, 0.0, 0.0, 1.0],
        uv,
    };
    StaticMeshArtifact {
        vertices: vec![
            vertex([0.0, 0.0, 0.0], [0.0, 0.0]),
            vertex([1.0, 0.0, 0.0], [1.0, 0.0]),
            vertex([0.0, 0.0, 1.0], [0.0, 1.0]),
        ],
        indices: vec![0, 1, 2],
        primitives: vec![MeshPrimitive {
            first_index: 0,
            index_count: 3,
            material_slot: 0,
        }],
        material_slots: vec![LogicalAssetId::new("test.material.rock").unwrap()],
        bounds_min: [0.0, 0.0, 0.0],
        bounds_max: [1.0, 0.0, 1.0],
        colors_rg: None,
    }
}

fn material_artifact() -> MaterialArtifact {
    MaterialArtifact {
        albedo_texture: LogicalAssetId::new("test.texture.albedo").unwrap(),
        normal_texture: None,
        roughness_metallic_texture: None,
        ambient_occlusion_texture: None,
        self_illumination_texture: None,
        base_color_factor: [1.0, 0.8, 0.6, 1.0],
        metallic_factor: 0.0,
        roughness_factor: 0.9,
        normal_scale: 1.0,
        alpha_mode: AlphaMode::Opaque,
        alpha_cutoff: 0.5,
        double_sided: false,
        translucent_foliage: false,
        uv_tiling: [1.0, 1.0],
        terrain_distorted: false,
    }
}

fn texture_artifact() -> TextureArtifact {
    TextureArtifact {
        descriptor: TextureDescriptor {
            width: 4,
            height: 4,
            mip_count: 1,
            array_layers: 1,
            format: TextureFormat::Bc1Unorm,
            semantic: TextureSemantic::Data,
        },
        mips: vec![MipDescriptor {
            width: 4,
            height: 4,
            payload_offset: 0,
            payload_length: 8,
        }],
        payload: vec![0_u8; 8],
    }
}

fn package_limits() -> PackageLimits {
    PackageLimits {
        max_package_bytes: 1 << 20,
        max_index_bytes: 1 << 16,
        max_asset_bytes: 1 << 20,
        max_entries: 16,
        max_chunks_per_asset: 16,
        max_total_chunks: 64,
    }
}

fn package_bytes() -> Vec<u8> {
    let provenance = SourceProvenance {
        classification: ProvenanceClassification::Original,
        source_package: "test".to_owned(),
        source_virtual_path: "test.alpha".to_owned(),
        source_sha256: "00".repeat(32),
        importer_id: "test".to_owned(),
        importer_version: "1".to_owned(),
    };
    let mut builder = PackageBuilder::new(PackageManifest {
        package_id: "test.package".to_owned(),
        semantic_version: "0.1.0".to_owned(),
        package_kind: PackageKind::Development,
        content_schema: 1,
    });
    builder.add(AssetInput {
        logical_id: LogicalAssetId::new("test.alpha").unwrap(),
        variant: AssetVariant {
            art_set: "fixture-art".to_owned(),
            quality: worldrunner_asset_id::QualityTier::Baseline,
            platform_profile: "fixture-platform".to_owned(),
        },
        kind: ArtifactKind::Data,
        dependencies: Vec::new(),
        provenance,
        bytes: b"alpha".to_vec(),
    });
    builder.build().unwrap().bytes
}

#[test]
fn exact_bytes_payload_downcasts_with_exact_contract_and_descriptor() {
    let content = b"exact owned bytes".to_vec();
    let handler = exact_bytes_handler(bare_route(CAPABILITY_BYTES, "test:handler:bytes"), 32);
    let binding = resource_binding(CAPABILITY_BYTES, None, content.len() as u64);
    let contribution = stream(handler.as_ref(), &binding, &content).unwrap();

    assert_eq!(contribution.payload_contract_uri, "test:payload:contract");
    assert_eq!(
        contribution.handler,
        HandlerDescriptor::new("test:handler:bytes", 1)
    );
    let payload = contribution.payload.downcast_ref::<ExactBytes>().unwrap();
    assert_eq!(payload.bytes(), content.as_slice());
    assert!(contribution.payload.downcast_ref::<U8Grid>().is_none());
}

#[test]
fn package_artifact_payload_downcasts() {
    let content = package_bytes();
    let handler = package_handler(
        bare_route(CAPABILITY_PACKAGE, "test:handler:package"),
        package_limits(),
    );
    let binding = resource_binding(CAPABILITY_PACKAGE, None, content.len() as u64);
    let contribution = stream(handler.as_ref(), &binding, &content).unwrap();

    let payload = contribution
        .payload
        .downcast_ref::<PackagePayload>()
        .unwrap();
    assert_eq!(payload.package().manifest().package_id, "test.package");
    assert_eq!(payload.package().entries().len(), 1);
    assert!(contribution.payload.downcast_ref::<ExactBytes>().is_none());
}

#[test]
fn static_mesh_material_and_texture_payloads_downcast() {
    let mesh = mesh_artifact().encode().unwrap();
    let handler = static_mesh_handler(bare_route(CAPABILITY_MESH, "test:handler:mesh"), 4096);
    let binding = resource_binding(CAPABILITY_MESH, None, mesh.len() as u64);
    let contribution = stream(handler.as_ref(), &binding, &mesh).unwrap();
    let payload = contribution
        .payload
        .downcast_ref::<StaticMeshPayload>()
        .unwrap();
    assert_eq!(payload.artifact().vertices.len(), 3);

    let material = material_artifact().encode().unwrap();
    let handler = material_handler(
        bare_route(CAPABILITY_MATERIAL, "test:handler:material"),
        4096,
    );
    let binding = resource_binding(CAPABILITY_MATERIAL, None, material.len() as u64);
    let contribution = stream(handler.as_ref(), &binding, &material).unwrap();
    let payload = contribution
        .payload
        .downcast_ref::<MaterialPayload>()
        .unwrap();
    assert_eq!(
        payload.artifact().albedo_texture.as_str(),
        "test.texture.albedo"
    );

    let texture = texture_artifact().encode().unwrap();
    let handler = texture_handler(bare_route(CAPABILITY_TEXTURE, "test:handler:texture"), 4096);
    let binding = resource_binding(CAPABILITY_TEXTURE, None, texture.len() as u64);
    let contribution = stream(handler.as_ref(), &binding, &texture).unwrap();
    let payload = contribution
        .payload
        .downcast_ref::<TexturePayload>()
        .unwrap();
    assert_eq!(payload.artifact().descriptor.width, 4);
}

#[test]
fn element_policy_mismatches_reject_at_session_start() {
    let handler = exact_bytes_handler(bare_route(CAPABILITY_BYTES, "test:handler:bytes"), 32);

    let error = handler
        .start_session(&resource_binding(CAPABILITY_BYTES, Some(ELEMENT_ROCK), 0))
        .err()
        .unwrap();
    assert_eq!(
        error,
        rejected(
            "binding element \"test:element:rock\" does not match the exact element policy \"none\"",
        )
    );

    let route = ExactCapabilityRoute::new(
        CAPABILITY_BYTES,
        "test:payload:contract",
        "test:handler:element",
        1,
        ElementPolicy::element(ELEMENT_ROCK).unwrap(),
    )
    .unwrap();
    let handler = exact_bytes_handler(route, 32);

    let error = handler
        .start_session(&resource_binding(
            CAPABILITY_BYTES,
            Some("test:element:water"),
            0,
        ))
        .err()
        .unwrap();
    assert_eq!(
        error,
        rejected(
            "binding element \"test:element:water\" does not match the exact element policy \"test:element:rock\"",
        )
    );

    let error = handler
        .start_session(&resource_binding(CAPABILITY_BYTES, None, 0))
        .err()
        .unwrap();
    assert_eq!(
        error,
        rejected(
            "binding element \"none\" does not match the exact element policy \"test:element:rock\""
        )
    );

    let contribution = stream(
        handler.as_ref(),
        &resource_binding(CAPABILITY_BYTES, Some(ELEMENT_ROCK), 2),
        b"ok",
    )
    .unwrap();
    assert!(contribution.payload.downcast_ref::<ExactBytes>().is_some());
}

#[test]
fn session_byte_bound_rejects_chunk_before_accumulation() {
    let handler = exact_bytes_handler(bare_route(CAPABILITY_BYTES, "test:handler:bytes"), 8);
    let binding = resource_binding(CAPABILITY_BYTES, None, 8);
    let mut session = handler.start_session(&binding).unwrap();

    session.write_chunk(b"12345678").unwrap();
    let error = session.write_chunk(b"x").err().unwrap();
    assert_eq!(
        error,
        rejected(
            "chunk of 1 bytes would raise the session to 9 bytes, exceeding the configured maximum of 8",
        )
    );

    // The rejected chunk was never accumulated, so the session still finishes
    // with exactly the eight bounded bytes.
    let contribution = session.finish().unwrap();
    assert_eq!(
        contribution
            .payload
            .downcast_ref::<ExactBytes>()
            .unwrap()
            .bytes(),
        b"12345678"
    );
}

#[test]
fn oversized_declared_binding_length_rejects_at_session_start() {
    let handler = exact_bytes_handler(bare_route(CAPABILITY_BYTES, "test:handler:bytes"), 8);
    let error = handler
        .start_session(&resource_binding(CAPABILITY_BYTES, None, 9))
        .err()
        .unwrap();
    assert_eq!(
        error,
        rejected("binding declares 9 content bytes but the session permits at most 8")
    );
}

#[test]
fn session_finish_requires_the_exact_declared_byte_length() {
    let handler = exact_bytes_handler(bare_route(CAPABILITY_BYTES, "test:handler:bytes"), 8);
    let short = stream(
        handler.as_ref(),
        &resource_binding(CAPABILITY_BYTES, None, 8),
        b"1234567",
    )
    .err()
    .unwrap();
    assert_eq!(
        short,
        rejected("session received 7 bytes but the binding declares 8")
    );

    let long = stream(
        handler.as_ref(),
        &resource_binding(CAPABILITY_BYTES, None, 7),
        b"12345678",
    )
    .err()
    .unwrap();
    assert_eq!(
        long,
        rejected("session received 8 bytes but the binding declares 7")
    );
}

#[test]
fn malformed_canonical_artifacts_are_rejected_after_streaming() {
    let corrupt = |mut bytes: Vec<u8>| {
        let last = bytes.len() - 1;
        bytes[last] ^= 1;
        bytes
    };

    let package = corrupt(package_bytes());
    let handler = package_handler(
        bare_route(CAPABILITY_PACKAGE, "test:handler:package"),
        package_limits(),
    );
    let binding = resource_binding(CAPABILITY_PACKAGE, None, package.len() as u64);
    let error = stream(handler.as_ref(), &binding, &package).err().unwrap();
    assert_eq!(
        error,
        rejected("package artifact decode rejected: package index hash does not match")
    );

    let mesh = corrupt(mesh_artifact().encode().unwrap());
    let handler = static_mesh_handler(bare_route(CAPABILITY_MESH, "test:handler:mesh"), 4096);
    let binding = resource_binding(CAPABILITY_MESH, None, mesh.len() as u64);
    let error = stream(handler.as_ref(), &binding, &mesh).err().unwrap();
    assert_eq!(
        error,
        rejected("static mesh artifact decode rejected: mesh artifact hash does not match")
    );

    let material = corrupt(material_artifact().encode().unwrap());
    let handler = material_handler(
        bare_route(CAPABILITY_MATERIAL, "test:handler:material"),
        4096,
    );
    let binding = resource_binding(CAPABILITY_MATERIAL, None, material.len() as u64);
    let error = stream(handler.as_ref(), &binding, &material).err().unwrap();
    assert_eq!(
        error,
        rejected("material artifact decode rejected: material payload hash does not match")
    );

    let texture = texture_artifact().encode().unwrap();
    let truncated = &texture[..texture.len() - 1];
    let handler = texture_handler(bare_route(CAPABILITY_TEXTURE, "test:handler:texture"), 4096);
    let binding = resource_binding(CAPABILITY_TEXTURE, None, truncated.len() as u64);
    let error = stream(handler.as_ref(), &binding, truncated).err().unwrap();
    assert_eq!(
        error,
        rejected("texture artifact decode rejected: texture artifact extent is invalid")
    );
}

#[test]
fn u16_grid_decodes_exact_little_endian_values() {
    let values: Vec<u16> = vec![0x0100, 0x0201, 0x0302, 0x0403, 0x0504, 0x0605];
    let content: Vec<u8> = values
        .iter()
        .flat_map(|value| value.to_le_bytes())
        .collect();
    let handler = u16_le_grid_handler(
        bare_route(CAPABILITY_GRID_U16, "test:handler:grid-u16"),
        GridShape::new(&[2, 3]),
    )
    .unwrap();
    let binding = resource_binding(CAPABILITY_GRID_U16, None, content.len() as u64);
    let contribution = stream(handler.as_ref(), &binding, &content).unwrap();

    let payload = contribution.payload.downcast_ref::<U16LeGrid>().unwrap();
    assert_eq!(payload.shape().extents(), &[2, 3]);
    assert_eq!(payload.values(), values.as_slice());
}

#[test]
fn u8_grid_accepts_coherent_zero_and_scalar_shapes() {
    let handler = u8_grid_handler(
        bare_route(CAPABILITY_GRID_U8, "test:handler:grid-u8"),
        GridShape::new(&[0, 4]),
    )
    .unwrap();
    let binding = resource_binding(CAPABILITY_GRID_U8, None, 0);
    let contribution = stream(handler.as_ref(), &binding, &[]).unwrap();
    let payload = contribution.payload.downcast_ref::<U8Grid>().unwrap();
    assert_eq!(payload.shape().extents(), &[0, 4]);
    assert!(payload.values().is_empty());

    let handler = u8_grid_handler(
        bare_route(CAPABILITY_GRID_U8, "test:handler:grid-u8"),
        GridShape::new(&[]),
    )
    .unwrap();
    let binding = resource_binding(CAPABILITY_GRID_U8, None, 1);
    let contribution = stream(handler.as_ref(), &binding, &[7]).unwrap();
    let payload = contribution.payload.downcast_ref::<U8Grid>().unwrap();
    assert_eq!(payload.values(), &[7]);
}

#[test]
fn grid_lengths_fail_closed_short_long_and_oversized() {
    let handler = u16_le_grid_handler(
        bare_route(CAPABILITY_GRID_U16, "test:handler:grid-u16"),
        GridShape::new(&[3]),
    )
    .unwrap();

    let binding = resource_binding(CAPABILITY_GRID_U16, None, 4);
    let error = stream(handler.as_ref(), &binding, &[0_u8; 4])
        .err()
        .unwrap();
    assert_eq!(
        error,
        rejected("grid payload carries 4 bytes but the exact shape requires 6 bytes")
    );

    let binding = resource_binding(CAPABILITY_GRID_U16, None, 7);
    let error = handler.start_session(&binding).err().unwrap();
    assert_eq!(
        error,
        rejected("binding declares 7 content bytes but the session permits at most 6")
    );

    let binding = resource_binding(CAPABILITY_GRID_U16, None, 6);
    let mut session = handler.start_session(&binding).unwrap();
    session.write_chunk(&[0_u8; 4]).unwrap();
    let error = session.write_chunk(&[0_u8; 3]).err().unwrap();
    assert_eq!(
        error,
        rejected(
            "chunk of 3 bytes would raise the session to 7 bytes, exceeding the configured maximum of 6",
        )
    );
}

#[test]
fn grid_shape_overflow_fails_closed_at_configuration() {
    let error = GridShape::new(&[usize::MAX, 2])
        .byte_length(1)
        .err()
        .unwrap();
    assert_eq!(error, CapabilityConfigError::GridLengthOverflow);

    let error = u16_le_grid_handler(
        bare_route(CAPABILITY_GRID_U16, "test:handler:grid-u16"),
        GridShape::new(&[usize::MAX]),
    )
    .err()
    .unwrap();
    assert_eq!(error, CapabilityConfigError::GridLengthOverflow);

    u8_grid_handler(
        bare_route(CAPABILITY_GRID_U8, "test:handler:grid-u8"),
        GridShape::new(&[usize::MAX]),
    )
    .unwrap();
}

#[test]
fn handlers_never_sniff_or_fall_back_across_decoders_or_routes() {
    // A valid STRPAK stream through the STRMSH handler is rejected by the
    // mesh decoder: format magic never selects a decoder.
    let content = package_bytes();
    let handler = static_mesh_handler(bare_route(CAPABILITY_MESH, "test:handler:mesh"), 1 << 20);
    let binding = resource_binding(CAPABILITY_MESH, None, content.len() as u64);
    let error = stream(handler.as_ref(), &binding, &content).err().unwrap();
    assert_eq!(
        error,
        rejected("static mesh artifact decode rejected: mesh artifact magic is invalid")
    );

    // A grid session never accepts artifact bytes of any shape.
    let content = mesh_artifact().encode().unwrap();
    let handler = u8_grid_handler(
        bare_route(CAPABILITY_GRID_U8, "test:handler:grid-u8"),
        GridShape::new(&[2]),
    )
    .unwrap();
    let binding = resource_binding(CAPABILITY_GRID_U8, None, content.len() as u64);
    let error = handler.start_session(&binding).err().unwrap();
    assert!(matches!(
        error,
        CapabilitySessionError::Rejected { reason } if reason.starts_with("binding declares")
    ));

    // A handler never serves a binding from a different exact capability route.
    let handler = exact_bytes_handler(bare_route(CAPABILITY_BYTES, "test:handler:bytes"), 32);
    let error = handler
        .start_session(&resource_binding(CAPABILITY_MESH, None, 0))
        .err()
        .unwrap();
    assert_eq!(
        error,
        rejected(
            "binding capability \"test:capability:mesh\" does not match the exact route \"test:capability:bytes\"",
        )
    );
}

#[test]
fn route_configuration_is_fail_closed() {
    let error = ExactCapabilityRoute::new(
        "no-scheme",
        "test:payload:contract",
        "test:handler:x",
        1,
        ElementPolicy::bare(),
    )
    .err()
    .unwrap();
    assert!(matches!(
        error,
        CapabilityConfigError::Invalid {
            field: "capability URI",
            ..
        }
    ));

    let error = ExactCapabilityRoute::new(
        CAPABILITY_BYTES,
        "no-scheme",
        "test:handler:x",
        1,
        ElementPolicy::bare(),
    )
    .err()
    .unwrap();
    assert!(matches!(
        error,
        CapabilityConfigError::Invalid {
            field: "payload contract URI",
            ..
        }
    ));

    let error = ExactCapabilityRoute::new(
        CAPABILITY_BYTES,
        "test:payload:contract",
        "",
        1,
        ElementPolicy::bare(),
    )
    .err()
    .unwrap();
    assert!(matches!(
        error,
        CapabilityConfigError::Invalid {
            field: "handler identity",
            ..
        }
    ));

    let error = ElementPolicy::element("no-scheme").err().unwrap();
    assert!(matches!(
        error,
        CapabilityConfigError::Invalid {
            field: "element URI",
            ..
        }
    ));
}
