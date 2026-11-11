//! How a player's compute shaders fare as kernels the renderer can dispatch: what the objects
//! hold, whether the SPIR-V platform's modules prepare for naga and write for each backend, whether
//! the declared bindings are the ones the modules decorate, and whether each variant becomes a
//! stage program with a layout and targets. It reports counts only; no module bytes, names or
//! strings leave the decoders.

use std::{
    collections::{BTreeMap, BTreeSet, HashSet},
    fmt,
    path::{Path, PathBuf},
};

use worldrunner_render_program::unity::{
    compute_kernel::{KernelError, kernel_stage},
    layout::ProgramLayout,
    module::{Reflected, StageProgram},
};
use worldrunner_unity_serialized::{
    ClassId,
    compute_shader::{
        ComputeBinding, ComputeBuffer, ComputeShader, ComputeVariant, SPIRV_RENDERER,
    },
};

use crate::{
    PlayerError,
    player_layouts::{layout_cause, module_cause},
    player_module_resources::{ResourceClass, module_resources},
    player_objects::OpenFile,
    player_parameters::Tally,
    shader_prep::{Backend, NagaModule, PrepareError, check_with_naga, prepare},
};

/// Counts from preparing and laying out a player's compute kernels.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ComputeReport {
    /// Compute shader objects that decode.
    pub shaders: usize,
    /// Compute shader objects that do not.
    pub unreadable: usize,
    /// The first object that did not decode, with the reason.
    pub first_unreadable: Option<String>,
    /// Platforms of the decoded shaders by renderer id.
    pub platforms: BTreeMap<u32, usize>,
    /// Kernels of the SPIR-V platforms.
    pub kernels: usize,
    /// Kernels that have a variant for no keywords.
    pub default_variants: Tally,
    pub variants: usize,
    /// Variants whose code is a SPIR-V module, and how many modules are distinct.
    pub modules: usize,
    pub distinct_modules: usize,
    /// Variants whose module `shader_prep::prepare` accepts, and that naga then parses and
    /// validates.
    pub prepared: usize,
    pub ready: usize,
    /// Ready modules in which combined image samplers were split.
    pub split: usize,
    /// Ready modules each backend writes.
    pub written: BTreeMap<Backend, usize>,
    /// Variants whose declared sets and bindings are exactly those the module decorates.
    pub bindings: Tally,
    /// Variants whose slot flags agree with the lists their bindings are in.
    pub slot_flags: Tally,
    /// Ready variants whose thread group size is their entry point's workgroup size.
    pub group_sizes: Tally,
    /// Constant buffer bindings whose layout has the member offsets of the module's block.
    pub constant_buffers: Tally,
    /// Variants that become stage programs, then have a layout built, then have every target
    /// resolve.
    pub programs: usize,
    pub layouts: usize,
    pub targets: usize,
    /// The storage image bindings of the variants that have a layout, by access, format and
    /// view dimension.
    pub images: BTreeMap<String, usize>,
    /// Variants with a layout and every target whose storage images a device made with no
    /// optional features cannot bind, by the first such image's access, format and dimension.
    /// The device in use decides whether they run.
    pub beyond_baseline: BTreeMap<String, usize>,
    pub failures: BTreeMap<String, usize>,
    /// The first failure of each cause, naming its object, kernel and variant by index.
    pub examples: BTreeMap<String, String>,
    seen: HashSet<Vec<u32>>,
}

fn tally(counts: &mut Tally, matched: bool) {
    counts.total += 1;
    counts.matched += usize::from(matched);
}

fn kernel_cause(error: &KernelError) -> String {
    match error {
        KernelError::Code(_) => "code is not a module".to_owned(),
        KernelError::Prepare(error) => error.cause(),
        KernelError::EntryPoints(_) => "not exactly one entry point".to_owned(),
        KernelError::Module(error) => module_cause(error),
        KernelError::NoDimension { .. } => "texture without a dimension".to_owned(),
    }
}

/// The set and binding of every resource a variant declares, with the separate samplers of its
/// textures.
fn declared(variant: &ComputeVariant) -> BTreeSet<(u32, u32)> {
    let places = |list: &[ComputeBinding]| {
        list.iter()
            .map(|b| (u32::from(b.slot.set()), u32::from(b.slot.binding())))
            .collect::<Vec<_>>()
    };
    let mut set: BTreeSet<(u32, u32)> = BTreeSet::new();
    for list in [
        &variant.constant_buffers,
        &variant.textures,
        &variant.buffers,
        &variant.writable,
    ] {
        set.extend(places(list));
    }
    for texture in &variant.textures {
        if let Some(sampler) = texture.sampler {
            set.insert((u32::from(texture.slot.set()), sampler));
        }
    }
    set.extend(variant.samplers.iter().map(|s| (0, s.bind_point)));
    set
}

