//! Source-line markers and debug records in modules that need the rewrite.

use std::collections::BTreeSet;

use rspirv::dr::{Module, Operand};
use rspirv::spirv::Op;

use super::words::{Spirv, text};
use super::*;
use crate::spirv::{instructions, opcode};

const DEBUG_INFO_SET: &str = "NonSemantic.Shader.DebugInfo.100";
const OTHER_SET: &str = "NonSemantic.Other.1";
const FABS: u32 = 4;

/// A local variable the entry block initialises with a constant, a load of it that reaches the
/// output, and a GLSL.std.450 instruction whose result reaches the output too.
const LOCALS: &str = "
@group(0) @binding(0) var<storage, read_write> out: array<f32, 2>;
@compute @workgroup_size(1) fn main() {
    var local = 7.0;
    out[0] = local;
    out[1] = abs(local);
}";

fn locals(constants: &[(f32, u32)]) -> Vec<u32> {
    sentinel::spirv(LOCALS, constants)
}

#[derive(Clone, Copy)]
enum Marker {
    Line,
    NoLine,
    LineBeforeLabel,
}

/// How many markers precede the entry block's first variable. A search for the end of the
/// leading variables that assumes they are contiguous fails for some of these counts and not
/// for others, so every count is checked.
const MARKER_COUNTS: std::ops::RangeInclusive<u32> = 1..=8;

/// `words` with `count` markers in front of the entry block's first variable (or in front of its
/// label), and the `OpString` that an `OpLine` names.
fn marked(words: &[u32], marker: Marker, count: u32) -> Vec<u32> {
    let mut spirv = Spirv::new(words);
    let file = source_file(&mut spirv);
    let label = spirv.entry_label();
    for line in 12..12 + count {
        match marker {
            Marker::Line => spirv.insert(label + 1, Op::Line, &[file, line, 3]),
            Marker::NoLine => spirv.insert(label + 1, Op::NoLine, &[]),
            Marker::LineBeforeLabel => spirv.insert(label, Op::Line, &[file, line, 3]),
        }
    }
    spirv.words()
}

/// Adds the `OpString` an `OpLine` names and returns its id.
fn source_file(spirv: &mut Spirv) -> u32 {
    let file = spirv.fresh_id();
    let string = [&[file][..], &text("synthetic.src")].concat();
    spirv.insert(spirv.after_debug(), Op::String, &string);
    file
}

/// Two locals, the finite one declared first, so a marker in front of the second sits inside
/// the entry block's run of variables, where a compiler that writes a line for each
/// declaration puts it.
const TWO_LOCALS: &str = "
@group(0) @binding(0) var<storage, read_write> out: array<f32, 2>;
@compute @workgroup_size(1) fn main() {
    var first = 3.0;
    var local = 7.0;
    out[0] = local;
    out[1] = first;
}";

/// `count` line markers between the finite local and the non-finite one.
fn marked_between(count: u32) -> Vec<u32> {
    let mut spirv = Spirv::new(&sentinel::spirv(TWO_LOCALS, &[(7.0, NAN)]));
    let file = source_file(&mut spirv);
    let local = spirv.non_finite_local();
    assert_eq!(spirv.opcode_at(local - 1), Op::Variable as u32);
    for line in 12..12 + count {
        spirv.insert(local, Op::Line, &[file, line, 3]);
    }
    spirv.words()
}

#[test]
fn line_markers_between_entry_variables_do_not_stop_an_initializer_being_taken() {
    for count in MARKER_COUNTS {
        let words = marked_between(count);
        parse(&words).unwrap_or_else(|error| panic!("naga refuses {count} markers: {error}"));
        admitted(&words);
        assert!(!has_line_marker(&lower(&words).unwrap()));
    }
}

fn has_line_marker(words: &[u32]) -> bool {
    instructions(words).any(|i| [Op::Line as u32, Op::NoLine as u32].contains(&opcode(i)))
}

fn has_constant(words: &[u32], bits: u32) -> bool {
    instructions(words).any(|i| opcode(i) == Op::Constant as u32 && i[3] == bits)
}

/// Checks that naga accepts the fixture, and accepts it again after the rewrite.
fn assert_marked_module_lowers(marker: Marker) {
    for count in MARKER_COUNTS {
        let words = marked(&locals(&[(7.0, NAN)]), marker, count);
        parse(&words).unwrap_or_else(|error| panic!("naga refuses {count} markers: {error}"));
        admitted(&words);
    }
}

#[test]
fn a_line_marker_before_an_entry_variable_does_not_stop_its_initializer_being_taken() {
    assert_marked_module_lowers(Marker::Line);
}

#[test]
fn a_no_line_marker_before_an_entry_variable_does_not_stop_its_initializer_being_taken() {
    assert_marked_module_lowers(Marker::NoLine);
}

#[test]
fn a_line_marker_before_the_entry_label_does_not_break_the_rewritten_module() {
    assert_marked_module_lowers(Marker::LineBeforeLabel);
}

#[test]
fn line_markers_are_removed_from_a_rewritten_module_and_the_bits_stay() {
    for marker in [Marker::Line, Marker::NoLine, Marker::LineBeforeLabel] {
        let words = marked(&locals(&[(7.0, NAN)]), marker, 2);
        let lowered = lower(&words).unwrap();
        assert!(matches!(lowered, Cow::Owned(_)));
        assert!(!has_line_marker(&lowered));
        assert!(has_constant(&lowered, NAN));
    }
}

/// The module with `%dbg = OpExtInstImport` of the shader debug-info set, a module-scope
/// `%none = OpExtInst %void %dbg 0`, the extension that allows them, a name for `%none` and a
/// name for the local variable.
struct Debugged {
    words: Vec<u32>,
    dbg: u32,
    none: u32,
    local: u32,
    glsl: u32,
}

