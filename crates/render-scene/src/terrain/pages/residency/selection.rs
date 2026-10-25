use super::{ResidentPage, ResidentPages};
use crate::terrain::pages::selection::{PageKey, PageSelection};

impl ResidentPages {
    /// Captures require exact selected pages, never a resident ancestor substitute.
    pub fn ready(&self, selection: &PageSelection) -> bool {
        let state = self.storage().state.borrow();
        selection
            .leaves()
            .iter()
            .all(|key| state.lookup(*key).is_some())
    }

    /// Evicts the deepest unused page first, preserving the root and all requested pages.
    /// Address order breaks ties deterministically. Pending writes are never evicted.
    pub fn evict_unselected(&self, protected: &[PageKey]) -> Option<PageKey> {
        let protected: std::collections::BTreeSet<_> = protected.iter().copied().collect();
        let mut state = self.storage().state.borrow_mut();
        let victim = state
            .residents()
            .map(ResidentPage::key)
            .filter(|key| *key != PageKey::ROOT && !protected.contains(key))
            .max_by_key(|key| (key.depth(), *key));
        if let Some(key) = victim {
            state.remove(key);
        }
        victim
    }

    pub fn residents(&self) -> Vec<ResidentPage> {
        let state = self.storage().state.borrow();
        state.residents().collect()
    }

    pub fn texture_view(&self) -> wgpu::TextureView {
        self.storage()
            .texture
            .create_view(&wgpu::TextureViewDescriptor {
                dimension: Some(wgpu::TextureViewDimension::D2Array),
                ..Default::default()
            })
    }
}
