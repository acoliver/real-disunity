//! How many bytes of GPU storage the headless player lets the program runtime hold.
//!
//! wgpu 30 reports no memory size for an adapter: `AdapterInfo` has no heap or VRAM size, and
//! `Limits` only bounds one buffer, which says nothing about the total. One rule sizes the budget
//! for every adapter from the two sizes the platform does publish, with an unknown size counting
//! as none:
//!
//! - the adapter's device-local memory, read by `AdapterMemory::probe` (dedicated VRAM, or an
//!   integrated GPU's carve-out), and
//! - a quarter of the memory the host reports available, which is what a software adapter such as
//!   lavapipe and an integrated GPU, whose textures live in host memory, can use.
//!
//! The budget is the larger of the two, and no less than [`DEFAULT_BYTES`]. An adapter that reports
//! nothing, on a host that reports nothing, gets [`DEFAULT_BYTES`]: the same rule with both sizes
//! unknown, not another path.
//!
//! The budget is not a buffer limit. The runtime reserves its arena in buffers the device can
//! hold, so a budget may exceed `max_buffer_size`.

/// The least a budget is, and the budget when no size is known.
pub const DEFAULT_BYTES: u64 = 512 << 20;

/// A chosen budget and where it came from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Choice {
    pub bytes: u64,
    pub source: &'static str,
}

/// Chooses the budget for an adapter with `device_local` bytes of device-local memory when the
/// host has `host_available` bytes of memory free, either of them where it is known.
#[must_use]
pub fn choose(device_local: Option<u64>, host_available: Option<u64>) -> Choice {
    let adapter = device_local.unwrap_or(0);
    let host_share = host_available.map_or(0, |available| available / 4);
    let (bytes, source) = if adapter >= host_share {
        (adapter, "the adapter's device-local memory")
    } else {
        (host_share, "a quarter of the host memory available")
    };
    if bytes < DEFAULT_BYTES {
        return Choice {
            bytes: DEFAULT_BYTES,
            source: "the default",
        };
    }
    Choice { bytes, source }
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

    fn bytes(device_local: Option<u64>, host_available: Option<u64>) -> u64 {
        choose(device_local, host_available).bytes
    }

    #[test]
    fn an_adapter_with_no_memory_size_gets_a_quarter_of_the_host_memory_available() {
        // Lavapipe: no PCI device to read, and the textures live in the host's memory.
        assert_eq!(bytes(None, Some(28 * GIB)), 7 * GIB);
        assert_eq!(bytes(None, Some(64 * GIB)), 16 * GIB);
        assert_eq!(bytes(None, Some(6 * GIB)), 1536 * MIB);
    }

    #[test]
    fn a_discrete_adapter_gets_its_device_local_memory() {
        assert_eq!(bytes(Some(16 * GIB), Some(32 * GIB)), 16 * GIB);
        assert_eq!(bytes(Some(24 * GIB), None), 24 * GIB);
    }

    #[test]
    fn an_integrated_adapter_with_a_small_carve_out_gets_the_host_share_it_draws_from() {
        // A Radeon 780M reports 2 GiB of VRAM and draws the rest from the host's memory.
        let host_available = 26_806_684 * 1024;
        assert_eq!(
            bytes(Some(2 * GIB), Some(host_available)),
            host_available / 4
        );
        assert!(bytes(Some(2 * GIB), Some(host_available)) >= 6144 * MIB);
    }

    #[test]
    fn no_budget_is_below_the_default_and_nothing_known_gives_the_default() {
        assert_eq!(bytes(None, None), DEFAULT_BYTES);
        assert_eq!(bytes(None, Some(GIB)), DEFAULT_BYTES);
        assert_eq!(bytes(Some(256 * MIB), Some(GIB)), DEFAULT_BYTES);
        assert_eq!(bytes(Some(0), None), DEFAULT_BYTES);
    }

    #[test]
    fn the_choice_names_the_size_it_came_from() {
        assert_eq!(
            choose(Some(16 * GIB), Some(32 * GIB)).source,
            "the adapter's device-local memory"
        );
        assert_eq!(
            choose(None, Some(32 * GIB)).source,
            "a quarter of the host memory available"
        );
        assert_eq!(choose(None, None).source, "the default");
    }

    #[test]
    fn available_memory_is_the_mem_available_line_of_meminfo_in_bytes() {
        let text = "MemTotal:       32000000 kB\nMemFree:         1000000 kB\nMemAvailable:    7340032 kB\nBuffers: 4 kB\n";
        assert_eq!(parse_meminfo(text), Some(7_340_032 * 1024));
        assert_eq!(parse_meminfo("MemTotal: 1 kB\n"), None);
        assert_eq!(parse_meminfo("MemAvailable: lots kB\n"), None);
    }
}
