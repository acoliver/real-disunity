//! The type tree of the uniform blocks a Vulkan stage module binds: each block member with its
//! `Offset`, the strides that decorate its arrays and matrices, and the structs nested in it.
//! [`crate::player_module_resources`] lists the same blocks' member offsets flat; this keeps the
//! tree, so a struct's own members and an array's stride can be checked against the
//! parameters a player stores.

use std::collections::BTreeMap;

/// The shape of a block member.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Shape {
    Scalar {
        bytes: u32,
    },
    Vector {
        count: u32,
        bytes: u32,
    },
    Matrix {
        columns: u32,
        rows: u32,
    },
    Array {
        element: Box<Shape>,
        /// `None` when the length is not a plain constant.
        length: Option<u32>,
        /// The `ArrayStride` decoration.
        stride: Option<u32>,
    },
    Struct(Vec<Field>),
}

/// A member of a struct type.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Field {
    /// The `Offset` decoration, from the start of the struct that holds the member.
    pub offset: Option<u32>,
    /// The `MatrixStride` decoration, which applies to a matrix however many arrays hold it.
    pub matrix_stride: Option<u32>,
    pub shape: Shape,
}

impl Shape {
    /// The bytes from the start of a value of this shape to the end of its last byte. An array
    /// spans its length times its stride, the padding after its last element included; a matrix
    /// spans its columns times `matrix_stride`.
    #[must_use]
    pub fn span(&self, matrix_stride: Option<u32>) -> u32 {
        match self {
            Self::Scalar { bytes } => *bytes,
            Self::Vector { count, bytes } => count * bytes,
            Self::Matrix { columns, rows } => {
                columns * matrix_stride.unwrap_or_else(|| rows * 4)
            }
            Self::Array {
                element,
                length,
                stride,
            } => {
                let stride = stride.unwrap_or_else(|| element.span(matrix_stride));
                length.unwrap_or(1) * stride
            }
            Self::Struct(fields) => fields.iter().map(Field::end).max().unwrap_or(0),
        }
    }
}

impl Field {
    /// The bytes the member covers from its own start.
    #[must_use]
    pub fn span(&self) -> u32 {
        self.shape.span(self.matrix_stride)
    }

    /// The byte past the member's last, from the start of the struct that holds it.
    fn end(&self) -> u32 {
        self.offset.unwrap_or(0) + self.span()
    }
}

/// A bound uniform or storage block and its members.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BlockTree {
    pub set: u32,
    pub binding: u32,
    pub fields: Vec<Field>,
}

const OP_TYPE_BOOL: u16 = 20;
const OP_TYPE_INT: u16 = 21;
const OP_TYPE_FLOAT: u16 = 22;
const OP_TYPE_VECTOR: u16 = 23;
const OP_TYPE_MATRIX: u16 = 24;
const OP_TYPE_ARRAY: u16 = 28;
const OP_TYPE_RUNTIME_ARRAY: u16 = 29;
const OP_TYPE_STRUCT: u16 = 30;
const OP_TYPE_POINTER: u16 = 32;
const OP_CONSTANT: u16 = 43;
const OP_VARIABLE: u16 = 59;
const OP_DECORATE: u16 = 71;
const OP_MEMBER_DECORATE: u16 = 72;
const DECORATION_ARRAY_STRIDE: u32 = 6;
const DECORATION_MATRIX_STRIDE: u32 = 7;
const DECORATION_BINDING: u32 = 33;
const DECORATION_DESCRIPTOR_SET: u32 = 34;
const DECORATION_OFFSET: u32 = 35;

/// The first instruction of a module, after its five header words.
const FIRST_INSTRUCTION: usize = 5;
/// A struct inside a struct this many levels down is not followed.
const MAX_DEPTH: usize = 16;

/// A type declaration, before its members and elements are resolved.
enum Declared {
    Scalar(u32),
    Vector { component: u32, count: u32 },
    Matrix { column: u32, columns: u32 },
    Array { element: u32, length: u32 },
    RuntimeArray { element: u32 },
    Struct(Vec<u32>),
}

#[derive(Default)]
struct Module {
    types: BTreeMap<u32, Declared>,
    constants: BTreeMap<u32, u32>,
    sets: BTreeMap<u32, u32>,
    bindings: BTreeMap<u32, u32>,
    array_strides: BTreeMap<u32, u32>,
    /// By struct ID and member index.
    offsets: BTreeMap<(u32, u32), u32>,
    matrix_strides: BTreeMap<(u32, u32), u32>,
    pointees: BTreeMap<u32, u32>,
    /// Variables as (pointer type ID, ID).
    variables: Vec<(u32, u32)>,
}

impl Module {
    fn record(&mut self, opcode: u16, operands: &[u32]) {
        match (opcode, operands) {
            (OP_DECORATE, &[target, DECORATION_DESCRIPTOR_SET, set, ..]) => {
                self.sets.insert(target, set);
            }
            (OP_DECORATE, &[target, DECORATION_BINDING, binding, ..]) => {
                self.bindings.insert(target, binding);
            }
            (OP_DECORATE, &[target, DECORATION_ARRAY_STRIDE, stride, ..]) => {
                self.array_strides.insert(target, stride);
            }
            (OP_MEMBER_DECORATE, &[owner, member, DECORATION_OFFSET, offset, ..]) => {
                self.offsets.insert((owner, member), offset);
            }
            (OP_MEMBER_DECORATE, &[owner, member, DECORATION_MATRIX_STRIDE, stride, ..]) => {
                self.matrix_strides.insert((owner, member), stride);
            }
            (OP_TYPE_POINTER, &[id, _, pointee, ..]) => {
                self.pointees.insert(id, pointee);
            }
            (OP_CONSTANT, &[_, id, value, ..]) => {
                self.constants.insert(id, value);
            }
            (OP_VARIABLE, &[pointer, id, ..]) => self.variables.push((pointer, id)),
            _ => self.record_type(opcode, operands),
        }
    }

