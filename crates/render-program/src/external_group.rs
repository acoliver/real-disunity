//! Groups for the sets a pass owner binds itself ([`ExternalSet`]), built by the runtime from
//! imports, so the owner never builds a bind group of its own.

use crate::bind::imported_resource;
use crate::binding::BindingSource;
use crate::error::ProgramError;
use crate::layout::Slot;
use crate::program::ExternalSet;
use crate::runtime::ProgramRuntime;
use crate::set::ImportTable;

/// The group of an [`ExternalSet`], filled from an [`ImportTable`]. The owner of a pass binds it
/// once with [`bind`](Self::bind) and every draw of a program that lends the set reads it.
#[derive(Clone, Debug)]
pub struct ExternalGroup {
    set: u32,
    group: wgpu::BindGroup,
}

impl ExternalGroup {
    /// The set the group is bound at.
    #[must_use]
    pub fn set(&self) -> u32 {
        self.set
    }

    /// Binds the group at its set in `pass`.
    pub fn bind(&self, pass: &mut wgpu::RenderPass<'_>) {
        pass.set_bind_group(self.set, &self.group, &[]);
    }
}

impl ProgramRuntime {
    /// Builds the group of `external` against its layout, filling every declared binding from
    /// the entry of `table` with the binding's engine semantic. The table may hold entries the
    /// set does not declare. Each import is checked against its declaration as a draw's imports
    /// are: the kind of resource, a texture's sample type, view dimension and sample count, and
    /// a buffer's size.
    ///
    /// # Errors
    /// Fails with `MissingBinding` for a declaration that names no engine semantic,
    /// `MissingImport` for a semantic the table has no entry for, `WrongResource` or
    /// `BufferTooSmall` for an import that does not suit its declaration, on a lost device and
    /// on a GPU validation error.
    pub fn prepare_external_set(
        &self,
        external: &ExternalSet,
        table: &ImportTable,
    ) -> Result<ExternalGroup, ProgramError> {
        self.authority.check_device()?;
        let slots: Vec<Slot> = external
            .bindings
            .bindings
            .iter()
            .map(|declared| Slot {
                set: declared.at.set,
                binding: declared.at.binding,
                visibility: declared.visibility,
                ty: declared.kind.binding_type(),
                min_size: declared.kind.min_size(),
                source: declared.source.clone(),
            })
            .collect();
        let mut entries = Vec::with_capacity(slots.len());
        for slot in &slots {
            let BindingSource::Engine(semantic) = &slot.source else {
                return Err(ProgramError::MissingBinding {
                    set: slot.set,
                    binding: slot.binding,
                });
            };
            let import = table
                .get(semantic)
                .ok_or_else(|| ProgramError::MissingImport {
                    semantic: semantic.to_string(),
                    set: slot.set,
                    binding: slot.binding,
                })?;
            entries.push(wgpu::BindGroupEntry {
                binding: slot.binding,
                resource: imported_resource(&self.device, slot, import)?,
            });
        }
        let group = self.guarded(|| {
            self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("render-program external set"),
                layout: &external.layout,
                entries: &entries,
            })
        })?;
        self.counters.bind_group_built();
        Ok(ExternalGroup {
            set: external.set,
            group,
        })
    }
}
