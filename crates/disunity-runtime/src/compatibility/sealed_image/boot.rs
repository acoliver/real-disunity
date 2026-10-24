use std::collections::{BTreeMap, BTreeSet};
use std::mem::ManuallyDrop;

use super::super::{CompatibilityBridge, CompatibilityHost, NativeTransport, handshake};
use super::NativeImageLease;
use super::process::Attachment;
use crate::{HostClaim, HostError, ManagedHostConfig, load_ffi_image};

impl CompatibilityHost {
    /// Attaches Rust and CLR to the same verified Linux sealed descriptor.
    /// `config.ffi_library` is ignored only for this explicit lease entry point.
    /// The first successfully loaded image and its descriptor remain owned through
    /// process exit, including binding or negotiation errors. Subsequent attachments
    /// must identify that same sealed inode.
    /// Managed artifacts and code must still be selected by a trusted caller.
    pub fn boot_sealed_native(
        config: &ManagedHostConfig,
        lease: NativeImageLease,
    ) -> Result<Self, HostError> {
        config.validate_managed()?;
        let claim = HostClaim::acquire()?;
        let attachment = Attachment::select(lease)?;
        let selected = ManagedHostConfig {
            ffi_library: attachment.path.clone(),
            ..config.clone()
        };
        // Keep the authenticated procfd stable even when binding or negotiation fails.
        // NODELETE mappings can otherwise outlive the descriptor and its pathname.
        let path = attachment.path.clone();
        attachment.retain_before_configuration(&selected)?;
        let library = load_ffi_image(&selected)?;
        let native = NativeTransport::bind(&library)?;
        let description = native.request(0, &serde_json::json!({"version":1,"op":"describe"}))?;
        handshake::validate_native(&description)?;
        let bridge = CompatibilityBridge::bind(config)?;
        let negotiation = handshake::Negotiation::new(description, bridge.description()?)?;
        let bytes = path.to_string_lossy();
        // SAFETY: the trusted Cdecl bridge consumes this UTF-8 path synchronously;
        // its descriptor and native mapping are now retained through process exit.
        bridge.check(unsafe { (bridge.configure)(bytes.as_ptr(), bytes.len()) })?;
        Ok(Self {
            bridge,
            native: ManuallyDrop::new(native),
            negotiation: ManuallyDrop::new(negotiation),
            worlds: ManuallyDrop::new(BTreeSet::new()),
            loaded: false,
            loaded_world: None,
            strict: ManuallyDrop::new(BTreeMap::new()),
            _library: ManuallyDrop::new(library),
            _claim: ManuallyDrop::new(claim),
            _thread_affinity: std::marker::PhantomData,
        })
    }
}
