//! Counts which constant buffer members the variants of a player's shaders hold that no shader
//! declares as a material property. A member that most variants hold and no shader declares is
//! filled by the engine, not by a material, so this is the measurement a host uses to find the
//! names it must supply for every draw. It reports counts and the names of the members that most
//! variants hold; it does not say what a member means.

use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
    path::PathBuf,
};

use worldrunner_unity_serialized::shader_parameters::{
    BufferMember, ResourceKind, VariantParameters,
};

use crate::{
    PlayerError,
    player_variants::{Event, walk_variants},
};

/// A member as a variant's parameters describe it, with the buffer that holds it. A member of a
/// struct is listed under `struct <name>`. A buffer name's trailing hash of digits is replaced by
/// `#`.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct MemberShape {
    pub buffer: String,
    pub name: String,
    pub kind: i32,
    pub rows: i32,
    pub columns: i32,
    pub is_matrix: bool,
    pub array_size: i32,
}

/// The digits that end a buffer's name are at least this many when they are a hash the compiler
/// appended to give each variant's globals a name of their own.
const HASH_DIGITS: usize = 6;

/// `name` with a trailing hash of digits replaced by `#`, so a buffer that every variant names
/// differently counts as one.
fn without_hash(name: &str) -> String {
    let stem = name.trim_end_matches(|c: char| c.is_ascii_digit());
    if name.len() - stem.len() >= HASH_DIGITS {
        format!("{stem}#")
    } else {
        name.to_owned()
    }
}

impl MemberShape {
    fn new(buffer: &str, member: &BufferMember) -> Self {
        Self {
            buffer: without_hash(buffer),
            name: member.name.clone(),
            kind: member.kind,
            rows: member.rows,
            columns: member.columns,
            is_matrix: member.is_matrix,
            array_size: member.array_size,
        }
    }
}

/// How many variants hold a member, and in how many of them the variant's shader declares a
/// property of that name.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MemberCount {
    pub variants: usize,
    pub declared: usize,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct BuiltinReport {
    pub shaders: usize,
    /// Variants whose parameters decoded.
    pub variants: usize,
    /// Variants that did not decode.
    pub unreadable: usize,
    pub members: BTreeMap<MemberShape, MemberCount>,
    /// Texture bindings of the variants that decoded.
    pub textures: usize,
    /// Texture bindings whose variant holds a float vector member named like the texture plus
    /// `_ST`.
    pub with_transform: usize,
    /// Texture bindings whose variant holds a float vector member named like the texture plus
    /// `_TexelSize`.
    pub with_texel_size: usize,
}

impl BuiltinReport {
    /// Counts the members of one variant once each, and once more as declared when the shader
    /// the variant belongs to declares a property of the member's name.
    pub fn add_variant(&mut self, parameters: &VariantParameters, declared: &BTreeSet<String>) {
        self.variants += 1;
        let mut held = BTreeSet::new();
        for buffer in &parameters.constant_buffers {
            held.extend(
                buffer
                    .members
                    .iter()
                    .map(|member| MemberShape::new(&buffer.name, member)),
            );
            for layout in &buffer.structs {
                let name = format!("struct {}", layout.name);
                held.extend(
                    layout
                        .members
                        .iter()
                        .map(|member| MemberShape::new(&name, member)),
                );
            }
        }
        self.add_textures(parameters, &held);
        for shape in held {
            let is_declared = declared.contains(&shape.name);
            let count = self.members.entry(shape).or_default();
            count.variants += 1;
            count.declared += usize::from(is_declared);
        }
    }

    /// Counts the variant's texture bindings, and those with a member named after them.
    fn add_textures(&mut self, parameters: &VariantParameters, held: &BTreeSet<MemberShape>) {
        let vectors: BTreeSet<&str> = held
            .iter()
            .filter(|shape| !shape.is_matrix && shape.columns == 4)
            .map(|shape| shape.name.as_str())
            .collect();
        for binding in &parameters.bindings {
            if matches!(binding.kind, ResourceKind::Texture { .. }) {
                self.textures += 1;
                self.with_transform +=
                    usize::from(vectors.contains(&*format!("{}_ST", binding.name)));
                self.with_texel_size +=
                    usize::from(vectors.contains(&*format!("{}_TexelSize", binding.name)));
            }
        }
    }

    /// The members no shader declares that at least `percent` of the variants hold, most common
    /// first.
    #[must_use]
    pub fn listing(&self, percent: usize) -> Vec<(MemberShape, MemberCount)> {
        let mut listed: Vec<_> = self
            .members
            .iter()
            .filter(|(_, count)| {
                count.declared == 0 && count.variants * 100 >= self.variants * percent
            })
            .map(|(shape, count)| (shape.clone(), *count))
            .collect();
        listed.sort_by(|a, b| b.1.variants.cmp(&a.1.variants).then_with(|| a.0.cmp(&b.0)));
        listed
    }

    /// The report with its listing at `percent`, of the matrices only when `matrices` is set.
    #[must_use]
    pub fn display(&self, percent: usize, matrices: bool) -> impl fmt::Display + '_ {
        Listing {
            report: self,
            percent,
            matrices,
        }
    }
}

struct Listing<'a> {
    report: &'a BuiltinReport,
    percent: usize,
    matrices: bool,
}

impl fmt::Display for Listing<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let report = self.report;
        writeln!(
            f,
            "{} shaders with a Vulkan platform, {} variants with parameters, {} unreadable, {} \
             distinct members",
            report.shaders,
            report.variants,
            report.unreadable,
            report.members.len()
        )?;
        let never_declared = report
            .members
            .values()
            .filter(|count| count.declared == 0)
            .count();
        writeln!(f, "{never_declared} members are never a declared property")?;
        writeln!(
            f,
            "{} texture bindings: {} with a vector named for the texture plus _ST, {} plus \
             _TexelSize",
            report.textures, report.with_transform, report.with_texel_size
        )?;
        writeln!(
            f,
            "members no shader declares that at least {}% of the variants hold:",
            self.percent
        )?;
        for (shape, count) in report
            .listing(self.percent)
            .into_iter()
            .filter(|(shape, _)| shape.is_matrix || !self.matrices)
        {
            let form = match (shape.is_matrix, shape.array_size) {
                (true, 0) => format!("matrix {}x{}", shape.rows, shape.columns),
                (true, size) => format!("matrix {}x{}[{size}]", shape.rows, shape.columns),
                (false, 0) => format!("kind {} x{}", shape.kind, shape.columns),
                (false, size) => format!("kind {} x{}[{size}]", shape.kind, shape.columns),
            };
            writeln!(
                f,
                "  {:?} in {:?}: {form}: {} of {}",
                shape.name, shape.buffer, count.variants, report.variants
            )?;
        }
        Ok(())
    }
}

/// Counts the constant buffer members of every Vulkan variant of the shaders in `paths`.
///
/// # Errors
/// Fails if a file cannot be read or a shader does not load; a variant that does not decode is
/// counted, not an error.
pub fn builtin_report(paths: &[PathBuf]) -> Result<BuiltinReport, PlayerError> {
    let mut report = BuiltinReport::default();
    let mut declared = BTreeSet::new();
    walk_variants(paths, &mut |event| match event {
        Event::Shader(shader) => {
            report.shaders += 1;
            declared = shader
                .parsed
                .properties
                .iter()
                .map(|property| property.name.clone())
                .collect();
        }
        Event::Unreadable(_) => report.unreadable += 1,
        Event::Variant { parameters, .. } => report.add_variant(parameters, &declared),
    })?;
    Ok(report)
}
