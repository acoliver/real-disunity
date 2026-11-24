//! The walk over a snapshot's work: each command in call order becomes the records it stands for,
//! or is placed (a dispatch, a pixel read) or is counted as left out.

use std::collections::BTreeSet;

use worldrunner_render_program::ids::TextureId;
use worldrunner_render_program::unity::offscreen::frame::Record;
use worldrunner_world_simulation::graphics::{ComputeBindings, OffscreenWork, TextureRef};

use super::frame::{Mapper, Placed, is_scratch_render};
use super::ids;
use super::pixels::{self, Wanted};
use super::script::kind;

impl Mapper<'_> {
    /// Maps every command of the snapshot, then generates the mips of the flagged textures that
    /// are still waiting for them.
    pub(super) fn records(&mut self) {
        for (index, work) in self.snapshot.work.iter().enumerate() {
            if let OffscreenWork::Dispatch { bindings, .. } = work {
                self.dispatch(index, bindings);
                continue;
            }
            if let Some(wanted) = Wanted::of(work) {
                self.pixel_read(index, wanted);
                continue;
            }
            match self.record(index, work) {
                Ok(records) => {
                    *self
                        .report
                        .built_by_kind
                        .entry(kind(work).to_owned())
                        .or_default() += 1;
                    let records = self.with_auto_mips(records);
                    self.report.built += records
                        .iter()
                        .filter(|record| !is_scratch_render(record))
                        .count();
                    if !records.is_empty() {
                        let globals = self.pending_globals();
                        self.frame.records.extend(globals);
                    }
                    self.frame.records.extend(records);
                }
                Err(skip) => {
                    *self.report.skipped.entry(skip).or_default() += 1;
                    *self
                        .report
                        .skipped_by_kind
                        .entry(format!("{}: {skip:?}", kind(work)))
                        .or_default() += 1;
                }
            }
        }
        let generated = self.mips_at_end();
        self.add_generated(generated);
    }

    /// Adds records the adapter made of its own, which no command of the world listed.
    fn add_generated(&mut self, records: Vec<Record>) {
        self.report.built += records.len();
        self.frame.records.extend(records);
    }

    /// Notes where a dispatch sits and brings the render textures its properties name into the
    /// frame, so they exist when the dispatch binds them. The flagged ones it binds are generated
    /// before it.
    fn dispatch(&mut self, work: usize, bindings: &ComputeBindings) {
        self.report.dispatches += 1;
        let bound: BTreeSet<TextureId> = bindings
            .properties
            .textures
            .iter()
            .filter_map(|(_, slot)| match slot.texture {
                TextureRef::Render(id) => Some(ids::render_texture(id)),
                _ => None,
            })
            .collect();
        let generated = self.mips_for_dispatch(&bound);
        self.add_generated(generated);
        self.placements.push(Placed {
            after: self.frame.records.len(),
            work,
        });
        for (_, slot) in &bindings.properties.textures {
            self.textures.resolve(&slot.texture);
        }
    }

    /// Notes where a pixel read sits and brings its render texture into the frame, so it exists
    /// when the read is made. A texture of integers or one the snapshot lacks is refused here.
    fn pixel_read(&mut self, work: usize, wanted: Wanted) {
        let refusal = match self.snapshot.render_textures.get(&wanted.texture) {
            None => Some(pixels::REFUSED_NO_TEXTURE),
            Some(texture) if pixels::is_integer_format(texture.format) => {
                Some(pixels::REFUSED_INTEGER)
            }
            Some(_) => None,
        };
        if let Some(reason) = refusal {
            *self
                .report
                .pixel_reads_refused
                .entry(reason.to_owned())
                .or_default() += 1;
            return;
        }
        self.textures.resolve(&TextureRef::Render(wanted.texture));
        self.placements.push(Placed {
            after: self.frame.records.len(),
            work,
        });
    }
}
