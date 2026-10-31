//! Scene-owned page preparation. Sources supply decoded linear material layers.
mod capture;
pub(super) mod table;
use super::{
    composition::{LayerSample, compose_page},
    mip_gpu::PageWriter,
    residency::{PageCacheError, ResidentPages},
    selection::{PageKey, PageSelection},
    view,
};
pub use capture::{TerrainResidencyError, TerrainResidencyRequest};
use worldrunner_render_gpu::{
    configuration::{AllocationBudget, ConfigurationError, DriverAllocationError},
    submission::SubmissionAuthority,
};
use worldrunner_render_plan::TerrainPageSettings;
use worldrunner_render_post::FrameError;

pub struct TerrainPageSource {
    pub settings: TerrainPageSettings,
    /// Must hold the complete bounded selection tree, including its root.
    pub slots: u32,
    /// Combined allocation ceiling for pages, upload scratch and draw tables.
    pub budget: AllocationBudget,
    /// Samples global normalized terrain U/V, including gutters outside [0, 1].
    /// Increasing source texel column and row follow increasing U and V with no
    /// vertical flip. Decode normalized normal channels as `2 * value - 1`.
    /// Returned [`LayerSample::normal`] directions must already be inverse-rotated
    /// into the common east/north material frame; page composition does not rotate them.
    pub sample: Box<dyn Fn([f64; 2]) -> Vec<LayerSample>>,
}

pub(crate) struct ScenePages {
    pub cache: ResidentPages,
    identity: u64,
    generation: u64,
    writer: PageWriter,
    sample: Box<dyn Fn([f64; 2]) -> Vec<LayerSample>>,
    layered: Option<std::sync::Arc<super::layered_gpu::LayeredGpuSource>>,
    pub selection: Option<PageSelection>,
    pub bounds: [f32; 4],
    pub elevation: [f32; 2],
    pub published_selection: std::rc::Rc<std::cell::RefCell<Option<PageSelection>>>,
    pub draw_residents: Vec<super::residency::ResidentPage>,
    draw_transaction: Option<worldrunner_render_gpu::submission::Publications>,
    pub tables: Vec<wgpu::Buffer>,
    pub groups: Vec<wgpu::BindGroup>,
    pub frame_slot: usize,
    capture_uploads: Option<capture::CaptureUploads>,
    draw_pending: bool,
    prepared_view: Option<(glam::Mat4, [u32; 2])>,
}
impl std::fmt::Debug for ScenePages {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ScenePages")
            .field("selection", &self.selection)
            .finish_non_exhaustive()
    }
}
impl ScenePages {
    #[cfg(test)]
    pub fn new(
        device: &wgpu::Device,
        authority: &SubmissionAuthority,
        source: TerrainPageSource,
        bounds: [f32; 4],
    ) -> Result<Self, ConfigurationError<DriverAllocationError>> {
        Self::with_layered(device, authority, source, bounds, None)
    }

    pub fn with_layered(
        device: &wgpu::Device,
        authority: &SubmissionAuthority,
        source: TerrainPageSource,
        bounds: [f32; 4],
        layered: Option<std::sync::Arc<super::layered_gpu::LayeredGpuSource>>,
    ) -> Result<Self, ConfigurationError<DriverAllocationError>> {
        let TerrainPageSource {
            settings,
            slots,
            budget,
            sample,
        } = source;
        let (cache, tables) = allocate_pages(
            device,
            authority,
            settings,
            slots,
            budget,
            layered.as_deref(),
        )?;
        Ok(Self {
            cache,
            identity: capture::identity(),
            generation: 1,
            writer: PageWriter::new(device, settings),
            sample,
            layered,
            selection: None,
            bounds,
            elevation: [0.0; 2],
            published_selection: std::rc::Rc::default(),
            draw_residents: Vec::new(),
            draw_transaction: None,
            tables,
            groups: Vec::new(),
            frame_slot: 0,
            capture_uploads: None,
            draw_pending: false,
            prepared_view: None,
        })
    }

