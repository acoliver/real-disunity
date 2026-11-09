//! How many bytes of GPU storage the headless player lets the program runtime hold.
//!
//! wgpu 30 reports no memory size for an adapter: `AdapterInfo` has no heap or VRAM size, and
//! `Limits` only bounds one buffer, which says nothing about the total. So the budget is chosen
//! here, in this order:
//!
//! 1. `PLAY_GPU_STORAGE_MB`, a whole number of MiB, when it is set.
//! 2. For a software adapter (`DeviceType::Cpu`, such as lavapipe), whose textures live in the
//!    host's memory, a quarter of the memory the host reports available, no less than the default
//!    and no more than [`CPU_CEILING`].
//! 3. [`DEFAULT_BYTES`] for every other adapter.
//!
//! The budget is not a buffer limit. The runtime reserves its arena in buffers the device can
//! hold, so a budget may exceed `max_buffer_size`.

use wgpu::DeviceType;

/// The environment variable that sets the budget, in MiB.
pub const STORAGE_BUDGET_ENV: &str = "PLAY_GPU_STORAGE_MB";

/// The budget of an adapter with no better source.
pub const DEFAULT_BYTES: u64 = 512 << 20;

/// The most a software adapter's budget is derived to, however much memory the host has.
const CPU_CEILING: u64 = 2 << 30;

/// A chosen budget and where it came from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Choice {
    pub bytes: u64,
    pub source: &'static str,
}

/// Chooses the budget for an adapter of `device_type` when the host has `host_available` bytes of
/// memory free and the environment variable holds `env`.
#[must_use]
pub fn choose(env: Option<&str>, device_type: DeviceType, host_available: Option<u64>) -> Choice {
    let mut source = "the default";
    if let Some(value) = env {
        let mebibytes = value.parse::<u64>().ok().filter(|mebibytes| *mebibytes > 0);
        if let Some(bytes) = mebibytes.and_then(|mebibytes| mebibytes.checked_mul(1 << 20)) {
            return Choice {
                bytes,
                source: STORAGE_BUDGET_ENV,
            };
        }
        source = "the default; PLAY_GPU_STORAGE_MB is ignored, not a positive number of MiB";
    }
    if let (DeviceType::Cpu, Some(available)) = (device_type, host_available) {
        return Choice {
            bytes: (available / 4).clamp(DEFAULT_BYTES, CPU_CEILING),
            source: "a quarter of the host memory available, for a software adapter",
        };
    }
    Choice {
        bytes: DEFAULT_BYTES,
        source,
    }
}

/// The bytes of memory the host reports available, where the platform says.
#[must_use]
pub fn host_available_bytes() -> Option<u64> {
    parse_meminfo(&std::fs::read_to_string("/proc/meminfo").ok()?)
}

/// The `MemAvailable` line of Linux's `/proc/meminfo`, which counts kibibytes.
fn parse_meminfo(text: &str) -> Option<u64> {
    let line = text
        .lines()
        .find(|line| line.starts_with("MemAvailable:"))?;
    let kibibytes: u64 = line
        .trim_start_matches("MemAvailable:")
        .trim()
        .trim_end_matches("kB")
        .trim()
        .parse()
        .ok()?;
    kibibytes.checked_mul(1024)
}

#[cfg(test)]
mod tests {
    use super::*;

    const MIB: u64 = 1 << 20;
    const GIB: u64 = 1 << 30;

    fn bytes(env: Option<&str>, device: DeviceType, host: Option<u64>) -> u64 {
        choose(env, device, host).bytes
    }

    #[test]
    fn the_environment_names_the_budget_in_mebibytes_whatever_the_adapter() {
        for device in [DeviceType::Cpu, DeviceType::DiscreteGpu] {
            assert_eq!(bytes(Some("1536"), device, Some(64 * GIB)), 1536 * MIB);
        }
        assert_eq!(
            choose(Some("96"), DeviceType::Cpu, None).source,
            STORAGE_BUDGET_ENV
        );
    }

    #[test]
    fn an_environment_value_that_is_not_a_positive_whole_number_is_ignored_and_said_so() {
        for bad in ["", "0", "-4", "1.5", "lots", "99999999999999999999"] {
            let choice = choose(Some(bad), DeviceType::DiscreteGpu, Some(64 * GIB));
            assert_eq!(choice.bytes, DEFAULT_BYTES, "{bad:?}");
            assert!(
                choice.source.contains("ignored"),
                "{bad:?}: {}",
                choice.source
            );
        }
    }

    #[test]
    fn a_software_adapter_gets_a_quarter_of_the_host_memory_between_the_default_and_the_ceiling() {
        assert_eq!(bytes(None, DeviceType::Cpu, Some(6 * GIB)), 1536 * MIB);
        assert_eq!(bytes(None, DeviceType::Cpu, Some(64 * GIB)), 2 * GIB);
        assert_eq!(bytes(None, DeviceType::Cpu, Some(GIB)), DEFAULT_BYTES);
        assert_eq!(bytes(None, DeviceType::Cpu, None), DEFAULT_BYTES);
    }

    #[test]
    fn a_hardware_adapter_keeps_the_default_because_the_host_memory_is_not_its_memory() {
        for device in [
            DeviceType::DiscreteGpu,
            DeviceType::IntegratedGpu,
            DeviceType::VirtualGpu,
            DeviceType::Other,
        ] {
            assert_eq!(bytes(None, device, Some(64 * GIB)), DEFAULT_BYTES);
        }
    }

    #[test]
    fn available_memory_is_the_mem_available_line_of_meminfo_in_bytes() {
        let text = "MemTotal:       32000000 kB\nMemFree:         1000000 kB\nMemAvailable:    7340032 kB\nBuffers: 4 kB\n";
        assert_eq!(parse_meminfo(text), Some(7_340_032 * 1024));
        assert_eq!(parse_meminfo("MemTotal: 1 kB\n"), None);
        assert_eq!(parse_meminfo("MemAvailable: lots kB\n"), None);
    }
}
