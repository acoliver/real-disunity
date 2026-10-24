use std::ops::Range;

use super::{AccessMode, BufferUse, BufferUseKind, BufferView, ContractError, ResourceRegistry};

/// Index format and byte slice are retained together, never supplied at encoding time.
#[derive(Clone, Debug)]
pub struct IndexBinding {
    pub view: BufferView,
    pub format: wgpu::IndexFormat,
}

/// Vertex slots correspond exactly to the pipeline's registered vertex layouts.
#[derive(Clone, Debug)]
pub struct DrawGeometry {
    pub vertex_buffers: Vec<BufferView>,
    pub index: Option<IndexBinding>,
    pub elements: Range<u32>,
    pub base_vertex: i32,
    pub instances: Range<u32>,
}

#[derive(Clone, Debug)]
pub(super) struct VertexAbi {
    stride: u64,
    step: wgpu::VertexStepMode,
    end: u64,
}
impl DrawGeometry {
    pub(super) fn register(
        &self,
        device: &wgpu::Device,
        layouts: &[Option<wgpu::VertexBufferLayout<'_>>],
    ) -> Result<Vec<VertexAbi>, ContractError> {
        if self.elements.is_empty()
            || self.instances.is_empty()
            || self.vertex_buffers.len() != layouts.len()
            || layouts.len() > device.limits().max_vertex_buffers as usize
            || (self.index.is_none() && self.base_vertex != 0)
        {
            return Err(ContractError::Pipeline);
        }
        let mut locations = Vec::new();
        let abi = layouts
            .iter()
            .map(|layout| {
                let layout = layout.as_ref().ok_or(ContractError::Pipeline)?;
                let mut end = 0;
                if layout.array_stride > u64::from(device.limits().max_vertex_buffer_array_stride)
                    || !layout.array_stride.is_multiple_of(4)
                {
                    return Err(ContractError::Pipeline);
                }
                for attribute in layout.attributes {
                    if locations.contains(&attribute.shader_location)
                        || attribute.shader_location >= device.limits().max_vertex_attributes
                        || !attribute
                            .offset
                            .is_multiple_of(attribute.format.size().min(4))
                    {
                        return Err(ContractError::Pipeline);
                    }
                    locations.push(attribute.shader_location);
                    end = end.max(
                        attribute
                            .offset
                            .checked_add(attribute.format.size())
                            .ok_or(ContractError::Pipeline)?,
                    );
                }
                if end > u64::from(device.limits().max_vertex_buffer_array_stride)
                    || (layout.array_stride != 0 && end > layout.array_stride)
                {
                    return Err(ContractError::Pipeline);
                }
                Ok(VertexAbi {
                    stride: layout.array_stride,
                    step: layout.step_mode,
                    end,
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        self.check_ranges(&abi)?;
        Ok(abi)
    }
    pub(super) fn check_ranges(&self, abi: &[VertexAbi]) -> Result<(), ContractError> {
        for (view, slot) in self.vertex_buffers.iter().zip(abi) {
            let count = match slot.step {
                wgpu::VertexStepMode::Instance => self.instances.end,
                wgpu::VertexStepMode::Vertex if self.index.is_none() => self.elements.end,
                wgpu::VertexStepMode::Vertex => 1,
            };
            let required = u64::from(count - 1)
                .checked_mul(slot.stride)
                .and_then(|offset| offset.checked_add(slot.end))
                .ok_or(ContractError::Range(view.allocation))?;
            check_slice(view, 4, wgpu::BufferUsages::VERTEX)?;
            if required > view.range.end - view.range.start {
                return Err(ContractError::Range(view.allocation));
            }
        }
        if let Some(index) = &self.index {
            let size = match index.format {
                wgpu::IndexFormat::Uint16 => 2,
                wgpu::IndexFormat::Uint32 => 4,
            };
            check_slice(&index.view, size, wgpu::BufferUsages::INDEX)?;
            if u64::from(self.elements.end) * size > index.view.range.end - index.view.range.start {
                return Err(ContractError::Range(index.view.allocation));
            }
        }
        Ok(())
    }
    pub(super) fn uses(&self) -> Vec<BufferUse> {
        let mut uses: Vec<_> = self
            .vertex_buffers
            .iter()
            .cloned()
            .map(|view| BufferUse::read(view, BufferUseKind::Vertex))
            .collect();
        if let Some(index) = &self.index {
            uses.push(BufferUse::read(index.view.clone(), BufferUseKind::Index));
        }
        uses
    }
    pub(super) fn validate(
        &self,
        registry: &ResourceRegistry,
        declared: &[BufferUse],
    ) -> Result<(), ContractError> {
        for expected in self.uses() {
            registry.check_buffer(&expected.view)?;
            if !declared.iter().any(|usage| {
                usage.view.allocation == expected.view.allocation
                    && usage.view.version == expected.view.version
                    && usage.view.range.start <= expected.view.range.start
                    && usage.view.range.end >= expected.view.range.end
                    && usage.kind == expected.kind
                    && usage.access == AccessMode::Read
            }) {
                return Err(ContractError::Binding(expected.view.allocation));
            }
        }
        Ok(())
    }
    pub(super) fn encode(&self, pass: &mut wgpu::RenderPass<'_>) {
        for (slot, view) in self.vertex_buffers.iter().enumerate() {
            pass.set_vertex_buffer(
                u32::try_from(slot).expect("checked vertex slot"),
                view.buffer.slice(view.range.clone()),
            );
        }
        if let Some(index) = &self.index {
            pass.set_index_buffer(
                index.view.buffer.slice(index.view.range.clone()),
                index.format,
            );
            pass.draw_indexed(
                self.elements.clone(),
                self.base_vertex,
                self.instances.clone(),
            );
        } else {
            pass.draw(self.elements.clone(), self.instances.clone());
        }
    }
}
fn check_slice(
    view: &BufferView,
    alignment: u64,
    usage: wgpu::BufferUsages,
) -> Result<(), ContractError> {
    if !view.range.start.is_multiple_of(alignment)
        || !view.range.end.is_multiple_of(alignment)
        || view.range.start >= view.range.end
        || view.range.end > view.buffer.size()
        || !view.buffer.usage().contains(usage)
    {
        return Err(ContractError::Binding(view.allocation));
    }
    Ok(())
}
