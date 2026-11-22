//! Allocation preflight and atomic replacement at a queue admission boundary.
use crate::submission::{SubmissionAuthority, SubmitError};
mod budget;
mod reservation;
pub use reservation::AllocationReservation;
mod errors;
pub use budget::AllocationBudgetPolicy;
pub use errors::{AllocationScope, DriverAllocationError};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AllocationBudget(pub u64);

#[derive(Debug, thiserror::Error)]
pub enum ConfigurationError<E> {
    #[error("configuration requires {required} bytes, budget is {budget}")]
    Budget { required: u64, budget: u64 },
    #[error("invalid allocation: {0}")]
    Invalid(&'static str),
    /// A texture array cannot fit on the requested device, before allocation.
    #[error("texture array requires {required} layers, device provides {available}")]
    TextureArrayLayers {
        /// Physical array layers required by the allocation policy.
        required: u64,
        /// The actual device limit, which may be below the adapter capability.
        available: u32,
    },
    #[error("configuration belongs to a different device")]
    ForeignDevice,
    #[error("replacement allocation failed: {0}")]
    Allocation(E),
    #[error(transparent)]
    Submission(#[from] SubmitError),
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Storage {
    Texture {
        extent: [u32; 3],
        format: wgpu::TextureFormat,
        mips: u32,
        samples: u32,
    },
    /// A 3D texture, whose levels each halve the depth as well as the width and height.
    Volume {
        extent: [u32; 3],
        format: wgpu::TextureFormat,
        mips: u32,
    },
    Buffer(u64),
    Readback {
        size: [u32; 2],
        texel_bytes: u32,
    },
    Cpu(u64),
    /// GPU storage its owner created and checked against the device itself, by bytes alone.
    External(u64),
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct Allocation {
    name: &'static str,
    storage: Storage,
    count: u32,
    shared: bool,
    /// Tells apart the retained backings that one name describes; zero for every other entry.
    backing: u64,
}
/// Each entry is one physical allocation family. Cloned views are not entries.
/// Shared entries must describe the same retained storage in every generation.
#[derive(Clone, Debug, Default)]
pub struct AllocationSet(Vec<Allocation>);
impl AllocationSet {
    pub fn texture(
        &mut self,
        name: &'static str,
        extent: [u32; 3],
        format: wgpu::TextureFormat,
        mip_samples: [u32; 2],
        count: u32,
    ) {
        let [mips, samples] = mip_samples;
        self.0.push(Allocation {
            name,
            storage: Storage::Texture {
                extent,
                format,
                mips,
                samples,
            },
            count,
            shared: false,
            backing: 0,
        });
    }
    /// Declares a 3D texture of `extent` (width, height, depth) with `mips` levels.
    pub fn volume(
        &mut self,
        name: &'static str,
        extent: [u32; 3],
        format: wgpu::TextureFormat,
        mips: u32,
    ) {
        self.0.push(Allocation {
            name,
            storage: Storage::Volume {
                extent,
                format,
                mips,
            },
            count: 1,
            shared: false,
            backing: 0,
        });
    }
    /// Declares retained texture storage, rather than cloned views or new backing.
    pub fn shared_texture(
        &mut self,
        name: &'static str,
        extent: [u32; 3],
        format: wgpu::TextureFormat,
        mip_samples: [u32; 2],
        count: u32,
    ) {
        self.texture(name, extent, format, mip_samples, count);
        self.0.last_mut().unwrap().shared = true;
    }
    pub fn buffer(&mut self, name: &'static str, bytes: u64, count: u32) {
        self.0.push(Allocation {
            name,
            storage: Storage::Buffer(bytes),
            count,
            shared: false,
            backing: 0,
        });
    }
    /// Declares one physical texture that every generation which keeps it describes the same
    /// way. `backing` identifies the physical texture among those `name` describes over time:
    /// entries with one name and one `backing` are the same storage and count once, whichever
    /// generations declare them, and entries with one name and different backings are distinct
    /// storage that both count while both are live.
    pub fn backing_texture(
        &mut self,
        name: &'static str,
        backing: u64,
        extent: [u32; 3],
        format: wgpu::TextureFormat,
        mip_samples: [u32; 2],
    ) {
        self.shared_texture(name, extent, format, mip_samples, 1);
        self.0.last_mut().unwrap().backing = backing;
    }
    pub fn shared_buffer(&mut self, name: &'static str, bytes: u64, count: u32) {
        self.0.push(Allocation {
            name,
            storage: Storage::Buffer(bytes),
            count,
            shared: true,
            backing: 0,
        });
    }
    pub fn readback(&mut self, name: &'static str, size: [u32; 2], texel_bytes: u32, count: u32) {
        self.0.push(Allocation {
            name,
            storage: Storage::Readback { size, texel_bytes },
            count,
            shared: false,
            backing: 0,
        });
    }
    pub fn cpu(&mut self, name: &'static str, bytes: u64) {
        self.0.push(Allocation {
            name,
            storage: Storage::Cpu(bytes),
            count: 1,
            shared: false,
            backing: 0,
        });
    }
    /// Declares one host-owned CPU reservation retained across generations.
    pub fn shared_cpu(&mut self, name: &'static str, bytes: u64) {
        self.cpu(name, bytes);
        self.0.last_mut().unwrap().shared = true;
    }
    /// Declares `bytes` of GPU storage that its owner creates outside the set's rules (a
    /// texture of any extent or format, for example) and has already checked against the
    /// device. It counts against the budget like any GPU entry, but no buffer or texture rule
    /// applies to it.
    pub fn external(&mut self, name: &'static str, bytes: u64) {
        self.0.push(Allocation {
            name,
            storage: Storage::External(bytes),
            count: 1,
            shared: false,
            backing: 0,
        });
    }
    pub fn extend(&mut self, other: Self) {
        self.0.extend(other.0);
    }
    /// Checks each physical array independently of allocation multiplicity and byte budgets.
    pub fn validate_array_layers<E>(
        &self,
        limits: &wgpu::Limits,
    ) -> Result<(), ConfigurationError<E>> {
        for entry in &self.0 {
            if let Storage::Texture { extent, .. } = entry.storage
                && extent[2] > limits.max_texture_array_layers
            {
                return Err(ConfigurationError::TextureArrayLayers {
                    required: u64::from(extent[2]),
                    available: limits.max_texture_array_layers,
                });
            }
        }
        Ok(())
    }
    /// Checks descriptor bytes without allocating CPU or GPU backing storage.
    pub fn validate_budget<E>(
        &self,
        limits: &wgpu::Limits,
        budget: AllocationBudget,
    ) -> Result<u64, ConfigurationError<E>> {
        let required = self.validate(limits).map_err(ConfigurationError::Invalid)?;
        if required > budget.0 {
            return Err(ConfigurationError::Budget {
                required,
                budget: budget.0,
            });
        }
        Ok(required)
    }
    pub fn validate(&self, limits: &wgpu::Limits) -> Result<u64, &'static str> {
        total([self], limits)
    }
}
fn add(a: u64, b: u64) -> Result<u64, &'static str> {
    a.checked_add(b).ok_or("allocation byte overflow")
}
fn mul(a: u64, b: u64) -> Result<u64, &'static str> {
    a.checked_mul(b).ok_or("allocation byte overflow")
}
fn allocation_bytes(entry: &Allocation, limits: &wgpu::Limits) -> Result<u64, &'static str> {
    if entry.count == 0 {
        return Err("zero allocation multiplicity");
    }
    let bytes = match entry.storage {
        Storage::Buffer(bytes) => {
            if bytes == 0
                || bytes > limits.max_buffer_size
                || bytes % wgpu::COPY_BUFFER_ALIGNMENT != 0
            {
                return Err("buffer size or alignment");
            }
            bytes
        }
        Storage::Readback { size, texel_bytes } => {
            if size.contains(&0) || texel_bytes == 0 {
                return Err("empty readback");
            }
            let row = mul(u64::from(size[0]), u64::from(texel_bytes))?;
            let row = mul(add(row, 255)? / 256, 256)?;
            let bytes = mul(row, u64::from(size[1]))?;
            if bytes > limits.max_buffer_size {
                return Err("readback buffer limit");
            }
            bytes
        }
        Storage::Cpu(bytes) => bytes,
        Storage::External(bytes) => {
            if bytes == 0 {
                return Err("empty external storage");
            }
            bytes
        }
        Storage::Texture {
            extent,
            format,
            mips,
            samples,
        } => texture_bytes(extent, format, mips, samples, limits)?,
        Storage::Volume {
            extent,
            format,
            mips,
        } => volume_bytes(extent, format, mips, limits)?,
    };
    mul(bytes, u64::from(entry.count))
}
fn texture_bytes(
    extent: [u32; 3],
    format: wgpu::TextureFormat,
    mips: u32,
    samples: u32,
    limits: &wgpu::Limits,
) -> Result<u64, &'static str> {
    if extent.contains(&0)
        || extent[0] > limits.max_texture_dimension_2d
        || extent[1] > limits.max_texture_dimension_2d
        || extent[2] > limits.max_texture_array_layers
    {
        return Err("texture extent limit");
    }
    if mips == 0
        || mips > 32 - extent[0].max(extent[1]).leading_zeros()
        || !matches!(samples, 1 | 2 | 4 | 8 | 16)
        || (samples != 1 && mips != 1)
    {
        return Err("texture mip or sample count");
    }
    let (bw, bh) = format.block_dimensions();
    let block = backing_block_bytes(format).ok_or("unsupported texture byte size")?;
    let mut bytes = 0;
    for mip in 0..mips {
        let blocks = mul(
            u64::from((extent[0] >> mip).max(1).div_ceil(bw)),
            u64::from((extent[1] >> mip).max(1).div_ceil(bh)),
        )?;
        bytes = add(bytes, mul(blocks, u64::from(block))?)?;
    }
    mul(mul(bytes, u64::from(extent[2]))?, u64::from(samples))
}
fn volume_bytes(
    extent: [u32; 3],
    format: wgpu::TextureFormat,
    mips: u32,
    limits: &wgpu::Limits,
) -> Result<u64, &'static str> {
    if extent.contains(&0)
        || extent
            .iter()
            .any(|&side| side > limits.max_texture_dimension_3d)
    {
        return Err("volume extent limit");
    }
    if mips == 0 || mips > 32 - extent.into_iter().max().unwrap_or(1).leading_zeros() {
        return Err("volume mip count");
    }
    let (bw, bh) = format.block_dimensions();
    let block = backing_block_bytes(format).ok_or("unsupported texture byte size")?;
    let mut bytes = 0;
    for mip in 0..mips {
        let blocks = mul(
            mul(
                u64::from((extent[0] >> mip).max(1).div_ceil(bw)),
                u64::from((extent[1] >> mip).max(1).div_ceil(bh)),
            )?,
            u64::from((extent[2] >> mip).max(1)),
        )?;
        bytes = add(bytes, mul(blocks, u64::from(block))?)?;
    }
    Ok(bytes)
}
/// Bytes per texel block of the storage a backend allocates for `format`.
///
/// Backends may emulate stencil with D32/S8 and promote D24/S8 to D32/S8. Copy payload sizes
/// omit that backing storage, so this uses the largest supported backing. `None` for formats
/// without a single-plane copy size.
pub(crate) fn backing_block_bytes(format: wgpu::TextureFormat) -> Option<u32> {
    match format {
        wgpu::TextureFormat::Stencil8
        | wgpu::TextureFormat::Depth24PlusStencil8
        | wgpu::TextureFormat::Depth32FloatStencil8 => Some(8),
        wgpu::TextureFormat::Depth24Plus => Some(4),
        _ => format.block_copy_size(None),
    }
}
fn total<'a>(
    sets: impl IntoIterator<Item = &'a AllocationSet>,
    limits: &wgpu::Limits,
) -> Result<u64, &'static str> {
    total_where(sets, limits, |_| true)
}
/// [`total`] of the entries that live on the GPU, without CPU-side storage.
fn gpu_total<'a>(
    sets: impl IntoIterator<Item = &'a AllocationSet>,
    limits: &wgpu::Limits,
) -> Result<u64, &'static str> {
    total_where(sets, limits, |entry| {
        !matches!(entry.storage, Storage::Cpu(_))
    })
}
fn total_where<'a>(
    sets: impl IntoIterator<Item = &'a AllocationSet>,
    limits: &wgpu::Limits,
    keep: impl Fn(&Allocation) -> bool,
) -> Result<u64, &'static str> {
    let mut shared: Vec<&Allocation> = Vec::new();
    let mut bytes = 0;
    for entry in sets
        .into_iter()
        .flat_map(|set| &set.0)
        .filter(|entry| keep(entry))
    {
        let size = allocation_bytes(entry, limits)?;
        if entry.shared {
            if let Some(previous) = shared
                .iter()
                .find(|previous| previous.name == entry.name && previous.backing == entry.backing)
            {
                if **previous != *entry {
                    return Err("shared allocation descriptor changed");
                }
                continue;
            }
            shared.push(entry);
        }
        bytes = add(bytes, size)?;
    }
    Ok(bytes)
}
/// The census entry of a counted configuration and the limits its sizes are taken under.
struct Counted {
    limits: wgpu::Limits,
    entry: crate::census::CensusEntry,
}
struct Generation<T> {
    value: T,
    allocations: AllocationSet,
    last_submission: u64,
}
/// Keeps complete old generations alive until all preceding queue work completes.
/// The allocator must construct private replacement objects without publishing edits.
pub struct Configuration<T> {
    current: T,
    allocations: AllocationSet,
    retired: Vec<Generation<T>>,
    generation: u64,
    budget: AllocationBudget,
    policy: AllocationBudgetPolicy,
    authority: SubmissionAuthority,
    reservations: Vec<std::sync::Weak<AllocationSet>>,
    census: Option<Counted>,
}
impl<T> Configuration<T> {
    /// `policy` is a fixed [`AllocationBudget`] or a policy that resolves one from the
    /// required bytes.
    pub fn new(
        authority: &SubmissionAuthority,
        current: T,
        allocations: AllocationSet,
        limits: &wgpu::Limits,
        policy: impl Into<AllocationBudgetPolicy>,
    ) -> Result<Self, ConfigurationError<std::convert::Infallible>> {
        let policy = policy.into();
        let required = allocations
            .validate(limits)
            .map_err(ConfigurationError::Invalid)?;
        let budget = policy.resolve(required)?;
        Ok(Self {
            current,
            allocations,
            reservations: Vec::new(),
            retired: Vec::new(),
            generation: 0,
            budget,
            policy,
            authority: authority.clone(),
            census: None,
        })
    }
    /// Counts the GPU bytes of the current and retired generations' admitted allocation sets
    /// in the authority's census under `class`, until each generation retires. Reservations
    /// are not counted: what they reserve is counted where it is created.
    #[must_use]
    pub fn counted(mut self, class: crate::census::ResourceClass, limits: &wgpu::Limits) -> Self {
        self.census = Some(Counted {
            limits: limits.clone(),
            entry: self.authority.census().track(class, 0),
        });
        self.recount();
        self
    }
    fn recount(&mut self) {
        let Some(counted) = &self.census else {
            return;
        };
        let sets = std::iter::once(&self.allocations)
            .chain(self.retired.iter().map(|old| &old.allocations));
        // Admission already validated every set, so a size error cannot occur here.
        let bytes = gpu_total(sets, &counted.limits).unwrap_or(0);
        let entry = self.authority.census().track(counted.entry.class(), bytes);
        if let Some(counted) = &mut self.census {
            counted.entry = entry;
        }
    }
    pub fn uses_authority(&self, authority: &SubmissionAuthority) -> bool {
        self.authority.same_authority(authority)
    }
    pub fn current(&self) -> &T {
        &self.current
    }
    pub fn current_mut(&mut self) -> &mut T {
        &mut self.current
    }
    pub fn generation(&self) -> u64 {
        self.generation
    }
    pub fn retired_count(&self) -> usize {
        self.retired.len()
    }
    pub fn budget(&self) -> AllocationBudget {
        self.budget
    }
    pub fn collect(&mut self) {
        self.reservations.retain(|entry| entry.strong_count() != 0);
        let complete = self.authority.completed_serial();
        self.retired.retain(|old| {
            let _ = &old.value;
            old.last_submission > complete
        });
        self.recount();
    }
    /// [`Self::collect`] after the completion bookkeeping of every retired generation's last
    /// submission has run, so a caller that waited for them is not charged for memory that is
    /// already free (hub issue 4d2b47c55165). Polls without blocking until then or until
    /// `deadline`.
    ///
    /// # Errors
    /// Fails with the poll error, `PollError::Timeout` past `deadline`; generations that are
    /// still in flight then stay counted.
    pub fn collect_settled(
        &mut self,
        device: &wgpu::Device,
        deadline: std::time::Instant,
    ) -> Result<(), wgpu::PollError> {
        let last = self.retired.iter().map(|old| old.last_submission).max();
        let settled = last.map_or(Ok(()), |serial| {
            self.authority
                .await_completion_record(device, serial, deadline)
        });
        self.collect();
        settled
    }
    pub fn resident_bytes(&self, limits: &wgpu::Limits) -> Result<u64, &'static str> {
        let reservations: Vec<_> = self
            .reservations
            .iter()
            .filter_map(std::sync::Weak::upgrade)
            .collect();
        total(
            std::iter::once(&self.allocations)
                .chain(self.retired.iter().map(|old| &old.allocations))
                .chain(reservations.iter().map(AsRef::as_ref)),
            limits,
        )
    }
    pub fn replace<E>(
        &mut self,
        limits: &wgpu::Limits,
        allocations: AllocationSet,
        allocate: impl FnOnce() -> Result<T, E>,
    ) -> Result<(), ConfigurationError<E>> {
        let _guard = self.authority.try_replacement()?;
        self.collect();
        let reservations: Vec<_> = self
            .reservations
            .iter()
            .filter_map(std::sync::Weak::upgrade)
            .collect();
        let required = total(
            std::iter::once(&allocations)
                .chain(std::iter::once(&self.allocations))
                .chain(self.retired.iter().map(|old| &old.allocations))
                .chain(reservations.iter().map(AsRef::as_ref)),
            limits,
        )
        .map_err(ConfigurationError::Invalid)?;
        let budget = self.policy.resolve(required)?;
        let replacement = allocate();
        self.authority.check_device()?;
        let replacement = replacement.map_err(ConfigurationError::Allocation)?;
        let old = Generation {
            value: std::mem::replace(&mut self.current, replacement),
            allocations: std::mem::replace(&mut self.allocations, allocations),
            last_submission: self.authority.submitted_serial(),
        };
        self.budget = budget;
        self.retired.push(old);
        self.generation += 1;
        self.collect();
        Ok(())
    }
}
