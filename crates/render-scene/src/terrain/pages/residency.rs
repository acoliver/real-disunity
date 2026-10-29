//! Submitted material pages in a budgeted texture array.
pub(super) mod allocation;
pub(crate) use allocation::validate_layers;
mod selection;
mod state;
use super::selection::PageKey;
use state::PageState;
pub use state::{PendingPage, ResidentPage};
use std::{cell::RefCell, rc::Rc};
use worldrunner_render_gpu::{
    configuration::{
        AllocationBudget, AllocationSet, Configuration, ConfigurationError, DriverAllocationError,
    },
    submission::{
        FrameOwner, Publications, SubmissionAuthority, SubmitError, TransactionError, capture,
    },
};
use worldrunner_render_plan::TerrainPageSettings;

#[derive(Debug, thiserror::Error)]
pub enum PageCacheError {
    #[error(transparent)]
    Submission(#[from] SubmitError),
    #[error(transparent)]
    Transaction(#[from] TransactionError),
    #[error("the per-frame material upload limit is exhausted")]
    UploadLimit,
    #[error("material upload scratch is still reserved or in flight")]
    ScratchFull,
    #[error("all material page slots are occupied")]
    Full,
    #[error("page is already resident or pending, or exceeds configured depth")]
    Address,
    #[error(transparent)]
    Capture(#[from] capture::WriteError),
}

pub(super) struct Storage {
    pub texture: wgpu::Texture,
    pub state: Rc<RefCell<PageState>>,
    pub owner: FrameOwner<()>,
}

/// No queue is owned here. All encoding and publication use the frame authority.
pub struct ResidentPages {
    configuration: Configuration<Option<Storage>>,
    authority: SubmissionAuthority,
    settings: TerrainPageSettings,
    products: u32,
    composition_bytes: u64,
    source_allocations: AllocationSet,
}

impl ResidentPages {
    pub(in crate::terrain::pages) fn queue(&self) -> &wgpu::Queue {
        self.authority.queue()
    }

    pub fn new(
        device: &wgpu::Device,
        authority: &SubmissionAuthority,
        settings: TerrainPageSettings,
        slots: u32,
        budget: AllocationBudget,
    ) -> Result<Self, ConfigurationError<DriverAllocationError>> {
        Self::with_products(
            device,
            authority,
            settings,
            slots,
            budget,
            (2, 0, AllocationSet::default()),
        )
    }

    pub fn new_layered(
        device: &wgpu::Device,
        authority: &SubmissionAuthority,
        settings: TerrainPageSettings,
        slots: u32,
        budget: AllocationBudget,
        source: &super::layered_gpu::LayeredGpuSource,
    ) -> Result<Self, ConfigurationError<DriverAllocationError>> {
        Self::with_products(
            device,
            authority,
            settings,
            slots,
            budget,
            (3, source.input_bytes(), source.allocations()),
        )
    }

    fn with_products(
        device: &wgpu::Device,
        authority: &SubmissionAuthority,
        settings: TerrainPageSettings,
        slots: u32,
        budget: AllocationBudget,
        format: (u32, u64, AllocationSet),
    ) -> Result<Self, ConfigurationError<DriverAllocationError>> {
        let (products, composition_bytes, source_allocations) = format;
        let configuration = Configuration::new(
            authority,
            None,
            AllocationSet::default(),
            &device.limits(),
            budget,
        )
        .expect("an empty allocation set fits every budget");
        let mut cache = Self {
            configuration,
            authority: authority.clone(),
            settings,
            products,
            composition_bytes,
            source_allocations,
        };
        cache.replace(device, slots)?;
        Ok(cache)
    }

    pub fn replace(
        &mut self,
        device: &wgpu::Device,
        slots: u32,
    ) -> Result<(), ConfigurationError<DriverAllocationError>> {
        validate_layers(&device.limits(), slots, self.products)?;
        self.source_allocations
            .validate_array_layers(&device.limits())?;
        let allocations = allocation::allocations(
            self.settings,
            slots,
            self.products,
            &self.source_allocations,
        )?;
        self.configuration
            .replace(&device.limits(), allocations, || {
                allocation::create(device, &self.authority, self.settings, slots, self.products)
                    .map(Some)
            })
    }

    pub fn allocation_bytes(&self, device: &wgpu::Device) -> Result<u64, &'static str> {
        self.configuration.resident_bytes(&device.limits())
    }

    pub fn collect(&mut self) {
        self.configuration.collect();
    }

    pub fn reserve(
        &self,
        encoder: &wgpu::CommandEncoder,
        key: PageKey,
    ) -> Result<PendingPage, PageCacheError> {
        let transaction = Publications::for_encoder(encoder)?;
        let storage = self.storage();
        Publications::retain_lease(encoder, &storage.owner.lease())?;
        let mut state = storage.state.borrow_mut();
        state.begin_frame(&transaction);
        if key.depth() > self.settings.parts().maximum_depth || state.contains(key) {
            return Err(PageCacheError::Address);
        }
        let bytes = self.page_bytes();
        if state.uploaded + bytes > self.settings.parts().upload_bytes_per_frame {
            return Err(PageCacheError::UploadLimit);
        }
        if !state.scratch_available() {
            return Err(PageCacheError::ScratchFull);
        }
        let slot = state.reserve(key).ok_or(PageCacheError::Full)?;
        state.uploaded += bytes;
        Ok(PendingPage::new(storage.state.clone(), transaction, slot))
    }

    /// Nearest submitted ancestor with enough filter support at the requested LOD.
    pub fn lookup(&self, key: PageKey, mip: u32) -> Option<ResidentPage> {
        let state = self.storage().state.borrow();
        for depth in (0..=key.depth()).rev() {
            let ancestor = key.ancestor(depth).expect("shallower depth");
            let local_mip = mip.saturating_sub(u32::from(key.depth() - depth));
            if local_mip <= self.settings.maximum_sampling_mip()
                && let Some(page) = state.lookup(ancestor)
            {
                return Some(page);
            }
        }
        None
    }

    pub(crate) fn capture_write_key(&self) -> capture::WriteKey {
        self.storage().state.borrow().capture_write_key()
    }

    pub(in crate::terrain::pages) fn lookup_for_drawing(
        &self,
        key: PageKey,
    ) -> Option<ResidentPage> {
        self.storage().state.borrow().lookup_for_drawing(key)
    }

    pub fn is_current(&self, page: ResidentPage) -> bool {
        self.storage().state.borrow().is_current(page)
    }

    pub fn evict(&self, key: PageKey) {
        self.storage().state.borrow_mut().remove(key);
    }

    /// Invalidates all intersecting padded footprints, including pending ancestors.
    pub fn invalidate(&self, minimum: [f64; 2], maximum: [f64; 2]) {
        assert!(minimum.into_iter().chain(maximum).all(f64::is_finite));
        assert!((0..2).all(|axis| minimum[axis] <= maximum[axis]));
        let mut state = self.storage().state.borrow_mut();
        let keys: Vec<_> = state
            .keys()
            .filter(|key| {
                let scale = f64::from(1_u32 << key.depth());
                (0..2).all(|axis| {
                    let gutter = f64::from(self.settings.parts().border)
                        / f64::from(self.settings.parts().interior[axis]);
                    let start = (f64::from(key.coordinate()[axis]) - gutter) / scale;
                    let end = (f64::from(key.coordinate()[axis]) + 1.0 + gutter) / scale;
                    minimum[axis] <= end && maximum[axis] >= start
                })
            })
            .collect();
        for key in keys {
            state.remove(key);
        }
    }

    pub(super) fn storage(&self) -> &Storage {
        self.configuration
            .current()
            .as_ref()
            .expect("initialized cache")
    }
    pub fn composition_bytes(&self) -> u64 {
        self.composition_bytes
    }

    pub fn product_count(&self) -> u32 {
        self.products
    }

    pub fn page_bytes(&self) -> u64 {
        self.settings.page_bytes() / 2 * u64::from(self.products)
    }

    pub fn settings(&self) -> TerrainPageSettings {
        self.settings
    }
}