fn with_debug_records(words: &[u32]) -> Debugged {
    let mut spirv = Spirv::new(words);
    let (dbg, none) = (spirv.fresh_id(), spirv.fresh_id());
    let (local, glsl, void) = (
        spirv.local_variable(),
        spirv.result_of(Op::ExtInstImport),
        spirv.result_of(Op::TypeVoid),
    );
    let extension = text("SPV_KHR_non_semantic_info");
    spirv.insert(spirv.last(Op::Capability) + 1, Op::Extension, &extension);
    let import = [&[dbg][..], &text(DEBUG_INFO_SET)].concat();
    spirv.insert(spirv.first(Op::MemoryModel), Op::ExtInstImport, &import);
    for (target, name) in [(none, "debug sentinel"), (local, "local")] {
        let operands = [&[target][..], &text(name)].concat();
        spirv.insert(spirv.after_debug(), Op::Name, &operands);
    }
    spirv.insert(
        spirv.first(Op::Function),
        Op::ExtInst,
        &[void, none, dbg, 0],
    );
    Debugged {
        words: spirv.words(),
        dbg,
        none,
        local,
        glsl,
    }
}

fn loaded(words: &[u32]) -> Module {
    rspirv::dr::load_words(words).unwrap()
}

fn names(module: &Module) -> Vec<(u32, String)> {
    module
        .debug_names
        .iter()
        .filter(|i| i.class.opcode == Op::Name)
        .filter_map(|i| match i.operands.as_slice() {
            [Operand::IdRef(target), Operand::LiteralString(name)] => Some((*target, name.clone())),
            _ => None,
        })
        .collect()
}

fn defined(module: &Module) -> BTreeSet<u32> {
    module.all_inst_iter().filter_map(|i| i.result_id).collect()
}

fn mentions(module: &Module, id: u32) -> bool {
    module
        .all_inst_iter()
        .any(|i| i.result_type == Some(id) || i.operands.contains(&Operand::IdRef(id)))
}

#[test]
fn naga_accepts_a_module_level_debug_info_instruction() {
    let debugged = with_debug_records(&locals(&[(7.0, NAN)]));
    parse(&debugged.words).unwrap();
}

#[test]
fn debug_info_records_are_dropped_and_the_rest_of_the_module_is_rewritten() {
    let debugged = with_debug_records(&locals(&[(7.0, NAN)]));
    admitted(&debugged.words);
    let lowered = lower(&debugged.words).unwrap();
    let module = loaded(&lowered);
    let names = names(&module);
    let ids = defined(&module);
    assert!(names.iter().all(|(target, _)| ids.contains(target)));
    assert!(names.contains(&(debugged.local, "local".into())));
    assert!(names.iter().all(|(_, name)| name != "debug sentinel"));
    assert!(!mentions(&module, debugged.none));
    assert!(
        !module.all_inst_iter().any(|i| i.class.opcode == Op::ExtInst
            && i.operands.first() == Some(&Operand::IdRef(debugged.dbg)))
    );
    assert!(
        module.all_inst_iter().any(|i| i.class.opcode == Op::ExtInst
            && i.operands[..2]
                == [
                    Operand::IdRef(debugged.glsl),
                    Operand::LiteralExtInstInteger(FABS)
                ]),
        "the GLSL.std.450 instruction is gone"
    );
    assert!(ids.contains(&debugged.dbg), "the import is kept");
}

/// `DebugNoScope`, a debug-info record with no operands that a compiler writes inside blocks.
const DEBUG_NO_SCOPE: u32 = 24;

#[test]
fn a_debug_info_record_inside_a_function_is_dropped() {
    let debugged = with_debug_records(&locals(&[(7.0, NAN)]));
    let mut spirv = Spirv::new(&debugged.words);
    let (record, void) = (spirv.fresh_id(), spirv.result_of(Op::TypeVoid));
    let operands = [void, record, debugged.dbg, DEBUG_NO_SCOPE];
    spirv.insert(spirv.first(Op::Return), Op::ExtInst, &operands);
    let words = spirv.words();
    parse(&words).unwrap_or_else(|error| panic!("naga refuses the fixture: {error}"));
    admitted(&words);
    let module = loaded(&lower(&words).unwrap());
    assert!(!defined(&module).contains(&record));
    assert!(!mentions(&module, record));
}

#[test]
fn a_debug_bearing_module_with_only_finite_constants_is_returned_word_for_word() {
    let debugged = with_debug_records(&locals(&[]));
    assert_unchanged(&debugged.words);
}

/// A function-level instruction of another `NonSemantic` set that takes the module-level debug
/// result `none` as an operand, placed before the entry block's return.
fn with_other_set_user(words: &[u32], none: u32) -> Vec<u32> {
    let mut spirv = Spirv::new(words);
    let (other, result) = (spirv.fresh_id(), spirv.fresh_id());
    let void = spirv.result_of(Op::TypeVoid);
    let import = [&[other][..], &text(OTHER_SET)].concat();
    spirv.insert(spirv.first(Op::MemoryModel), Op::ExtInstImport, &import);
    spirv.insert(
        spirv.first(Op::Return),
        Op::ExtInst,
        &[void, result, other, 1, none],
    );
    spirv.words()
}

#[test]
fn a_dropped_debug_result_that_another_instruction_uses_leaves_the_module_unchanged() {
    let debugged = with_debug_records(&locals(&[(7.0, NAN)]));
    let words = with_other_set_user(&debugged.words, debugged.none);
    assert_unchanged(&words);
}
