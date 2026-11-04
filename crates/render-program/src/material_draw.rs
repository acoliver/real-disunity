//! Drawing materials: resolving a material's properties into a prepared packet, recording it
//! into a frame and admitting it into a renderer's pass.

use crate::arena::{AdmittedPrograms, ProgramArena};
use crate::binding::{BindingSource, ResourceKind};
use crate::constants::pack_constants;
use crate::error::ProgramError;
use crate::material::{MaterialDraw, MaterialPacket};
use crate::packet::{TargetSignature, instance_end};
use crate::properties::{PropertySet, PropertyValue, lookup};
use crate::registry::Variant;
use crate::resources::{BoundResource, Resource, TargetHandle};
use crate::runtime::ProgramRuntime;
use crate::variant::UnknownFeatures;

/// The constant snapshots, resource bindings and zeroed properties of one draw.
struct Resolved {
    snapshots: Vec<crate::bind::Snapshot>,
    bindings: Vec<BoundResource>,
    missing: Vec<String>,
}

impl ProgramRuntime {
    /// Resolves `draw` against its material now: selects the variant for the material's
    /// features, packs the constants from the draw's overrides, then the material's
    /// properties, and takes the resources its properties name. The frame `encoder` records
    /// holds the packed buffers' reservations until the frame is cancelled, dropped or
    /// completed on the GPU. Later edits to the material change future packets only.
    ///
    /// # Errors
    /// Fails on a handle that is not live, an encoder that is not the authority's, a feature
    /// set no variant has, a resource property with no value, a value the constants cannot
    /// take, geometry that runs past a buffer or an instance range that ends before it starts.
    pub fn prepare_material_draw(
        &self,
        encoder: &wgpu::CommandEncoder,
        draw: &MaterialDraw,
    ) -> Result<MaterialPacket, ProgramError> {
        self.recording(encoder)?;
        if draw.instances.start > draw.instances.end {
            return Err(ProgramError::InstanceRange {
                start: draw.instances.start,
                end: draw.instances.end,
            });
        }
        let material = self.registry.material(draw.material)?;
        let registered = &material.program;
        let variant = registered
            .variants
            .select(&material.features, UnknownFeatures::Fail)?;
        self.store.check_instanced_geometry(
            &draw.vertices,
            draw.geometry,
            instance_end(&draw.instances),
        )?;
        let layers = [&draw.overrides, &material.properties];
        let resolved = self.resolve(variant, &layers)?;
        let shares = resolved
            .snapshots
            .iter()
            .map(|snapshot| snapshot.reservation.clone())
            .collect();
        self.hold_for_frame(encoder, shares)?;
        Ok(MaterialPacket {
            program: variant.program.clone(),
            state: registered.state.clone(),
            vertices: draw.vertices.clone(),
            bindings: resolved.bindings,
            geometry: draw.geometry,
            instances: draw.instances.clone(),
            snapshots: resolved.snapshots,
            missing: resolved.missing,
        })
    }

    /// Records a prepared packet into a pass over `target`, which keeps what it holds. The
    /// frame retains everything the packet uses, as for [`draw`](Self::draw).
    ///
    /// # Errors
    /// Fails like [`draw`](Self::draw).
    pub fn record_material(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        target: TargetHandle,
        packet: &MaterialPacket,
    ) -> Result<(), ProgramError> {
        self.record_into_target(
            encoder,
            target,
            &packet.draw_packet(),
            Some(&packet.parts()),
        )
    }

    /// Prepares `draw` and records it into `target`: [`prepare_material_draw`] then
    /// [`record_material`](Self::record_material).
    ///
    /// # Errors
    /// Fails like either of them.
    ///
    /// [`prepare_material_draw`]: Self::prepare_material_draw
    pub fn draw_material(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        target: TargetHandle,
        draw: &MaterialDraw,
    ) -> Result<(), ProgramError> {
        let packet = self.prepare_material_draw(encoder, draw)?;
        self.record_material(encoder, target, &packet)
    }

    /// Admits prepared packets into the frame `encoder` records, as
    /// [`admit_draws`](Self::admit_draws) does for plain packets, and draws their instances.
    ///
    /// # Errors
    /// Fails like [`admit_draws`](Self::admit_draws).
    pub fn admit_material_draws(
        &mut self,
        encoder: &wgpu::CommandEncoder,
        arena: &ProgramArena,
        signature: TargetSignature,
        packets: &[MaterialPacket],
    ) -> Result<AdmittedPrograms, ProgramError> {
        let items: Vec<_> = packets
            .iter()
            .map(|packet| (packet.draw_packet(), Some(packet.parts())))
            .collect();
        self.admit_parts(encoder, arena, signature, &items)
    }

    fn resolve(
        &self,
        variant: &Variant,
        layers: &[&PropertySet],
    ) -> Result<Resolved, ProgramError> {
        let layout = variant.program.layout();
        let mut resolved = Resolved {
            snapshots: Vec::new(),
            bindings: Vec::new(),
            missing: Vec::new(),
        };
        for constants in &variant.constants {
            let at = constants.at;
            let Some(slot) = layout.slot(at.set, at.binding) else {
                continue;
            };
            let min_len = usize::try_from(slot.min_size).unwrap_or(usize::MAX);
            let packed = pack_constants(&constants.layout, layers, min_len)?;
            for name in packed.missing {
                if !resolved.missing.contains(&name) {
                    resolved.missing.push(name);
                }
            }
            resolved
                .snapshots
                .push(self.snapshot_buffer(at, &packed.bytes)?);
        }
        for binding in &variant.resources {
            let at = binding.at;
            let covered = variant.constants.iter().any(|constants| constants.at == at);
            let skipped = covered
                || layout.slot(at.set, at.binding).is_none()
                || matches!(binding.kind, ResourceKind::Sampler(_));
            if skipped {
                continue;
            }
            resolved.bindings.push(BoundResource {
                set: at.set,
                binding: at.binding,
                resource: property_resource(&binding.source, at.set, at.binding, layers)?,
            });
        }
        Ok(resolved)
    }
}

/// The resource the property a binding is sourced from holds.
fn property_resource(
    source: &BindingSource,
    set: u32,
    binding: u32,
    layers: &[&PropertySet],
) -> Result<Resource, ProgramError> {
    let BindingSource::Property(id) = source else {
        return Err(ProgramError::MissingBinding { set, binding });
    };
    match lookup(layers, id.as_str()) {
        Some(PropertyValue::Resource(resource)) => Ok(*resource),
        Some(_) => Err(ProgramError::WrongResource {
            set,
            binding,
            expected: "a buffer or texture".to_owned(),
            found: format!("property {id}"),
        }),
        None => Err(ProgramError::MissingProperty {
            property: id.to_string(),
            set,
            binding,
        }),
    }
}
