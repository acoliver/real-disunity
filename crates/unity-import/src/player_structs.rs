//! Checks the structs of a player's bound constant buffers against the uniform blocks of the
//! variant's stage modules, using the placement `worldrunner-render-program` packs by. It reports
//! counts only: how many buffers and struct members the rule explains exactly, the facts the rule
//! rests on, the shapes of the members, and how the members' names relate to other names. No
//! name or string of the player leaves the decoders.

use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
    path::PathBuf,
};

use worldrunner_render_program::{
    buffers::BufferLayout, constants::pack_variant_buffer, structs::StructPlacement,
};
use worldrunner_unity_serialized::{
    ClassId,
    graphics::Material,
    shader_parameters::{
        BufferMember, ConstantBuffer, ResourceBinding, ResourceKind, StructLayout,
        VariantParameters,
    },
};

use crate::{
    PlayerError,
    player_module_types::{Base, BlockTree, Field, Shape, block_trees},
    player_objects::OpenFile,
    player_parameters::Tally,
    player_variants::{Event, RawStage, walk_variants},
};

/// The facts the rule rests on, each counted over the bound buffers that have a struct, a placement
/// from the rule and a block. [`ALL`](fact::ALL) lists them in the order a buffer's cause is
/// picked from.
pub mod fact {
    pub const NO_MEMBERS: &str = "the buffer has no members of its own";
    pub const INDEX_IS_BLOCK_OFFSET: &str =
        "the struct's index is the Offset of a struct array in the block";
    pub const ONLY_BLOCK_MEMBER: &str = "that array is the block's only member";
    pub const ARRAY_SIZE_IS_LENGTH: &str = "the struct's array size is the block array's length";
    pub const SIZE_IS_ARRAY_STRIDE: &str = "the struct's size is the block array's ArrayStride";
    pub const SIZE_IS_STRUCT_SPAN: &str = "the struct's size is the struct type's span";
    pub const BUFFER_SIZE_IS_THE_ARRAYS: &str =
        "the buffer's size is the array size times the struct size";
    pub const BUFFER_SIZE_IS_BLOCK_SPAN: &str = "the buffer's size is the block's span";
    pub const MEMBER_OFFSETS_ARE_THE_TYPES: &str =
        "the member offsets are the struct type's Offsets, as sets";
    pub const MEMBERS_END_IN_STRUCT: &str = "every member ends within the struct's size";
    pub const STAGES_AGREE: &str = "the stages that name the buffer declare the same block";
    pub const PACKER_ACCEPTS: &str = "the packer accepts the buffer";
    /// Informational: a buffer is explained whether or not this holds.
    pub const MEMBERS_IN_BLOCK_ORDER: &str =
        "the members are listed in the struct type's member order";

    pub const ALL: [&str; 13] = [
        NO_MEMBERS,
        INDEX_IS_BLOCK_OFFSET,
        ONLY_BLOCK_MEMBER,
        ARRAY_SIZE_IS_LENGTH,
        SIZE_IS_ARRAY_STRIDE,
        SIZE_IS_STRUCT_SPAN,
        BUFFER_SIZE_IS_THE_ARRAYS,
        BUFFER_SIZE_IS_BLOCK_SPAN,
        MEMBER_OFFSETS_ARE_THE_TYPES,
        MEMBERS_END_IN_STRUCT,
        STAGES_AGREE,
        PACKER_ACCEPTS,
        MEMBERS_IN_BLOCK_ORDER,
    ];
}

const REFUSED: &str = "the packer refuses the struct";
const NO_BLOCK: &str = "no block bound in a stage the binding names";
const NOT_COMPARED: &str = "its buffer has no struct type to compare with";
const NO_MEMBER_AT_OFFSET: &str = "no member of the struct type at the offset";
const TYPE_DIFFERS: &str = "type of the member differs from the struct type's";
const TOO_LARGE: &str = "the member is larger than the struct type's member";

/// Counts from checking a player's bound constant buffers that have structs.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StructReport {
    /// Bound buffers with structs, per variant, and how many the rule explains exactly: every
    /// fact except [`fact::MEMBERS_IN_BLOCK_ORDER`] holds and every member is explained.
    pub buffers: Tally,
    pub unexplained: BTreeMap<String, usize>,
    /// Members of those buffers' structs, and how many are at an offset of the struct type with
    /// the same type and a span that holds them.
    pub members: Tally,
    pub unexplained_members: BTreeMap<String, usize>,
    pub facts: BTreeMap<&'static str, Tally>,
    /// Members by their declared shape and the type the struct type has at their offset.
    pub shapes: BTreeMap<String, usize>,
    member_names: BTreeSet<String>,
    struct_names: BTreeSet<String>,
    top_level_names: BTreeSet<String>,
    material_names: BTreeSet<String>,
}

