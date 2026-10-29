//! Allocation preflight and atomic replacement at a queue admission boundary.
use crate::submission::{SubmissionAuthority, SubmitError};
mod errors;
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
    Buffer(u64),
    Readback {
        size: [u32; 2],
        texel_bytes: u32,
    },
    Cpu(u64),
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct Allocation {
    name: &'static str,
    storage: Storage,
    count: u32,
    shared: bool,
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
        });
    }
    pub fn shared_buffer(&mut self, name: &'static str, bytes: u64, count: u32) {
        self.0.push(Allocation {
            name,
            storage: Storage::Buffer(bytes),
            count,
            shared: true,
        });
    }
    pub fn readback(&mut self, name: &'static str, size: [u32; 2], texel_bytes: u32, count: u32) {
        self.0.push(Allocation {
            name,
            storage: Storage::Readback { size, texel_bytes },
            count,
            shared: false,
        });
    }
    pub fn cpu(&mut self, name: &'static str, bytes: u64) {
        self.0.push(Allocation {
            name,
            storage: Storage::Cpu(bytes),
            count: 1,
            shared: false,
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
        Storage::Texture {
            extent,
            format,
            mips,
            samples,
        } => texture_bytes(extent, format, mips, samples, limits)?,
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
    // Backends may emulate stencil with D32/S8 and promote D24/S8 to D32/S8.
    // Copy payload sizes omit that backing storage. Use the largest supported backing.
    let block = match format {
        wgpu::TextureFormat::Stencil8
        | wgpu::TextureFormat::Depth24PlusStencil8
        | wgpu::TextureFormat::Depth32FloatStencil8 => 8,
        wgpu::TextureFormat::Depth24Plus => 4,
        _ => format
            .block_copy_size(None)
            .ok_or("unsupported texture byte size")?,
    };
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
fn total<'a>(
    sets: impl IntoIterator<Item = &'a AllocationSet>,
    limits: &wgpu::Limits,
) -> Result<u64, &'static str> {
    let mut shared: Vec<&Allocation> = Vec::new();
    let mut bytes = 0;
    for entry in sets.into_iter().flat_map(|set| &set.0) {
        let size = allocation_bytes(entry, limits)?;
        if entry.shared {
            if let Some(previous) = shared.iter().find(|previous| previous.name == entry.name) {
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
    authority: SubmissionAuthority,
}
impl<T> Configuration<T> {
    pub fn new(
        authority: &SubmissionAuthority,
        current: T,
        allocations: AllocationSet,
        limits: &wgpu::Limits,
        budget: AllocationBudget,
    ) -> Result<Self, ConfigurationError<std::convert::Infallible>> {
        let required = allocations
            .validate(limits)
            .map_err(ConfigurationError::Invalid)?;
        if required > budget.0 {
            return Err(ConfigurationError::Budget {
                required,
                budget: budget.0,
            });
        }
        Ok(Self {
            current,
            allocations,
            retired: Vec::new(),
            generation: 0,
            budget,
            authority: authority.clone(),
        })
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
    pub fn set_budget(&mut self, budget: AllocationBudget) {
        self.budget = budget;
    }
    pub fn collect(&mut self) {
        let complete = self.authority.completed_serial();
        self.retired.retain(|old| {
            let _ = &old.value;
            old.last_submission > complete
        });
    }
    pub fn resident_bytes(&self, limits: &wgpu::Limits) -> Result<u64, &'static str> {
        total(
            std::iter::once(&self.allocations)
                .chain(self.retired.iter().map(|old| &old.allocations)),
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
        let required = total(
            std::iter::once(&allocations)
                .chain(std::iter::once(&self.allocations))
                .chain(self.retired.iter().map(|old| &old.allocations)),
            limits,
        )
        .map_err(ConfigurationError::Invalid)?;
        if required > self.budget.0 {
            return Err(ConfigurationError::Budget {
                required,
                budget: self.budget.0,
            });
        }
        let replacement = allocate();
        self.authority.check_device()?;
        let replacement = replacement.map_err(ConfigurationError::Allocation)?;
        let old = Generation {
            value: std::mem::replace(&mut self.current, replacement),
            allocations: std::mem::replace(&mut self.allocations, allocations),
            last_submission: self.authority.submitted_serial(),
        };
        self.retired.push(old);
        self.generation += 1;
        self.collect();
        Ok(())
    }
}