/// Whether the slot flags of every binding say what its list does: textures and images carry the
/// texture flag, writable buffers and images the writable flag, and only images the image flag.
fn flags_agree(variant: &ComputeVariant) -> bool {
    let plain = |list: &[ComputeBinding]| {
        list.iter()
            .all(|b| !b.slot.is_texture() && !b.slot.is_writable() && !b.slot.is_image())
    };
    let textures = variant
        .textures
        .iter()
        .all(|b| b.slot.is_texture() && !b.slot.is_writable() && !b.slot.is_image());
    let writable = variant
        .writable
        .iter()
        .all(|b| b.slot.is_writable() && b.slot.is_image() == b.dimension.is_some());
    plain(&variant.constant_buffers) && plain(&variant.buffers) && textures && writable
}

/// Whether the variant's thread group size is the workgroup size of the module's one entry point.
fn group_size_agrees(variant: &ComputeVariant, naga: &NagaModule) -> bool {
    matches!(
        naga.module.entry_points.as_slice(),
        [entry] if entry.workgroup_size == variant.thread_group_size
    )
}

/// Whether the module's block at a constant buffer's place has the member offsets its layout has.
fn block_agrees(binding: &ComputeBinding, layouts: &[ComputeBuffer], words: &[u32]) -> bool {
    let at = (
        u32::from(binding.slot.set()),
        u32::from(binding.slot.binding()),
    );
    let block = module_resources(words)
        .into_iter()
        .find(|r| (r.set, r.binding) == at && r.class == ResourceClass::Block);
    let layout = layouts.iter().find(|layout| layout.name == binding.name);
    let (Some(block), Some(layout)) = (block, layout) else {
        return false;
    };
    let mut wanted: Vec<u32> = block.member_offsets;
    wanted.sort_unstable();
    let mut found: Vec<u32> = layout
        .members
        .iter()
        .filter_map(|member| u32::try_from(member.offset).ok())
        .collect();
    found.sort_unstable();
    wanted == found
}

/// Whether every constant buffer, texture and buffer target of the stage resolves.
fn targets_resolve(stage: &StageProgram) -> bool {
    stage.constant_buffer_targets().is_ok()
        && stage.texture_targets().is_ok()
        && stage.buffer_targets().is_ok()
        && stage.image_targets().is_ok()
}

impl ComputeReport {
    fn fail(&mut self, cause: String, detail: &dyn fmt::Display, place: &str) {
        self.examples
            .entry(cause.clone())
            .or_insert_with(|| format!("{place}: {detail}"));
        *self.failures.entry(cause).or_default() += 1;
    }

    fn add_shader(&mut self, shader: &ComputeShader, place: &str) {
        self.shaders += 1;
        for platform in &shader.platforms {
            *self.platforms.entry(platform.renderer).or_default() += 1;
        }
        let Some(platform) = shader.platform(SPIRV_RENDERER) else {
            return;
        };
        for (kernel_index, kernel) in platform.kernels.iter().enumerate() {
            self.kernels += 1;
            tally(
                &mut self.default_variants,
                kernel.variant_for(&[]).is_some(),
            );
            for (index, variant) in kernel.variants.iter().enumerate() {
                let place = format!("{place} kernel {kernel_index} variant {index}");
                self.add_variant(variant, &platform.constant_buffers, &place);
            }
        }
    }

    fn add_variant(&mut self, variant: &ComputeVariant, buffers: &[ComputeBuffer], place: &str) {
        self.variants += 1;
        let words = match variant.module_words() {
            Ok(words) => words,
            Err(error) => return self.fail("code is not a module".to_owned(), &error, place),
        };
        self.modules += 1;
        self.distinct_modules += usize::from(self.seen.insert(words.clone()));
        tally(&mut self.slot_flags, flags_agree(variant));
        self.add_bindings(variant, buffers, &words);
        self.add_preparation(variant, &words, place);
        self.add_program(variant, buffers, place);
    }

    fn add_bindings(&mut self, variant: &ComputeVariant, buffers: &[ComputeBuffer], words: &[u32]) {
        let module: BTreeSet<(u32, u32)> = module_resources(words)
            .iter()
            .map(|r| (r.set, r.binding))
            .collect();
        tally(&mut self.bindings, module == declared(variant));
        for binding in &variant.constant_buffers {
            tally(
                &mut self.constant_buffers,
                block_agrees(binding, buffers, words),
            );
        }
    }

    /// Prepares the module, checks it with naga and writes it for each backend.
    fn add_preparation(&mut self, variant: &ComputeVariant, words: &[u32], place: &str) {
        let checked = prepare(words)
            .map_err(PrepareError::from)
            .and_then(|prepared| {
                let naga = check_with_naga(&prepared.words)?;
                self.prepared += 1;
                Ok((prepared, naga))
            });
        let (prepared, naga) = match checked {
            Ok(checked) => checked,
            Err(error) => return self.fail_prepare(&error, place),
        };
        self.ready += 1;
        tally(&mut self.group_sizes, group_size_agrees(variant, &naga));
        let split = prepared
            .bindings
            .iter()
            .any(|b| b.sampler_binding.is_some());
        self.split += usize::from(split);
        for backend in Backend::ALL {
            match backend.write(&naga) {
                Ok(()) => *self.written.entry(backend).or_default() += 1,
                Err(error) => self.fail_prepare(&error, place),
            }
        }
    }

