use crate::{
    EnvironmentInstance, GenerationParameters, TerrainDocument, WorldDocument, WorldOverride,
    WorldValidationError,
};
use worldrunner_asset_id::QualityTier;
use worldrunner_core_types::WorldStateHash;

/// Fresh v1 document hash: BLAKE3 over domain-separated, length-prefixed fields in
/// declaration order, little-endian integers and IEEE-754 floats (negative zero normalized).
/// Collection order is significant. No historical document encoding is accepted.
pub fn canonical_world_hash(world: &WorldDocument) -> Result<WorldStateHash, WorldValidationError> {
    world.validate()?;
    let mut canonical = CanonicalWorldHasher::new();
    canonical.bytes(b"WORLDRUNNER-WORLD-V1");
    encode_identity(&mut canonical, world);
    encode_generation_parameters(&mut canonical, &world.generation_parameters);
    encode_terrain(&mut canonical, &world.terrain);
    encode_biomes(&mut canonical, world);
    canonical.length(world.environment_instances.len());
    for instance in &world.environment_instances {
        encode_environment_instance(&mut canonical, instance);
    }
    encode_catalog(&mut canonical, world);
    encode_overrides(&mut canonical, world);
    Ok(canonical.finish())
}

fn encode_identity(canonical: &mut CanonicalWorldHasher, world: &WorldDocument) {
    canonical.u32(world.schema_version.0);
    canonical.u32(world.generator_version.0);
    canonical.u8(world.coordinate_frame.discriminant());
    canonical.bytes(world.world_id.as_uuid().as_bytes());
    canonical.u64(world.seed.0);
    canonical.u32(world.dimensions.width_meters);
    canonical.u32(world.dimensions.depth_meters);
}

fn encode_environment_instance(
    canonical: &mut CanonicalWorldHasher,
    instance: &EnvironmentInstance,
) {
    canonical.u64(instance.instance_id);
    canonical.string(instance.asset_id.as_str());
    canonical.f64(instance.position.east_meters);
    canonical.f64(instance.position.elevation_meters);
    canonical.f64(instance.position.north_meters);
    for component in instance.orientation.quaternion_xyzw {
        canonical.f64(component);
    }
    canonical.u8(instance.placement.discriminant());
    canonical.u32(instance.uniform_scale_millionths);
    canonical.u8(u8::from(instance.visible));
}

fn encode_biomes(canonical: &mut CanonicalWorldHasher, world: &WorldDocument) {
    canonical.length(world.biome_definitions.len());
    for biome in &world.biome_definitions {
        canonical.u32(biome.definition_id);
        canonical.string(biome.biome_asset_id.as_str());
    }
    canonical.length(world.biome_regions.len());
    for region in &world.biome_regions {
        canonical.u32(region.region_id);
        canonical.u32(region.biome_definition_id);
        canonical.i64(region.center_east_millimeters);
        canonical.i64(region.center_north_millimeters);
        canonical.u64(region.radius_millimeters);
    }
}

fn encode_catalog(canonical: &mut CanonicalWorldHasher, world: &WorldDocument) {
    canonical.u32(world.catalog.version.0);
    canonical.length(world.catalog.preferred_art_sets.len());
    for value in &world.catalog.preferred_art_sets {
        canonical.string(value);
    }
    canonical.length(world.catalog.preferred_platform_profiles.len());
    for value in &world.catalog.preferred_platform_profiles {
        canonical.string(value);
    }
    canonical.u8(match world.catalog.maximum_quality {
        QualityTier::Baseline => 0,
        QualityTier::High => 1,
        QualityTier::Ultra => 2,
    });
}

fn encode_overrides(canonical: &mut CanonicalWorldHasher, world: &WorldDocument) {
    canonical.length(world.authored_overrides.len());
    for authored in &world.authored_overrides {
        canonical.u64(authored.override_id);
        match authored.operation {
            WorldOverride::TerrainHeight {
                sample_index,
                height_millimeters,
            } => {
                canonical.u8(0);
                canonical.u64(sample_index);
                canonical.i32(height_millimeters);
            }
            WorldOverride::RemoveEnvironment { instance_id } => {
                canonical.u8(1);
                canonical.u64(instance_id);
            }
        }
    }
}

fn encode_generation_parameters(
    canonical: &mut CanonicalWorldHasher,
    value: &GenerationParameters,
) {
    canonical.i32(value.sea_level_millimeters);
    canonical.u32(value.elevation_amplitude_millimeters);
    canonical.u32(value.terrain_frequency_microunits);
    canonical.u32(value.environment_density_per_square_kilometer);
}

fn encode_terrain(canonical: &mut CanonicalWorldHasher, terrain: &TerrainDocument) {
    canonical.u32(terrain.width_samples);
    canonical.u32(terrain.depth_samples);
    canonical.u32(terrain.sample_spacing_millimeters);
    canonical.length(terrain.heights_millimeters.len());
    for &value in &terrain.heights_millimeters {
        canonical.i32(value);
    }
    canonical.length(terrain.material_palette.len());
    for slot in &terrain.material_palette {
        canonical.string(slot.material_id.as_str());
        canonical.u32(slot.uv_scale_millimeters);
    }
    canonical.length(terrain.material_indices.len());
    for &value in &terrain.material_indices {
        canonical.u16(value);
    }
}

struct CanonicalWorldHasher(blake3::Hasher);

impl CanonicalWorldHasher {
    fn new() -> Self {
        Self(blake3::Hasher::new())
    }

    fn finish(self) -> WorldStateHash {
        WorldStateHash(*self.0.finalize().as_bytes())
    }

    fn bytes(&mut self, value: &[u8]) {
        self.length(value.len());
        self.0.update(value);
    }

    fn string(&mut self, value: &str) {
        self.bytes(value.as_bytes());
    }

    fn length(&mut self, value: usize) {
        self.u64(u64::try_from(value).expect("world collection length fits in u64"));
    }

    fn u8(&mut self, value: u8) {
        self.0.update(&[value]);
    }

    fn u16(&mut self, value: u16) {
        self.0.update(&value.to_le_bytes());
    }

    fn u32(&mut self, value: u32) {
        self.0.update(&value.to_le_bytes());
    }

    fn i32(&mut self, value: i32) {
        self.0.update(&value.to_le_bytes());
    }

    fn u64(&mut self, value: u64) {
        self.0.update(&value.to_le_bytes());
    }

    fn i64(&mut self, value: i64) {
        self.0.update(&value.to_le_bytes());
    }

    fn f64(&mut self, value: f64) {
        let normalized = if value == 0.0 { 0.0 } else { value };
        self.u64(normalized.to_bits());
    }
}