    pub fn prepare(
        &mut self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        matrix: glam::Mat4,
        viewport: [u32; 2],
    ) -> Result<(), FrameError> {
        let transaction = worldrunner_render_gpu::submission::Publications::for_encoder(encoder)?;
        if self.draw_pending
            && self.prepared_view == Some((matrix, viewport))
            && self
                .draw_transaction
                .as_ref()
                .is_some_and(|prepared| transaction.same_transaction(prepared))
        {
            // The metadata prefix already uploaded this selection. Keep its draw
            // table, including pending pages, until this transaction submits.
            return Ok(());
        }
        self.prepare_inner(device, encoder, matrix, viewport, false)
    }

    pub fn prepare_depth_prefix(
        &mut self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        matrix: glam::Mat4,
        viewport: [u32; 2],
    ) -> Result<(), FrameError> {
        self.prepare_inner(device, encoder, matrix, viewport, true)
    }

    fn prepare_inner(
        &mut self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        matrix: glam::Mat4,
        viewport: [u32; 2],
        draw_pending: bool,
    ) -> Result<(), FrameError> {
        self.draw_pending = draw_pending;
        if let Some(uploads) = &mut self.capture_uploads {
            uploads.reset();
        }
        self.select(matrix, viewport);
        let selection = self.selection.as_ref().expect("selected").clone();
        let mut required = std::collections::BTreeSet::from([PageKey::ROOT]);
        for key in selection.leaves() {
            for depth in 0..=key.depth() {
                required.insert(key.ancestor(depth).expect("ancestor"));
            }
        }
        let protected: Vec<_> = required.into_iter().collect();
        for &key in &protected {
            if self
                .cache
                .lookup(key, 0)
                .is_some_and(|page| page.key() == key)
            {
                continue;
            }
            let pending = match self.cache.reserve(encoder, key) {
                Ok(pending) => pending,
                Err(PageCacheError::Full) => {
                    self.cache
                        .evict_unselected(&protected)
                        .expect("admitted selection tree fits cache");
                    self.cache
                        .reserve(encoder, key)
                        .map_err(preparation_error)?
                }
                Err(PageCacheError::UploadLimit | PageCacheError::ScratchFull) => break,
                Err(error) => return Err(preparation_error(error)),
            };
            if self.layered.is_some() {
                self.encode_layered(device, encoder, pending)?;
                continue;
            }
            let image = compose_page(self.cache.settings(), key, &self.sample)
                .map_err(preparation_error)?;
            if let Some(uploads) = &mut self.capture_uploads {
                uploads.encode(
                    &self.writer,
                    device,
                    encoder,
                    &self.cache,
                    (pending, &image),
                )?;
            } else {
                self.writer
                    .encode(device, encoder, &self.cache, pending, &image)
                    .map_err(preparation_error)?;
            }
        }
        self.prepare_draw(device, encoder, &protected)?;
        self.prepared_view = Some((matrix, viewport));
        Ok(())
    }

    fn encode_layered(
        &mut self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        pending: super::residency::PendingPage,
    ) -> Result<(), FrameError> {
        let source = self.layered.as_ref().expect("layered source");
        if let Some(uploads) = &mut self.capture_uploads {
            uploads.encode_layered(source, device, encoder, &self.cache, pending)
        } else {
            source
                .encode_page(device, encoder, &self.cache, pending)
                .map_err(preparation_error)
        }
    }

    fn prepare_draw(
        &mut self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        protected: &[PageKey],
    ) -> Result<(), FrameError> {
        worldrunner_render_gpu::submission::Publications::retain_lease(
            encoder,
            &self.cache.storage().owner.lease(),
        )
        .map_err(preparation_error)?;
        self.draw_residents = protected
            .iter()
            .filter_map(|key| {
                if self.draw_pending {
                    self.cache.lookup_for_drawing(*key)
                } else {
                    self.cache.lookup(*key, 0).filter(|page| page.key() == *key)
                }
            })
            .collect();
        self.draw_transaction =
            Some(worldrunner_render_gpu::submission::Publications::for_encoder(encoder)?);
        table::upload(self, device, encoder)?;
        let destination = self.published_selection.clone();
        let selection = self.selection.clone();
        crate::compositor::publication::encode(encoder, &destination.clone(), move || {
            *destination.borrow_mut() = selection;
        })?;
        Ok(())
    }