/// How the names of struct members relate to other names, as counts of distinct names.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NameCounts {
    pub members: usize,
    pub structs: usize,
    pub members_that_are_top_level_members: usize,
    pub members_that_are_material_properties: usize,
    pub members_that_are_struct_names: usize,
    /// Members whose name is a top-level member's or a material property's name followed by
    /// more characters, and how many different endings those have.
    pub members_with_a_suffix_after_a_known_name: usize,
    pub suffixes: usize,
}

fn count(map: &mut BTreeMap<String, usize>, cause: impl Into<String>) {
    *map.entry(cause.into()).or_default() += 1;
}

/// The struct type a block member holds, with the array's length and stride when it is an
/// array of the struct.
#[derive(Clone, Copy)]
struct Inner<'a> {
    fields: &'a [Field],
    length: Option<u32>,
    stride: Option<u32>,
}

fn struct_of(shape: &Shape) -> Option<Inner<'_>> {
    let (fields, length, stride) = match shape {
        Shape::Struct(fields) => (fields, None, None),
        Shape::Array {
            element,
            length,
            stride,
        } => match &**element {
            Shape::Struct(fields) => (fields, *length, *stride),
            _ => return None,
        },
        _ => return None,
    };
    Some(Inner {
        fields,
        length,
        stride,
    })
}

fn span(fields: &[Field]) -> u32 {
    Shape::Struct(fields.to_vec()).span(None)
}

fn base_label(base: Base) -> &'static str {
    match base {
        Base::Float => "float",
        Base::Signed => "signed",
        Base::Unsigned => "unsigned",
        Base::Bool => "bool",
    }
}

fn shape_label(shape: &Shape) -> String {
    match shape {
        Shape::Scalar { base, .. } => format!("{} x1", base_label(*base)),
        Shape::Vector { count, base, .. } => format!("{} x{count}", base_label(*base)),
        Shape::Matrix { columns, rows } => format!("matrix {columns}x{rows}"),
        Shape::Array {
            element, stride, ..
        } => format!(
            "array of {} stride {}",
            shape_label(element),
            stride.map_or("none".to_owned(), |s| s.to_string())
        ),
        Shape::Struct(_) => "struct".to_owned(),
    }
}

fn declared_label(member: &BufferMember) -> String {
    format!(
        "kind {} {}x{}{}",
        member.kind,
        member.rows,
        member.columns,
        if member.is_matrix { " matrix" } else { "" }
    )
}

/// Whether a member declared as `member` has the type `shape`. A float matrix of four columns is
/// four float4 rows, as an array of float4 at stride 16 or as a matrix of that many columns.
fn compatible(member: &BufferMember, shape: &Shape) -> bool {
    let float4 = Shape::Vector {
        count: 4,
        bytes: 4,
        base: Base::Float,
    };
    let base_matches = |base: Base| match member.kind {
        0 => base == Base::Float,
        1 => matches!(base, Base::Signed | Base::Unsigned),
        _ => false,
    };
    let rows = u32::try_from(member.rows).unwrap_or(0);
    let columns = u32::try_from(member.columns).unwrap_or(0);
    match shape {
        Shape::Scalar { bytes: 4, base } => {
            !member.is_matrix && columns == 1 && base_matches(*base)
        }
        Shape::Vector {
            count,
            bytes: 4,
            base,
        } => !member.is_matrix && *count == columns && base_matches(*base),
        Shape::Array {
            element,
            length,
            stride,
        } => {
            member.is_matrix
                && member.kind == 0
                && *length == Some(rows)
                && *stride == Some(16)
                && **element == float4
        }
        Shape::Matrix {
            columns: c,
            rows: r,
        } => member.is_matrix && member.kind == 0 && *c == rows && *r == 4,
        _ => false,
    }
}

/// The bytes the packer writes for `member`, or 0 for a shape it does not write.
fn extent(member: &BufferMember) -> u32 {
    let (lines, width) = if member.is_matrix {
        (member.rows, 16)
    } else {
        (member.columns, 4)
    };
    u32::try_from(lines).unwrap_or(0) * width
}

