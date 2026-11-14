use std::collections::{BTreeMap, BTreeSet};

use rspirv::dr::{Instruction, Module, Operand};
use rspirv::spirv::{Decoration, Dim, ImageFormat, Op, StorageClass};

use super::{id, malformed};
use crate::module::ModuleError;

#[derive(Clone, Copy)]
pub(super) struct Image {
    pub scalar: u32,
    pub components: u32,
    pub float: bool,
}

pub(super) struct Scan {
    pub images: BTreeMap<u32, Image>,
    pub pointers: BTreeMap<u32, u32>,
    pub variables: BTreeMap<u32, u32>,
    pub loads: BTreeMap<u32, u32>,
    addresses: BTreeMap<u32, (u32, u32)>,
}

impl Scan {
    pub fn new(module: &Module) -> Result<Self, ModuleError> {
        let mut scan = Self {
            images: BTreeMap::new(),
            pointers: BTreeMap::new(),
            variables: BTreeMap::new(),
            loads: BTreeMap::new(),
            addresses: BTreeMap::new(),
        };
        for image in module
            .types_global_values
            .iter()
            .filter(|i| buffer_image(i))
        {
            let image_id = image.result_id.ok_or_else(|| malformed("image ID"))?;
            let address = address_of(module, image_id)?;
            scan.addresses.insert(image_id, address);
            scan.images.insert(image_id, scan.image(module, image)?);
        }
        scan.find_resources(module)?;
        scan.check_uses(module)?;
        Ok(scan)
    }

    pub fn refusal(&self, image: u32, what: impl Into<String>) -> ModuleError {
        let (set, binding) = self.addresses[&image];
        ModuleError::TexelBuffer {
            set,
            binding,
            what: what.into(),
        }
    }

    fn image(&self, module: &Module, i: &Instruction) -> Result<Image, ModuleError> {
        let image = i.result_id.unwrap();
        let a = &i.operands;
        if a[3] != Operand::LiteralBit32(0) {
            return Err(self.refusal(image, "arrays of buffer images"));
        }
        if a[5] != Operand::LiteralBit32(1) {
            return Err(self.refusal(image, "write (storage buffer image)"));
        }
        let Operand::ImageFormat(format) = a[6] else {
            return Err(malformed("image format"));
        };
        let (components, kind) = match format {
            ImageFormat::Rgba32f => (4, None),
            ImageFormat::Rg32f => (2, None),
            ImageFormat::R32f => (1, None),
            ImageFormat::Rgba32ui => (4, Some(0)),
            ImageFormat::Rg32ui => (2, Some(0)),
            ImageFormat::R32ui => (1, Some(0)),
            ImageFormat::Rgba32i => (4, Some(1)),
            ImageFormat::Rg32i => (2, Some(1)),
            ImageFormat::R32i => (1, Some(1)),
            _ => return Err(self.refusal(image, format!("format {format:?} needs conversion"))),
        };
        let scalar = id(&a[0]).ok_or_else(|| malformed("sampled type"))?;
        let expected = match kind {
            None => (Op::TypeFloat, vec![Operand::LiteralBit32(32)]),
            Some(sign) => (
                Op::TypeInt,
                vec![Operand::LiteralBit32(32), Operand::LiteralBit32(sign)],
            ),
        };
        let exact = module.types_global_values.iter().any(|t| {
            t.result_id == Some(scalar) && t.class.opcode == expected.0 && t.operands == expected.1
        });
        if !exact
            || a[4] != Operand::LiteralBit32(0)
            || a.len() != 7
            || !matches!(a[2], Operand::LiteralBit32(0 | 2))
        {
            return Err(self.refusal(
                image,
                format!("format {format:?} has non-exact sampled type or shape"),
            ));
        }
        Ok(Image {
            scalar,
            components,
            float: kind.is_none(),
        })
    }

    fn find_resources(&mut self, module: &Module) -> Result<(), ModuleError> {
        for i in &module.types_global_values {
            if matches!(i.class.opcode, Op::TypeArray | Op::TypeRuntimeArray)
                && let Some(image) = i
                    .operands
                    .first()
                    .and_then(id)
                    .filter(|t| self.images.contains_key(t))
            {
                return Err(self.refusal(image, "arrays of buffer images"));
            }
            if i.class.opcode == Op::TypePointer
                && let Some(image) = i.operands.get(1).and_then(id)
                && self.images.contains_key(&image)
            {
                if i.operands[0] != Operand::StorageClass(StorageClass::UniformConstant) {
                    return Err(self.refusal(image, "non-fetch use: image pointer storage class"));
                }
                self.pointers.insert(i.result_id.unwrap(), image);
            }
        }
        for i in &module.types_global_values {
            if i.class.opcode == Op::Variable
                && let Some(image) = i.result_type.and_then(|ty| self.pointers.get(&ty))
            {
                if i.operands != [Operand::StorageClass(StorageClass::UniformConstant)] {
                    return Err(self.refusal(*image, "non-fetch use: image variable initializer"));
                }
                let variable = i.result_id.unwrap();
                self.addresses
                    .insert(variable, descriptor_address(module, Some(variable))?);
                self.variables.insert(variable, *image);
            }
        }
        for i in module.all_inst_iter() {
            if i.class.opcode == Op::Load
                && let Some(image) = i.result_type.filter(|t| self.images.contains_key(t))
            {
                let variable = i.operands.first().and_then(id);
                if i.operands.len() != 1
                    || variable.and_then(|v| self.variables.get(&v)) != Some(&image)
                {
                    return Err(self.refusal(image, "non-fetch use: indirect image load"));
                }
                self.loads.insert(i.result_id.unwrap(), variable.unwrap());
            }
        }
        Ok(())
    }

