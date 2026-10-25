use super::ScenePages;
use crate::terrain::pages::selection::PageSelection;
use std::sync::atomic::{AtomicU64, Ordering};
static NEXT_SCENE: AtomicU64 = AtomicU64::new(1);

/// Exact camera selection and source generation declared by a terrain capture.
#[derive(Clone, Debug)]
pub struct TerrainResidencyRequest {
    scene: u64,
    generation: u64,
    selection: PageSelection,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum TerrainResidencyError {
    #[error("terrain residency request belongs to another scene or an edited source generation")]
    Stale,
}

pub(super) fn identity() -> u64 {
    NEXT_SCENE.fetch_add(1, Ordering::Relaxed)
}

impl ScenePages {
    pub fn request(&self) -> TerrainResidencyRequest {
        TerrainResidencyRequest {
            scene: self.identity,
            generation: self.generation,
            selection: self
                .selection
                .as_ref()
                .expect("select a capture camera")
                .clone(),
        }
    }

    pub fn ready_for(
        &self,
        request: &TerrainResidencyRequest,
    ) -> Result<bool, TerrainResidencyError> {
        if request.scene != self.identity || request.generation != self.generation {
            return Err(TerrainResidencyError::Stale);
        }
        Ok(self.cache.ready(&request.selection))
    }

    pub fn invalidate(&mut self, minimum: [f64; 2], maximum: [f64; 2]) {
        self.cache.invalidate(minimum, maximum);
        self.generation = self
            .generation
            .checked_add(1)
            .expect("terrain source generation overflow");
    }
}
