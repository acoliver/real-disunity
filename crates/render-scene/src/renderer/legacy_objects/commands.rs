use std::ops::Range;

use wgpu::{BindGroup, Buffer, IndexFormat, RenderPipeline};

/// The commands the fixed object path issues into a pass.
pub(crate) trait Commands {
    fn set_pipeline(&mut self, pipeline: &RenderPipeline);
    fn set_bind_group(&mut self, index: u32, group: &BindGroup);
    fn set_vertex_buffer(&mut self, slot: u32, buffer: &Buffer);
    fn set_index_buffer(&mut self, buffer: &Buffer, format: IndexFormat);
    fn draw_indexed(&mut self, indices: Range<u32>, base_vertex: i32, instances: Range<u32>);
}

impl Commands for wgpu::RenderPass<'_> {
    fn set_pipeline(&mut self, pipeline: &RenderPipeline) {
        Self::set_pipeline(self, pipeline);
    }
    fn set_bind_group(&mut self, index: u32, group: &BindGroup) {
        Self::set_bind_group(self, index, group, &[]);
    }
    fn set_vertex_buffer(&mut self, slot: u32, buffer: &Buffer) {
        Self::set_vertex_buffer(self, slot, buffer.slice(..));
    }
    fn set_index_buffer(&mut self, buffer: &Buffer, format: IndexFormat) {
        Self::set_index_buffer(self, buffer.slice(..), format);
    }
    fn draw_indexed(&mut self, indices: Range<u32>, base_vertex: i32, instances: Range<u32>) {
        Self::draw_indexed(self, indices, base_vertex, instances);
    }
}

/// Counts the commands a recording issues, by kind.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Counted {
    pub pipelines: u32,
    pub groups: u32,
    pub vertex_buffers: u32,
    pub index_buffers: u32,
    pub draws: u32,
}

impl Counted {
    pub(crate) const fn total(self) -> u32 {
        self.pipelines + self.groups + self.vertex_buffers + self.index_buffers + self.draws
    }
}

impl Commands for Counted {
    fn set_pipeline(&mut self, _: &RenderPipeline) {
        self.pipelines += 1;
    }
    fn set_bind_group(&mut self, _: u32, _: &BindGroup) {
        self.groups += 1;
    }
    fn set_vertex_buffer(&mut self, _: u32, _: &Buffer) {
        self.vertex_buffers += 1;
    }
    fn set_index_buffer(&mut self, _: &Buffer, _: IndexFormat) {
        self.index_buffers += 1;
    }
    fn draw_indexed(&mut self, _: Range<u32>, _: i32, _: Range<u32>) {
        self.draws += 1;
    }
}
