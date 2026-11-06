//! What a flush handler reads and writes besides the work: the world's shaders, materials, render
//! textures, compute shaders and compute buffers, and the global shader state.
use crate::{GpuFlush, HostError, native};

/// A shader: its name and, when it has one, the serialized file and object its programs are in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GpuShader {
    pub name: String,
    pub file: Option<String>,
    pub path_id: i64,
}

/// One kernel of a compute shader. `thread_group_size` is `None` until the compute shader's data
/// or the host states it (see [`GpuFlush::set_kernel_thread_group_size`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GpuKernel {
    pub name: String,
    pub thread_group_size: Option<[u32; 3]>,
}

/// A compute shader, whose kernels are numbered in the order here.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GpuComputeShader {
    pub name: String,
    pub file: String,
    pub path_id: i64,
    pub kernels: Vec<GpuKernel>,
    /// Rises when the host fills in a kernel's group size.
    pub generation: u64,
}

/// A compute buffer of `count` elements of `stride` bytes. `kind` is the engine's buffer type.
/// `generation` rises with every write, the handler's included.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GpuComputeBuffer {
    pub count: u32,
    pub stride: u32,
    pub kind: u32,
    pub generation: u64,
}

/// Which holder of properties and keywords a read addresses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GpuScope {
    /// The material with this id.
    Material(u64),
    /// The global shader state.
    Global,
}

impl GpuScope {
    fn parts(self) -> (u32, u64) {
        match self {
            Self::Material(id) => (0, id),
            Self::Global => (1, 0),
        }
    }
}

fn length(text: &str) -> Result<u32, HostError> {
    u32::try_from(text.len()).map_err(|_| HostError::Managed("a name is too long".into()))
}