    fn check_uses(&self, module: &Module) -> Result<(), ModuleError> {
        for i in module.all_inst_iter() {
            if matches!(
                i.class.opcode,
                Op::Name | Op::MemberName | Op::Decorate | Op::EntryPoint
            ) {
                continue;
            }
            for used in i.operands.iter().filter_map(id) {
                let Some(image) = self.image_for(used) else {
                    continue;
                };
                let allowed = match i.class.opcode {
                    Op::TypePointer => self.pointers.contains_key(&i.result_id.unwrap()),
                    Op::Load => self.loads.contains_key(&i.result_id.unwrap()),
                    Op::ImageFetch => self.valid_fetch(module, i, used, image)?,
                    _ => false,
                };
                if !allowed {
                    return Err(self.use_error(used, i));
                }
            }
            if let Some(image) = i.result_type.and_then(|ty| self.image_for(ty)) {
                let allowed = match i.class.opcode {
                    Op::Variable => self.variables.contains_key(&i.result_id.unwrap()),
                    Op::Load => self.loads.contains_key(&i.result_id.unwrap()),
                    _ => false,
                };
                if !allowed {
                    return Err(self.use_error(image, i));
                }
            }
        }
        Ok(())
    }

    fn image_for(&self, used: u32) -> Option<u32> {
        self.images
            .get(&used)
            .map(|_| used)
            .or_else(|| self.pointers.get(&used).copied())
            .or_else(|| self.variables.get(&used).copied())
            .or_else(|| self.loads.get(&used).map(|var| self.variables[var]))
    }

    fn valid_fetch(
        &self,
        module: &Module,
        i: &Instruction,
        used: u32,
        image: u32,
    ) -> Result<bool, ModuleError> {
        if i.operands.first().and_then(id) != Some(used) || !self.loads.contains_key(&used) {
            return Ok(false);
        }
        if i.operands.len() != 2
            && i.operands.get(2..)
                != Some(&[Operand::ImageOperands(rspirv::spirv::ImageOperands::NONE)])
        {
            return Err(self.refusal(self.origin(used), "non-fetch use: image fetch operands"));
        }
        let scalar = self.images[&image].scalar;
        Ok(module.types_global_values.iter().any(|t| {
            t.result_id == i.result_type
                && t.class.opcode == Op::TypeVector
                && t.operands == [Operand::IdRef(scalar), Operand::LiteralBit32(4)]
        }))
    }

    fn origin(&self, used: u32) -> u32 {
        self.loads
            .get(&used)
            .copied()
            .or_else(|| self.variables.contains_key(&used).then_some(used))
            .unwrap_or_else(|| self.image_for(used).unwrap())
    }

    fn use_error(&self, used: u32, i: &Instruction) -> ModuleError {
        let what = match i.class.opcode {
            Op::ImageWrite | Op::ImageRead => "write or storage-image read",
            Op::ImageQuerySize | Op::ImageQuerySizeLod => "size query",
            Op::ImageSparseFetch | Op::ImageSparseRead => "sparse fetch",
            Op::TypeArray | Op::TypeRuntimeArray => "arrays of buffer images",
            Op::FunctionParameter | Op::TypeFunction | Op::FunctionCall => {
                "non-fetch use: function parameter"
            }
            Op::CopyObject => "non-fetch use: OpCopyObject",
            _ => {
                return self.refusal(
                    self.origin(used),
                    format!("non-fetch use: {:?}", i.class.opcode),
                );
            }
        };
        self.refusal(self.origin(used), what)
    }
}

fn buffer_image(i: &Instruction) -> bool {
    i.class.opcode == Op::TypeImage && i.operands.get(1) == Some(&Operand::Dim(Dim::DimBuffer))
}

fn address_of(module: &Module, image: u32) -> Result<(u32, u32), ModuleError> {
    let mut related = BTreeSet::from([image]);
    loop {
        let before = related.len();
        for i in &module.types_global_values {
            if matches!(
                i.class.opcode,
                Op::TypePointer
                    | Op::TypeArray
                    | Op::TypeRuntimeArray
                    | Op::TypeStruct
                    | Op::TypeSampledImage
            ) && i
                .operands
                .iter()
                .filter_map(id)
                .any(|id| related.contains(&id))
            {
                related.extend(i.result_id);
            }
        }
        if related.len() == before {
            break;
        }
    }
    let variable = module
        .types_global_values
        .iter()
        .find(|i| {
            i.class.opcode == Op::Variable && i.result_type.is_some_and(|ty| related.contains(&ty))
        })
        .and_then(|i| i.result_id);
    descriptor_address(module, variable)
}

fn descriptor_address(module: &Module, variable: Option<u32>) -> Result<(u32, u32), ModuleError> {
    let decoration = |kind| {
        module
            .annotations
            .iter()
            .find_map(|i| match i.operands.as_slice() {
                [
                    Operand::IdRef(target),
                    Operand::Decoration(found),
                    Operand::LiteralBit32(value),
                ] if i.class.opcode == Op::Decorate
                    && Some(*target) == variable
                    && *found == kind =>
                {
                    Some(*value)
                }
                _ => None,
            })
    };
    match (
        decoration(Decoration::DescriptorSet),
        decoration(Decoration::Binding),
    ) {
        (Some(set), Some(binding)) => Ok((set, binding)),
        _ => Err(malformed("buffer image has no descriptor set/binding")),
    }
}
