use std::path::PathBuf;
use std::sync::OnceLock;

use libloading::Library;

use super::{NativeImageLease, rejected};
use crate::{HostError, ManagedHostConfig, load_ffi_image};

struct ProcessImage {
    lease: NativeImageLease,
    _library: Library,
}

// CLR retains a configured pathname and native resolver; native TLS can survive
// host destruction. One immutable image is therefore pinned until process exit.
static IMAGE: OnceLock<ProcessImage> = OnceLock::new();

pub(super) struct Attachment {
    lease: Option<NativeImageLease>,
    pub(super) path: PathBuf,
}

impl Attachment {
    // HostClaim must be held throughout selection, loading and commitment.
    pub(super) fn select(lease: NativeImageLease) -> Result<Self, HostError> {
        if let Some(image) = IMAGE.get() {
            if !image.lease.same_image(&lease) {
                return Err(rejected("a different process image is already retained"));
            }
            return Ok(Self {
                lease: None,
                path: image.lease.load_path(),
            });
        }
        let path = lease.load_path();
        Ok(Self {
            lease: Some(lease),
            path,
        })
    }

    pub(super) fn retain_before_configuration(
        self,
        config: &ManagedHostConfig,
    ) -> Result<(), HostError> {
        if let Some(lease) = self.lease {
            let library = load_ffi_image(config)?;
            IMAGE
                .set(ProcessImage {
                    lease,
                    _library: library,
                })
                .map_err(|_| rejected("process image was concurrently configured"))?;
        }
        Ok(())
    }
}
