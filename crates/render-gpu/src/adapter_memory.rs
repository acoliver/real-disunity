//! How much memory the adapter has, read only where the platform publishes it without `unsafe`.
//!
//! wgpu exposes no heap sizes, and reaching the backend through `Adapter::as_hal` would need
//! `unsafe` code, which the workspace forbids. On Linux, DRM drivers that publish `mem_info_*`
//! (amdgpu) give the totals in sysfs under the PCI device that `AdapterInfo::device_pci_bus_id`
//! names. Elsewhere the host may declare the sizes, or they stay unknown.
use std::path::Path;

/// Device memory of one adapter, in bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AdapterMemory {
    /// Device-local memory: dedicated VRAM, or an integrated GPU's carve-out.
    pub device_local: u64,
    /// The part of `device_local` the host can map directly, when the source reports it.
    pub host_visible_device_local: Option<u64>,
    /// System memory the GPU can map (the DRM GTT), when the source reports it.
    pub host_mappable: Option<u64>,
    /// Where the sizes came from.
    pub source: AdapterMemorySource,
}

/// Where an [`AdapterMemory`] reading came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AdapterMemorySource {
    /// The host declared the sizes.
    Declared,
    /// Linux sysfs `mem_info_*` files of the adapter's PCI device.
    LinuxDrm,
}

/// Where Linux publishes PCI devices.
const PCI_DEVICES: &str = "/sys/bus/pci/devices";

impl AdapterMemory {
    /// Sizes declared by the host, for platforms without a readable source or to override one.
    pub fn declared(device_local: u64) -> Self {
        Self {
            device_local,
            host_visible_device_local: None,
            host_mappable: None,
            source: AdapterMemorySource::Declared,
        }
    }

    /// Reads the adapter's memory where this platform publishes it, or `None`.
    pub fn probe(info: &wgpu::AdapterInfo) -> Option<Self> {
        if cfg!(target_os = "linux") {
            Self::from_pci_devices(Path::new(PCI_DEVICES), info)
        } else {
            None
        }
    }

    /// Reads `mem_info_*` from `root/<device_pci_bus_id>`, after checking that the directory's
    /// vendor and device IDs are the adapter's. `None` when the adapter names no bus ID, the
    /// IDs differ, or the driver publishes no VRAM total.
    pub fn from_pci_devices(root: &Path, info: &wgpu::AdapterInfo) -> Option<Self> {
        if info.device_pci_bus_id.is_empty() || info.device_pci_bus_id.contains(['/', '\\']) {
            return None;
        }
        let device = root.join(&info.device_pci_bus_id);
        let id = |name: &str| read_hex(&device.join(name));
        if id("vendor")? != info.vendor || id("device")? != info.device {
            return None;
        }
        let bytes = |name: &str| read_decimal(&device.join(name));
        Some(Self {
            device_local: bytes("mem_info_vram_total")?,
            host_visible_device_local: bytes("mem_info_vis_vram_total"),
            host_mappable: bytes("mem_info_gtt_total"),
            source: AdapterMemorySource::LinuxDrm,
        })
    }
}

fn read_hex(path: &Path) -> Option<u32> {
    let text = std::fs::read_to_string(path).ok()?;
    u32::from_str_radix(text.trim().trim_start_matches("0x"), 16).ok()
}

fn read_decimal(path: &Path) -> Option<u64> {
    std::fs::read_to_string(path).ok()?.trim().parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    const BUS: &str = "0000:c6:00.0";

    fn info(vendor: u32, device: u32, bus: &str) -> wgpu::AdapterInfo {
        wgpu::AdapterInfo {
            name: "fixture".to_owned(),
            vendor,
            device,
            device_type: wgpu::DeviceType::IntegratedGpu,
            device_pci_bus_id: bus.to_owned(),
            driver: String::new(),
            driver_info: String::new(),
            backend: wgpu::Backend::Vulkan,
            subgroup_min_size: 0,
            subgroup_max_size: 0,
            transient_saves_memory: None,
            limit_bucket: None,
        }
    }

    /// A sysfs PCI device with the Radeon 780M's IDs and the files `files` names.
    fn fixture(files: &[(&str, &str)]) -> tempfile::TempDir {
        let root = tempfile::tempdir().unwrap();
        let device = root.path().join(BUS);
        std::fs::create_dir(&device).unwrap();
        for (name, text) in [("vendor", "0x1002\n"), ("device", "0x1900\n")]
            .iter()
            .chain(files)
        {
            std::fs::write(device.join(name), text).unwrap();
        }
        root
    }

    #[test]
    fn a_drm_device_reports_its_vram_visible_vram_and_gtt() {
        let root = fixture(&[
            ("mem_info_vram_total", "2147483648\n"),
            ("mem_info_vis_vram_total", "2147483648\n"),
            ("mem_info_gtt_total", "15651434496\n"),
        ]);
        let memory = AdapterMemory::from_pci_devices(root.path(), &info(0x1002, 0x1900, BUS));
        assert_eq!(
            memory,
            Some(AdapterMemory {
                device_local: 2_147_483_648,
                host_visible_device_local: Some(2_147_483_648),
                host_mappable: Some(15_651_434_496),
                source: AdapterMemorySource::LinuxDrm,
            })
        );
    }

    #[test]
    fn optional_totals_may_be_missing_but_vram_may_not() {
        let root = fixture(&[("mem_info_vram_total", "1024")]);
        let memory = AdapterMemory::from_pci_devices(root.path(), &info(0x1002, 0x1900, BUS));
        assert_eq!(
            memory.map(|m| (m.device_local, m.host_mappable)),
            Some((1024, None))
        );
        let root = fixture(&[("mem_info_gtt_total", "1024")]);
        assert_eq!(
            AdapterMemory::from_pci_devices(root.path(), &info(0x1002, 0x1900, BUS)),
            None
        );
    }

    #[test]
    fn another_device_at_the_bus_id_or_a_missing_bus_id_reads_nothing() {
        let root = fixture(&[("mem_info_vram_total", "1024")]);
        for adapter in [
            info(0x10de, 0x1900, BUS),
            info(0x1002, 0x15bf, BUS),
            info(0x1002, 0x1900, ""),
            info(0x1002, 0x1900, "0000:00:02.0"),
            info(0x1002, 0x1900, "../0000:c6:00.0"),
        ] {
            assert_eq!(AdapterMemory::from_pci_devices(root.path(), &adapter), None);
        }
    }

    #[test]
    fn declared_sizes_name_their_source() {
        let memory = AdapterMemory::declared(4096);
        assert_eq!(memory.device_local, 4096);
        assert_eq!(memory.source, AdapterMemorySource::Declared);
    }
}
