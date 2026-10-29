//! Identity of the adapter actually selected for rendering.

/// Hardware category reported by wgpu, without name-based guesses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AdapterClass {
    Discrete,
    Integrated,
    Virtual,
    Software,
    Other,
}

impl From<wgpu::DeviceType> for AdapterClass {
    fn from(value: wgpu::DeviceType) -> Self {
        match value {
            wgpu::DeviceType::DiscreteGpu => Self::Discrete,
            wgpu::DeviceType::IntegratedGpu => Self::Integrated,
            wgpu::DeviceType::VirtualGpu => Self::Virtual,
            wgpu::DeviceType::Cpu => Self::Software,
            wgpu::DeviceType::Other => Self::Other,
        }
    }
}

/// Name and classification from one selected adapter.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AdapterIdentity {
    pub name: String,
    pub class: AdapterClass,
}

impl From<wgpu::AdapterInfo> for AdapterIdentity {
    fn from(info: wgpu::AdapterInfo) -> Self {
        Self {
            name: info.name,
            class: info.device_type.into(),
        }
    }
}