/// The bound blocks of the stages a binding names.
fn blocks_of<'a>(
    binding: &ResourceBinding,
    trees: &'a [(u8, Vec<BlockTree>)],
) -> Vec<&'a BlockTree> {
    let (set, index) = (u32::from(binding.slot.set), u32::from(binding.slot.binding));
    trees
        .iter()
        .filter(|(stage, _)| stage & binding.slot.stages != 0)
        .filter_map(|(_, blocks)| blocks.iter().find(|b| b.set == set && b.binding == index))
        .collect()
}

/// One bound buffer with a struct, the rule's placement of it and the block it is bound to.
struct Subject<'a> {
    buffer: &'a ConstantBuffer,
    layout: &'a StructLayout,
    placement: StructPlacement,
    blocks: Vec<&'a BlockTree>,
    /// The block member that holds the struct: the struct array at the offset the struct's index
    /// gives.
    holder: Option<&'a Field>,
    /// The struct type of the holder.
    inner: Option<Inner<'a>>,
}

impl<'a> Subject<'a> {
    fn new(
        buffer: &'a ConstantBuffer,
        placement: StructPlacement,
        blocks: Vec<&'a BlockTree>,
    ) -> Self {
        let layout = &buffer.structs[0];
        let holder = blocks[0].fields.iter().find(|field| {
            struct_of(&field.shape).is_some() && field.offset == u32::try_from(layout.index).ok()
        });
        let inner = holder.and_then(|field| struct_of(&field.shape));
        Self {
            buffer,
            layout,
            placement,
            blocks,
            holder,
            inner,
        }
    }

    fn type_offsets(&self) -> Option<Vec<i64>> {
        let fields = self.inner?.fields;
        fields.iter().map(|f| f.offset.map(i64::from)).collect()
    }

    fn member_offsets(&self) -> Vec<i64> {
        self.layout
            .members
            .iter()
            .map(|m| i64::from(m.offset))
            .collect()
    }

    fn length_is_count(&self) -> bool {
        self.inner.is_some_and(|inner| {
            inner.length.map_or(self.placement.count == 1, |l| {
                usize::try_from(l) == Ok(self.placement.count)
            })
        })
    }

    /// The block array's stride, or for a lone struct the struct type's span.
    fn stride_is_size(&self) -> bool {
        self.inner.is_some_and(|inner| {
            let stride = inner.stride.unwrap_or_else(|| span(inner.fields));
            usize::try_from(stride) == Ok(self.placement.stride)
        })
    }

    fn span_is_size(&self) -> bool {
        self.inner
            .is_some_and(|inner| usize::try_from(span(inner.fields)) == Ok(self.placement.stride))
    }

    fn buffer_size(&self) -> Option<usize> {
        usize::try_from(self.buffer.size).ok()
    }

    fn size_is_the_arrays(&self) -> bool {
        self.buffer_size() == self.placement.count.checked_mul(self.placement.stride)
    }

    fn size_is_block_span(&self) -> bool {
        let end = |f: &Field| f.offset.unwrap_or(0) + f.span();
        let block = self.blocks[0].fields.iter().map(end).max();
        self.buffer_size() == block.and_then(|b| usize::try_from(b).ok())
    }

    fn offsets_are_the_types(&self) -> bool {
        let sorted = |mut v: Vec<i64>| {
            v.sort_unstable();
            v
        };
        self.type_offsets()
            .is_some_and(|theirs| sorted(theirs) == sorted(self.member_offsets()))
    }

    fn members_end_in_struct(&self) -> bool {
        self.layout.members.iter().all(|m| {
            let end = u64::try_from(m.offset).map(|o| o + u64::from(extent(m)));
            end.is_ok_and(|end| end <= self.placement.stride as u64)
        })
    }

    fn stages_agree(&self) -> bool {
        self.blocks
            .iter()
            .all(|b| b.fields == self.blocks[0].fields)
    }

    /// Each fact, in the order of [`fact::ALL`].
    fn facts(&self) -> [bool; 13] {
        [
            self.buffer.members.is_empty(),
            self.holder.is_some(),
            self.blocks[0].fields.len() == 1,
            self.length_is_count(),
            self.stride_is_size(),
            self.span_is_size(),
            self.size_is_the_arrays(),
            self.size_is_block_span(),
            self.offsets_are_the_types(),
            self.members_end_in_struct(),
            self.stages_agree(),
            pack_variant_buffer(self.buffer, &[], 0).is_ok(),
            self.type_offsets() == Some(self.member_offsets()),
        ]
    }