impl GpuFlush<'_> {
    /// A shader by id.
    pub fn shader(&self, id: u64) -> Result<GpuShader, HostError> {
        let mut info = native::DisunityShaderInfo::default();
        let probe = unsafe { (self.calls.shader)(self.world, id, &mut info, std::ptr::null_mut()) };
        self.check(probe)?;
        let mut text = vec![0_u8; info.text_capacity as usize];
        let read = unsafe { (self.calls.shader)(self.world, id, &mut info, text.as_mut_ptr()) };
        self.check(read)?;
        let name_end = info.name_length as usize;
        let file_end = name_end + info.file_length as usize;
        let part =
            |range: std::ops::Range<usize>| String::from_utf8_lossy(&text[range]).into_owned();
        Ok(GpuShader {
            name: part(0..name_end),
            file: (info.has_source == 1).then(|| part(name_end..file_end)),
            path_id: info.path_id,
        })
    }

    /// A material's shader, queue, keyword and disabled pass counts and generation.
    pub fn material(&self, id: u64) -> Result<native::DisunityMaterialInfo, HostError> {
        let mut info = native::DisunityMaterialInfo::default();
        let read = unsafe { (self.calls.material)(self.world, id, &mut info) };
        self.check(read)?;
        Ok(info)
    }

    /// A render texture's description and generation.
    pub fn render_texture(&self, id: u64) -> Result<native::DisunityRenderTexture, HostError> {
        let mut texture = native::DisunityRenderTexture::default();
        let read = unsafe { (self.calls.render_texture)(self.world, id, &mut texture) };
        self.check(read)?;
        Ok(texture)
    }

    /// The values a property name holds in a material or in the global shader state, one per
    /// kind.
    pub fn property(
        &self,
        scope: GpuScope,
        name: &str,
    ) -> Result<native::DisunityPropertyRead, HostError> {
        let (scope, target) = scope.parts();
        let mut read = native::DisunityPropertyRead::default();
        let result = unsafe {
            (self.calls.property)(
                self.world,
                scope,
                target,
                name.as_ptr(),
                length(name)?,
                &mut read,
            )
        };
        self.check(result)?;
        Ok(read)
    }

    /// Whether a keyword is enabled on a material or in the global shader state.
    pub fn keyword(&self, scope: GpuScope, name: &str) -> Result<bool, HostError> {
        let (scope, target) = scope.parts();
        let mut enabled = 0_u32;
        let result = unsafe {
            (self.calls.keyword)(
                self.world,
                scope,
                target,
                name.as_ptr(),
                length(name)?,
                &mut enabled,
            )
        };
        self.check(result)?;
        Ok(enabled == 1)
    }

    /// A compute shader by id, with its kernels and the group sizes known so far.
    pub fn compute_shader(&self, id: u64) -> Result<GpuComputeShader, HostError> {
        let mut info = native::DisunityComputeShaderInfo::default();
        let probe = unsafe {
            (self.calls.compute_shader)(
                self.world,
                id,
                &mut info,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        };
        self.check(probe)?;
        let mut kernels =
            vec![native::DisunityComputeKernel::default(); info.kernel_count as usize];
        let mut text = vec![0_u8; info.text_length as usize];
        let read = unsafe {
            (self.calls.compute_shader)(
                self.world,
                id,
                &mut info,
                kernels.as_mut_ptr(),
                text.as_mut_ptr(),
            )
        };
        self.check(read)?;
        let part = |start: u32, count: u32| {
            let range = start as usize..(start as usize + count as usize);
            String::from_utf8_lossy(&text[range]).into_owned()
        };
        Ok(GpuComputeShader {
            name: part(0, info.name_length),
            file: part(info.name_length, info.file_length),
            path_id: info.path_id,
            kernels: kernels
                .iter()
                .map(|kernel| GpuKernel {
                    name: part(kernel.name_offset, kernel.name_length),
                    thread_group_size: (kernel.size_x != 0).then_some([
                        kernel.size_x,
                        kernel.size_y,
                        kernel.size_z,
                    ]),
                })
                .collect(),
            generation: info.generation,
        })
    }

    /// States the thread group size of a kernel the shader's data did not state.
    pub fn set_kernel_thread_group_size(
        &mut self,
        shader: u64,
        kernel: u32,
        size: [u32; 3],
    ) -> Result<(), HostError> {
        let result = unsafe {
            (self.calls.kernel_size)(self.world, shader, kernel, size[0], size[1], size[2])
        };
        self.check(result)
    }

    /// A compute buffer's shape and generation.
    pub fn compute_buffer(&self, id: u64) -> Result<GpuComputeBuffer, HostError> {
        let mut info = native::DisunityComputeBufferInfo::default();
        let read = unsafe { (self.calls.buffer_info)(self.world, id, &mut info) };
        self.check(read)?;
        Ok(GpuComputeBuffer {
            count: info.count,
            stride: info.stride,
            kind: info.kind,
            generation: info.generation,
        })
    }

    /// `length` bytes of a compute buffer's contents from `offset`, as the CPU and earlier
    /// writes of the handler left them.
    pub fn read_compute_buffer(
        &self,
        id: u64,
        offset: u64,
        length: u32,
    ) -> Result<Vec<u8>, HostError> {
        let mut bytes = vec![0_u8; length as usize];
        let read =
            unsafe { (self.calls.buffer_read)(self.world, id, offset, bytes.as_mut_ptr(), length) };
        self.check(read)?;
        Ok(bytes)
    }

    /// Writes bytes into a compute buffer at `offset`, which the managed `GetData` reads once the
    /// flush returns. The range must lie inside the buffer.
    pub fn write_compute_buffer(
        &mut self,
        id: u64,
        offset: u64,
        bytes: &[u8],
    ) -> Result<(), HostError> {
        let count = u32::try_from(bytes.len())
            .map_err(|_| HostError::Managed("a buffer write is at most 4 GiB".into()))?;
        let result =
            unsafe { (self.calls.buffer_write)(self.world, id, offset, bytes.as_ptr(), count) };
        self.check(result)
    }
}