    fn record_type(&mut self, opcode: u16, operands: &[u32]) {
        let declared = match (opcode, operands) {
            (OP_TYPE_BOOL, &[id]) => (id, Declared::Scalar(4)),
            (OP_TYPE_INT | OP_TYPE_FLOAT, &[id, width, ..]) => (id, Declared::Scalar(width / 8)),
            (OP_TYPE_VECTOR, &[id, component, count]) => (id, Declared::Vector { component, count }),
            (OP_TYPE_MATRIX, &[id, column, columns]) => (id, Declared::Matrix { column, columns }),
            (OP_TYPE_ARRAY, &[id, element, length]) => (id, Declared::Array { element, length }),
            (OP_TYPE_RUNTIME_ARRAY, &[id, element]) => (id, Declared::RuntimeArray { element }),
            (OP_TYPE_STRUCT, &[id, ref members @ ..]) => (id, Declared::Struct(members.to_vec())),
            _ => return,
        };
        self.types.insert(declared.0, declared.1);
    }

    /// The shape of type `id`, with the array stride that decorates it.
    fn shape(&self, id: u32, depth: usize) -> Option<Shape> {
        if depth > MAX_DEPTH {
            return None;
        }
        Some(match self.types.get(&id)? {
            Declared::Scalar(bytes) => Shape::Scalar { bytes: *bytes },
            Declared::Vector { component, count } => {
                let Shape::Scalar { bytes } = self.shape(*component, depth + 1)? else {
                    return None;
                };
                Shape::Vector {
                    count: *count,
                    bytes,
                }
            }
            Declared::Matrix { column, columns } => {
                let Shape::Vector { count, .. } = self.shape(*column, depth + 1)? else {
                    return None;
                };
                Shape::Matrix {
                    columns: *columns,
                    rows: count,
                }
            }
            Declared::Array { element, length } => Shape::Array {
                element: Box::new(self.shape(*element, depth + 1)?),
                length: self.constants.get(length).copied(),
                stride: self.array_strides.get(&id).copied(),
            },
            Declared::RuntimeArray { element } => Shape::Array {
                element: Box::new(self.shape(*element, depth + 1)?),
                length: None,
                stride: self.array_strides.get(&id).copied(),
            },
            Declared::Struct(members) => Shape::Struct(self.fields(id, members, depth)?),
        })
    }

    fn fields(&self, id: u32, members: &[u32], depth: usize) -> Option<Vec<Field>> {
        (0..)
            .zip(members)
            .map(|(index, &member)| {
                Some(Field {
                    offset: self.offsets.get(&(id, index)).copied(),
                    matrix_stride: self.matrix_strides.get(&(id, index)).copied(),
                    shape: self.shape(member, depth + 1)?,
                })
            })
            .collect()
    }

    /// The struct type a variable's pointer type points at, through any arrays.
    fn block_type(&self, pointer: u32) -> Option<u32> {
        let mut ty = *self.pointees.get(&pointer)?;
        loop {
            match self.types.get(&ty)? {
                Declared::Array { element, .. } | Declared::RuntimeArray { element } => {
                    ty = *element;
                }
                Declared::Struct(_) => return Some(ty),
                _ => return None,
            }
        }
    }

    fn block(&self, pointer: u32, id: u32) -> Option<BlockTree> {
        let binding = *self.bindings.get(&id)?;
        let ty = self.block_type(pointer)?;
        let Declared::Struct(members) = self.types.get(&ty)? else {
            return None;
        };
        Some(BlockTree {
            set: self.sets.get(&id).copied().unwrap_or_default(),
            binding,
            fields: self.fields(ty, members, 0)?,
        })
    }
}

/// Lists the bound blocks of a SPIR-V module with their member trees, ordered by set and
/// binding. A block whose types cannot be followed is left out. Instructions that are malformed
/// end the scan; the module is expected to have passed
/// [`worldrunner_unity_serialized::smolv::entry_points`].
#[must_use]
pub fn block_trees(words: &[u32]) -> Vec<BlockTree> {
    let mut module = Module::default();
    let mut at = FIRST_INSTRUCTION;
    while let Some(&head) = words.get(at) {
        let count = usize::try_from(head >> 16).unwrap_or_default();
        let Some(instruction) = words.get(at..at + count).filter(|_| count > 0) else {
            break;
        };
        let opcode = u16::try_from(head & 0xffff).unwrap_or_default();
        module.record(opcode, &instruction[1..]);
        at += count;
    }
    let mut blocks: Vec<_> = module
        .variables
        .iter()
        .filter_map(|&(pointer, id)| module.block(pointer, id))
        .collect();
    blocks.sort_by_key(|block| (block.set, block.binding));
    blocks
}