    pub fn select(&mut self, matrix: glam::Mat4, viewport: [u32; 2]) {
        self.selection = Some(view::select_volume(
            self.cache.settings(),
            self.published_selection.borrow().as_ref(),
            &view::PageView {
                matrix,
                viewport,
                origin: [self.bounds[0], self.bounds[1]],
                extent: [self.bounds[2], self.bounds[3]],
                elevation: self.elevation,
            },
        ));
    }

    pub fn draw_group(&self) -> Option<wgpu::BindGroup> {
        assert!(
            self.draw_transaction.as_ref().is_some_and(|owner| {
                worldrunner_render_gpu::submission::Publications::current()
                    .is_some_and(|current| current.same_transaction(owner))
            }),
            "prepare material pages in the drawing transaction"
        );
        assert!(
            self.selection.is_some(),
            "prepare material pages before drawing"
        );
        assert!(
            self.draw_residents.iter().all(|page| {
                if self.draw_pending {
                    self.cache.lookup_for_drawing(page.key()) == Some(*page)
                } else {
                    self.cache.is_current(*page)
                }
            }),
            "material pages changed after draw preparation"
        );
        self.draw_residents
            .iter()
            .any(|page| page.key() == PageKey::ROOT)
            .then(|| self.groups[self.frame_slot].clone())
    }

    pub fn ready(&self) -> bool {
        self.selection
            .as_ref()
            .is_some_and(|selection| self.cache.ready(selection))
    }
}

fn preparation_error(error: impl std::fmt::Display) -> FrameError {
    FrameError::FeaturePreparation {
        feature: "terrain material pages",
        reason: error.to_string(),
    }
}

fn allocate_pages(
    device: &wgpu::Device,
    authority: &SubmissionAuthority,
    settings: TerrainPageSettings,
    slots: u32,
    budget: AllocationBudget,
    layered: Option<&super::layered_gpu::LayeredGpuSource>,
) -> Result<(ResidentPages, Vec<wgpu::Buffer>), ConfigurationError<DriverAllocationError>> {
    let leaves = 1 + (u64::from(settings.parts().maximum_selected_pages) - 1) / 3 * 3;
    let nodes = ((leaves * 4 - 1) / 3)
        .min(((1_u64 << (2 * (u32::from(settings.parts().maximum_depth) + 1))) - 1) / 3);
    if u64::from(slots) < nodes {
        return Err(ConfigurationError::Invalid(
            "page slots must hold the bounded selection tree",
        ));
    }
    let bytes = table::bytes(settings, slots);
    let reserved = bytes.checked_mul(6).ok_or(ConfigurationError::Invalid(
        "page table allocation overflow",
    ))?;
    let page_budget = budget
        .0
        .checked_sub(reserved)
        .ok_or(ConfigurationError::Budget {
            required: reserved,
            budget: budget.0,
        })?;
    if bytes > device.limits().max_buffer_size
        || bytes > device.limits().max_storage_buffer_binding_size
    {
        return Err(ConfigurationError::Invalid(
            "page table exceeds device limits",
        ));
    }
    let cache = if let Some(source) = &layered {
        ResidentPages::new_layered(
            device,
            authority,
            settings,
            slots,
            AllocationBudget(page_budget),
            source,
        )?
    } else {
        ResidentPages::new(
            device,
            authority,
            settings,
            slots,
            AllocationBudget(page_budget),
        )?
    };
    let tables = table::allocate(device, bytes).map_err(ConfigurationError::Allocation)?;
    Ok((cache, tables))
}