    /// How `member` compares with the struct type: the shapes of the two when the type has a
    /// member at the offset, and why the type does not explain the member, if it does not.
    fn member(&self, member: &BufferMember) -> (Option<String>, Result<(), &'static str>) {
        let Some(Inner { fields, .. }) = self.inner else {
            return (None, Err(NOT_COMPARED));
        };
        let offset = Some(i64::from(member.offset));
        let Some(at) = fields.iter().find(|f| f.offset.map(i64::from) == offset) else {
            return (None, Err(NO_MEMBER_AT_OFFSET));
        };
        let shape = format!("{} <-> {}", declared_label(member), shape_label(&at.shape));
        let result = if !compatible(member, &at.shape) {
            Err(TYPE_DIFFERS)
        } else if extent(member) > at.span() {
            Err(TOO_LARGE)
        } else {
            Ok(())
        };
        (Some(shape), result)
    }
}

fn tally(counts: &mut Tally, held: bool) {
    counts.total += 1;
    counts.matched += usize::from(held);
}

impl StructReport {
    /// Checks the bound constant buffers of the variant that have structs against the blocks of
    /// its `stages`.
    pub fn add_variant(&mut self, parameters: &VariantParameters, stages: &[RawStage]) {
        for buffer in &parameters.constant_buffers {
            let names = buffer.members.iter().map(|m| m.name.clone());
            self.top_level_names.extend(names);
        }
        let trees: Vec<(u8, Vec<BlockTree>)> = stages
            .iter()
            .filter_map(|stage| {
                let bit = 1u8.checked_shl(u32::try_from(stage.slot).ok()?)?;
                Some((bit, block_trees(&stage.words)))
            })
            .collect();
        let bound = parameters
            .bindings
            .iter()
            .filter(|binding| binding.kind == ResourceKind::ConstantBuffer);
        for binding in bound {
            let blocks = blocks_of(binding, &trees);
            let layouts = parameters.constant_buffers.iter();
            for buffer in layouts.filter(|b| b.name == binding.name && !b.structs.is_empty()) {
                self.add_buffer(buffer, &blocks);
            }
        }
    }

    /// Adds the property names of a player's materials, which struct member names are compared
    /// with.
    pub fn add_material_names(&mut self, names: impl IntoIterator<Item = String>) {
        self.material_names.extend(names);
    }

    fn add_buffer(&mut self, buffer: &ConstantBuffer, blocks: &[&BlockTree]) {
        for layout in &buffer.structs {
            self.struct_names.insert(layout.name.clone());
            let names = layout.members.iter().map(|m| m.name.clone());
            self.member_names.extend(names);
        }
        let layout = BufferLayout::from_variant(buffer);
        let placed = layout.struct_placement(&[]);
        let cause = match placed {
            Ok(Some((_, placement))) if !blocks.is_empty() => {
                let subject = Subject::new(buffer, placement, blocks.to_vec());
                return self.add_subject(&subject);
            }
            Ok(Some(_)) => NO_BLOCK,
            _ => REFUSED,
        };
        tally(&mut self.buffers, false);
        count(&mut self.unexplained, cause);
        for _ in buffer.structs.iter().flat_map(|s| &s.members) {
            tally(&mut self.members, false);
            count(&mut self.unexplained_members, NOT_COMPARED);
        }
    }

    fn add_subject(&mut self, subject: &Subject<'_>) {
        let facts = subject.facts();
        for (name, held) in fact::ALL.iter().zip(facts) {
            tally(self.facts.entry(name).or_default(), held);
        }
        let members_explained = self.add_members(subject);
        // The last fact only reports the members' order; the others are required.
        let required = &facts[..facts.len() - 1];
        let failed = fact::ALL.iter().zip(required).find(|(_, held)| !**held);
        tally(&mut self.buffers, failed.is_none() && members_explained);
        match failed {
            Some((name, _)) => count(&mut self.unexplained, *name),
            None if !members_explained => count(&mut self.unexplained, "a member is unexplained"),
            None => {}
        }
    }

    /// Counts the members of the subject's struct, and says whether every one is explained.
    fn add_members(&mut self, subject: &Subject<'_>) -> bool {
        let mut all = true;
        for member in &subject.layout.members {
            let (shape, result) = subject.member(member);
            if let Some(shape) = shape {
                count(&mut self.shapes, shape);
            }
            tally(&mut self.members, result.is_ok());
            if let Err(cause) = result {
                count(&mut self.unexplained_members, cause);
                all = false;
            }
        }
        all
    }