    fn fail_prepare(&mut self, error: &PrepareError, place: &str) {
        self.fail(error.cause(), error, place);
    }

    fn add_program(&mut self, variant: &ComputeVariant, buffers: &[ComputeBuffer], place: &str) {
        let stage = match kernel_stage(variant, buffers) {
            Ok(stage) => stage,
            Err(error) => return self.fail(kernel_cause(&error), &error, place),
        };
        self.programs += 1;
        match ProgramLayout::build(&[&stage]) {
            Ok(_) => self.layouts += 1,
            Err(error) => return self.fail(layout_cause(&error), &error, place),
        }
        if targets_resolve(&stage) {
            self.targets += 1;
            self.add_images(&stage);
        } else {
            self.fail("targets do not resolve".to_owned(), &"a target", place);
        }
    }
}

impl ComputeReport {
    /// Counts the storage images of a stage that has its layout and targets.
    fn add_images(&mut self, stage: &StageProgram) {
        let images: Vec<_> = stage
            .resources()
            .iter()
            .filter_map(Reflected::storage_image)
            .collect();
        for image in &images {
            *self.images.entry(image.describe()).or_default() += 1;
        }
        if let Some(first) = images.iter().find(|image| image.needs_device_features()) {
            *self.beyond_baseline.entry(first.describe()).or_default() += 1;
        }
    }
}

fn compute_objects(open: &OpenFile) -> Vec<i64> {
    open.file
        .objects()
        .iter()
        .filter(|object| object.class_id == ClassId::COMPUTE_SHADER)
        .map(|object| object.path_id)
        .collect()
}

/// Reads every compute shader of the serialized files in `paths` and counts what becomes of its
/// SPIR-V platform's kernels.
///
/// # Errors
/// Fails if a file cannot be read; a shader that does not decode is counted, not an error.
pub fn compute_report(paths: &[PathBuf]) -> Result<ComputeReport, PlayerError> {
    let mut report = ComputeReport::default();
    for path in paths {
        let mut open = OpenFile::open(path)?;
        for path_id in compute_objects(&open) {
            match open.decode::<ComputeShader>(path_id) {
                Ok(shader) => report.add_shader(&shader, &place(path, path_id)),
                Err(error) => {
                    report.unreadable += 1;
                    report
                        .first_unreadable
                        .get_or_insert_with(|| format!("{}: {error}", place(path, path_id)));
                }
            }
        }
    }
    Ok(report)
}

fn place(path: &Path, path_id: i64) -> String {
    let name = path
        .file_name()
        .map_or_else(String::new, |name| name.to_string_lossy().into_owned());
    format!("{name}:{path_id}")
}

impl fmt::Display for ComputeReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(
            f,
            "{} compute shaders ({} do not decode)",
            self.shaders, self.unreadable
        )?;
        if let Some(first) = &self.first_unreadable {
            writeln!(f, "    first: {first}")?;
        }
        let platforms: Vec<String> = self
            .platforms
            .iter()
            .map(|(renderer, count)| format!("renderer {renderer}: {count}"))
            .collect();
        writeln!(f, "platforms: {}", platforms.join(", "))?;
        writeln!(
            f,
            "SPIR-V platform (renderer {SPIRV_RENDERER}): {} kernels ({} with a variant for no keywords), {} variants",
            self.kernels, self.default_variants, self.variants
        )?;
        writeln!(
            f,
            "{} variants are modules ({} distinct); {} prepared, {} ready for naga ({} with split samplers)",
            self.modules, self.distinct_modules, self.prepared, self.ready, self.split
        )?;
        let written: Vec<String> = Backend::ALL
            .into_iter()
            .map(|backend| {
                let count = self.written.get(&backend).copied().unwrap_or(0);
                format!("{} {count}", backend.name())
            })
            .collect();
        writeln!(f, "written by naga: {}", written.join(", "))?;
        writeln!(
            f,
            "declared bindings are the module's: {}; slot flags agree: {}; thread group size is the workgroup size: {}; constant buffer offsets are the block's: {}",
            self.bindings, self.slot_flags, self.group_sizes, self.constant_buffers
        )?;
        writeln!(
            f,
            "stage programs {}, layouts {}, targets {}",
            self.programs, self.layouts, self.targets
        )?;
        for (image, count) in &self.images {
            writeln!(f, "{count} storage images: {image}")?;
        }
        for (image, count) in &self.beyond_baseline {
            writeln!(
                f,
                "{count} variants need device features to bind a storage image: {image}"
            )?;
        }
        for (cause, count) in &self.failures {
            writeln!(f, "{count} {cause}: {}", self.examples[cause])?;
        }
        Ok(())
    }
}
