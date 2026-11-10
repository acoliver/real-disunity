//! Bind group layouts built from declarations, for the sets a renderer binds itself.
//!
//! A renderer that binds a set once per pass builds the set's group against a
//! `wgpu::BindGroupLayout` and lends the set to the programs that read it with an
//! [`ExternalSet`], whose `bindings` say what the shader reads. Both describe one thing, and
//! [`set_layout`](ProgramRuntime::set_layout) builds the first from the second, so the set is
//! written once.

use crate::binding::{ResourceKind, ResourceLayout};
use crate::error::{ProgramError, StorageImageError};
use crate::layout::{LayoutError, check_unique};
use crate::program::ExternalSet;
use crate::runtime::ProgramRuntime;

impl ProgramRuntime {
    /// Builds the bind group layout of the set that `bindings` declares: one entry for each
    /// declaration, with its visibility and the binding type its kind names
    /// ([`ResourceKind::binding_type`]). A buffer's declared `min_size` is the entry's minimum
    /// binding size, so a group is valid against the layout exactly when each resource suits its
    /// declaration. Comparison samplers, unfilterable float textures and non-filtering samplers
    /// are all expressible, because the renderer that builds the groups owns the resources.
    ///
    /// Every call builds a new layout. Groups built against one fit only the pipelines of programs
    /// that were given that layout, so build the layout once and clone it.
    ///
    /// # Errors
    /// Fails with `LayoutError::Declaration` for a declaration with no bindings or with bindings
    /// in more than one set, with `LayoutError::DuplicateDeclaration` for an address declared
    /// twice, with `StorageImage` for a storage image the device cannot bind, and with `Gpu` when
    /// the device rejects the layout.
    pub fn set_layout(
        &self,
        bindings: &ResourceLayout,
    ) -> Result<wgpu::BindGroupLayout, ProgramError> {
        let set = declared_set(bindings)?;
        check_unique(bindings)?;
        let mut entries = Vec::new();
        for declared in &bindings.bindings {
            if let ResourceKind::StorageImage { access, format, .. } = declared.kind
                && !self.supports_storage_image(format, access)
            {
                return Err(ProgramError::StorageImage {
                    set,
                    binding: declared.at.binding,
                    error: StorageImageError::Unsupported { format, access },
                });
            }
            entries.push(wgpu::BindGroupLayoutEntry {
                binding: declared.at.binding,
                visibility: declared.visibility,
                ty: declared.kind.binding_type(),
                count: None,
            });
        }
        self.guarded(|| {
            self.device
                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some("render-program declared set layout"),
                    entries: &entries,
                })
        })
    }

    /// The set that `bindings` declares, with the layout [`set_layout`](Self::set_layout) builds
    /// for it, ready for [`graphics_program_in`](Self::graphics_program_in). The set number is
    /// the one the declarations name.
    ///
    /// # Errors
    /// As [`set_layout`](Self::set_layout).
    pub fn external_set(&self, bindings: ResourceLayout) -> Result<ExternalSet, ProgramError> {
        let set = declared_set(&bindings)?;
        let layout = self.set_layout(&bindings)?;
        Ok(ExternalSet {
            set,
            layout,
            bindings,
        })
    }
}

/// The one set every declaration is at.
fn declared_set(bindings: &ResourceLayout) -> Result<u32, LayoutError> {
    let Some(first) = bindings.bindings.first() else {
        return Err(LayoutError::Declaration {
            reason: "a set layout needs at least one binding".to_owned(),
        });
    };
    let set = first.at.set;
    match bindings
        .bindings
        .iter()
        .find(|declared| declared.at.set != set)
    {
        Some(stray) => Err(LayoutError::Declaration {
            reason: format!(
                "a set layout is for one set, and the declarations name sets {set} and {}",
                stray.at.set
            ),
        }),
        None => Ok(set),
    }
}