    /// How the struct members' names relate to the other names seen.
    #[must_use]
    pub fn names(&self) -> NameCounts {
        let known: BTreeSet<&str> = self
            .top_level_names
            .iter()
            .chain(&self.material_names)
            .map(String::as_str)
            .filter(|name| !name.is_empty())
            .collect();
        let suffix_of = |name: &str| {
            known
                .iter()
                .filter_map(|k| name.strip_prefix(k))
                .filter(|suffix| !suffix.is_empty())
                .min_by_key(|suffix| suffix.len())
                .map(str::to_owned)
        };
        let suffixes: Vec<String> = self
            .member_names
            .iter()
            .filter_map(|name| suffix_of(name))
            .collect();
        let among = |names: &BTreeSet<String>| {
            self.member_names
                .iter()
                .filter(|n| names.contains(*n))
                .count()
        };
        NameCounts {
            members: self.member_names.len(),
            structs: self.struct_names.len(),
            members_that_are_top_level_members: among(&self.top_level_names),
            members_that_are_material_properties: among(&self.material_names),
            members_that_are_struct_names: among(&self.struct_names),
            members_with_a_suffix_after_a_known_name: suffixes.len(),
            suffixes: suffixes.iter().collect::<BTreeSet<_>>().len(),
        }
    }
}

fn section(
    f: &mut fmt::Formatter<'_>,
    title: &str,
    counts: &BTreeMap<String, usize>,
) -> fmt::Result {
    if counts.is_empty() {
        return Ok(());
    }
    writeln!(f, "{title}:")?;
    for (cause, count) in counts {
        writeln!(f, "  {count} {cause}")?;
    }
    Ok(())
}

impl fmt::Display for StructReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(
            f,
            "bound constant buffers with structs that the rule explains exactly: {}",
            self.buffers
        )?;
        section(
            f,
            "buffers the rule leaves unexplained, by cause",
            &self.unexplained,
        )?;
        writeln!(
            f,
            "struct members that the rule explains exactly: {}",
            self.members
        )?;
        section(
            f,
            "members the rule leaves unexplained, by cause",
            &self.unexplained_members,
        )?;
        writeln!(f, "facts, over the buffers that have a struct and a block:")?;
        for name in fact::ALL {
            if let Some(held) = self.facts.get(name) {
                writeln!(f, "  {held} {name}")?;
            }
        }
        section(
            f,
            "members by declared shape and the block's type",
            &self.shapes,
        )?;
        let names = self.names();
        writeln!(f, "distinct struct member names: {}", names.members)?;
        writeln!(f, "distinct struct names: {}", names.structs)?;
        writeln!(
            f,
            "member names that are another buffer's top-level member names: {}",
            names.members_that_are_top_level_members
        )?;
        writeln!(
            f,
            "member names that are material property names: {}",
            names.members_that_are_material_properties
        )?;
        writeln!(
            f,
            "member names that are struct names: {}",
            names.members_that_are_struct_names
        )?;
        writeln!(
            f,
            "member names that are a top-level or material name followed by a suffix: {} (with {} different suffixes)",
            names.members_with_a_suffix_after_a_known_name, names.suffixes
        )
    }
}

/// The names of every material property in the serialized files of `paths`.
fn material_names(paths: &[PathBuf]) -> Result<Vec<String>, PlayerError> {
    let mut names = Vec::new();
    for path in paths {
        let mut open = OpenFile::open(path)?;
        let materials: Vec<i64> = open
            .file
            .objects()
            .iter()
            .filter(|object| object.class_id == ClassId::MATERIAL)
            .map(|object| object.path_id)
            .collect();
        for path_id in materials {
            let properties = open.decode::<Material>(path_id)?.properties;
            names.extend(properties.textures.into_iter().map(|(name, _)| name));
            names.extend(properties.ints.into_iter().map(|(name, _)| name));
            names.extend(properties.floats.into_iter().map(|(name, _)| name));
            names.extend(properties.colors.into_iter().map(|(name, _)| name));
        }
    }
    Ok(names)
}

/// Checks the structs of every bound constant buffer of the Vulkan variants of the shaders in
/// `paths` against their stage modules' blocks.
///
/// # Errors
/// Fails if a file cannot be read, a shader does not load or a material does not decode; a
/// buffer that the rule does not explain is counted, not an error.
pub fn struct_report(paths: &[PathBuf]) -> Result<StructReport, PlayerError> {
    let mut report = StructReport::default();
    walk_variants(paths, &mut |event| {
        if let Event::Variant {
            parameters, stages, ..
        } = event
        {
            report.add_variant(parameters, stages);
        }
    })?;
    report.add_material_names(material_names(paths)?);
    Ok(report)
}
