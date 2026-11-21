//! Snapshots of the body values a loop's `continuing` block uses.
//!
//! naga evaluates an expression where the `Emit` that covers it runs (a statement's result,
//! where the statement runs), and a `Loop`'s `continuing` block may use what its `body`
//! computed. naga 30's Metal writer prints `continuing` at the top of the next iteration,
//! before the body, and prints a body expression it has not named yet inline. A `Load` there is
//! evaluated again at its use, so when `continuing` stores to the variable first, the use reads
//! the new value. DXC writes exactly that when a continue block computes one phi's next value
//! from another phi: the phi stores come first, and terrain block compression packed every BC
//! index three bits high on Metal. naga's trunk fixes its Metal writer (`back/msl/hoist.rs`,
//! upstream issue gfx-rs/wgpu#4558); no release has the fix yet.
//!
//! This pass gives the same result in the IR, for every backend. Each body value that
//! `continuing` or `break_if` uses and whose value depends on when it is evaluated is stored
//! into a function local right after the body computes it. `continuing` starts by loading those
//! locals and computing again, from the loads, the body's arithmetic and addresses that lead
//! from them to its uses; its own expressions that use any of these are rebuilt over the new
//! handles, because naga requires an expression's operands to come before it in the arena. The
//! expressions they replace are no longer emitted.
//!
//! Delete this module when the engine moves to a naga release with the Metal writer's hoist fix.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use naga::proc::TypeResolution;
use naga::valid::{FunctionInfo, ModuleInfo, TypeFlags};
use naga::{
    AtomicFunction, Block, Expression, Function, GatherMode, Handle, ImageQuery, LocalVariable,
    Module, Range, RayPipelineFunction, RayQueryFunction, SampleLevel, Span, Statement, Type,
    TypeInner, UniqueArena,
};

use crate::module::ModuleError;

/// The name of the locals the pass adds.
const SNAPSHOT: &str = "continuing_snapshot";

type Expr = Handle<Expression>;

/// Gives every loop of `module` snapshots of the body values its `continuing` block and
/// `break_if` use. `info` is `module` validated; validate the result again when it changed.
///
/// # Errors
/// `ContinuingSnapshot`, naming the function and loop, for a value no local can hold.
pub(crate) fn snapshot_continuing_uses(
    module: &mut Module,
    info: &ModuleInfo,
) -> Result<bool, ModuleError> {
    let Module {
        types,
        functions,
        entry_points,
        ..
    } = module;
    let mut changed = false;
    for (handle, function) in functions.iter_mut() {
        let name = function
            .name
            .clone()
            .unwrap_or_else(|| format!("#{}", handle.index()));
        changed |= Pass::new(types, info, &info[handle], name).run(function)?;
    }
    for (index, entry) in entry_points.iter_mut().enumerate() {
        let info_entry = info.get_entry_point(index);
        changed |=
            Pass::new(types, info, info_entry, entry.name.clone()).run(&mut entry.function)?;
    }
    Ok(changed)
}

/// The pass over one function.
struct Pass<'a> {
    types: &'a mut UniqueArena<Type>,
    module: &'a ModuleInfo,
    function: &'a FunctionInfo,
    name: String,
    /// Loops met so far, in the order their statements start.
    loops: usize,
    changed: bool,
}

impl<'a> Pass<'a> {
    fn new(
        types: &'a mut UniqueArena<Type>,
        module: &'a ModuleInfo,
        function: &'a FunctionInfo,
        name: String,
    ) -> Self {
        Self {
            types,
            module,
            function,
            name,
            loops: 0,
            changed: false,
        }
    }

    fn run(mut self, function: &mut Function) -> Result<bool, ModuleError> {
        let mut body = std::mem::take(&mut function.body);
        let walked = self.block(&mut body, function);
        function.body = body;
        walked.map(|()| self.changed)
    }

    /// Inner loops first, so that every value a loop snapshots is one validation typed.
    fn block(&mut self, block: &mut Block, function: &mut Function) -> Result<(), ModuleError> {
        for statement in block.iter_mut() {
            match statement {
                Statement::Loop {
                    body,
                    continuing,
                    break_if,
                } => {
                    let index = self.loops;
                    self.loops += 1;
                    self.block(body, function)?;
                    self.block(continuing, function)?;
                    self.snapshot(index, body, continuing, break_if, function)?;
                }
                Statement::Block(inner) => self.block(inner, function)?,
                Statement::If { accept, reject, .. } => {
                    self.block(accept, function)?;
                    self.block(reject, function)?;
                }
                Statement::Switch { cases, .. } => {
                    for case in cases {
                        self.block(&mut case.body, function)?;
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn snapshot(
        &mut self,
        index: usize,
        body: &mut Block,
        continuing: &mut Block,
        break_if: &mut Option<Expr>,
        function: &mut Function,
    ) -> Result<(), ModuleError> {
        let defined = definitions(body);
        let mut roots = Vec::new();
        uses(continuing, function, &mut roots);
        roots.extend(*break_if);
        let (timed, recomputed) = reached(&roots, &defined, function);
        if timed.is_empty() {
            return Ok(());
        }
        let mut map = HashMap::new();
        let mut prologue = Vec::new();
        let mut stores: BTreeMap<usize, Vec<Statement>> = BTreeMap::new();
        for &value in &timed {
            let ty = self.local_type(index, value)?;
            let local = function.local_variables.append(
                LocalVariable {
                    name: Some(SNAPSHOT.to_owned()),
                    ty,
                    init: None,
                },
                Span::UNDEFINED,
            );
            let span = function.expressions.get_span(value);
            let pointer = function
                .expressions
                .append(Expression::LocalVariable(local), span);
            stores
                .entry(defined[&value])
                .or_default()
                .push(Statement::Store { pointer, value });
            prologue.push((value, Expression::Load { pointer }, span));
        }
        for (at, statements) in stores.into_iter().rev() {
            let after = at + 1;
            body.splice(after..after, Block::from_vec(statements));
        }
        let start = function.expressions.len();
        for (value, load, span) in prologue {
            map.insert(value, function.expressions.append(load, span));
        }
        for &value in &recomputed {
            let span = function.expressions.get_span(value);
            let expression = remapped(&function.expressions[value], &map);
            map.insert(value, function.expressions.append(expression, span));
        }
        let mut rebuilt = Block::new();
        rebuilt.push(
            Statement::Emit(function.expressions.range_from(start)),
            Span::UNDEFINED,
        );
        rebuilt.extend_block(rebuild(std::mem::take(continuing), &mut map, function));
        *continuing = rebuilt;
        remap(break_if.as_mut(), &map);
        self.changed = true;
        Ok(())
    }

    /// The type of a local that holds `value`.
    fn local_type(&mut self, index: usize, value: Expr) -> Result<Handle<Type>, ModuleError> {
        let refused = |what: String| ModuleError::ContinuingSnapshot {
            function: self.name.clone(),
            loop_index: index,
            what,
        };
        match self.function[value].ty {
            TypeResolution::Handle(ty) if self.module[ty].contains(TypeFlags::CONSTRUCTIBLE) => {
                Ok(ty)
            }
            TypeResolution::Handle(ty) => Err(refused(format!(
                "expression [{}] of type {:?}, which a local cannot hold",
                value.index(),
                self.types[ty].inner
            ))),
            TypeResolution::Value(
                ref inner @ (TypeInner::Scalar(_)
                | TypeInner::Vector { .. }
                | TypeInner::Matrix { .. }),
            ) => Ok(self.types.insert(
                Type {
                    name: None,
                    inner: inner.clone(),
                },
                Span::UNDEFINED,
            )),
            TypeResolution::Value(ref inner) => Err(refused(format!(
                "expression [{}] of type {inner:?}, which a local cannot hold",
                value.index()
            ))),
        }
    }
}

/// The expressions `body` itself computes, with the index of the statement that computes each.
fn definitions(body: &Block) -> HashMap<Expr, usize> {
    let mut defined = HashMap::new();
    for (at, statement) in body.iter().enumerate() {
        match statement {
            Statement::Emit(range) => defined.extend(range.clone().map(|value| (value, at))),
            other => defined.extend(statement_result(other).map(|value| (value, at))),
        }
    }
    defined
}

/// The expression a statement produces as its result, if any.
fn statement_result(statement: &Statement) -> Option<Expr> {
    match *statement {
        Statement::Atomic { result, .. } | Statement::Call { result, .. } => result,
        Statement::RayQuery {
            fun: RayQueryFunction::Proceed { result },
            ..
        }
        | Statement::WorkGroupUniformLoad { result, .. }
        | Statement::SubgroupBallot { result, .. }
        | Statement::SubgroupGather { result, .. }
        | Statement::SubgroupCollectiveOperation { result, .. } => Some(result),
        _ => None,
    }
}

/// Every expression `block` refers to: the operands of the expressions it emits and the
/// expressions its statements name, in nested blocks too.
fn uses(block: &Block, function: &Function, found: &mut Vec<Expr>) {
    for statement in block {
        match statement {
            Statement::Emit(range) => {
                for value in range.clone() {
                    found.extend(operands(&function.expressions[value]));
                }
            }
            Statement::Block(inner) => uses(inner, function, found),
            Statement::If { accept, reject, .. } => {
                uses(accept, function, found);
                uses(reject, function, found);
            }
            Statement::Switch { cases, .. } => {
                for case in cases {
                    uses(&case.body, function, found);
                }
            }
            Statement::Loop {
                body, continuing, ..
            } => {
                uses(body, function, found);
                uses(continuing, function, found);
            }
            _ => {}
        }
        let mut statement = statement_operands_only(statement);
        each_statement_operand(&mut statement, &mut |value| found.push(*value));
    }
}

/// The body values reached from `roots` through the body's own expressions: those whose value
/// depends on when they are evaluated, which get snapshots, and the others that depend on one
/// of them, which `continuing` computes again. Both ascend, so operands come first.
fn reached(
    roots: &[Expr],
    defined: &HashMap<Expr, usize>,
    function: &Function,
) -> (BTreeSet<Expr>, Vec<Expr>) {
    let mut timed = BTreeSet::new();
    let mut pure = BTreeSet::new();
    let mut pending = roots.to_vec();
    while let Some(value) = pending.pop() {
        if !defined.contains_key(&value) || timed.contains(&value) || pure.contains(&value) {
            continue;
        }
        let expression = &function.expressions[value];
        if timing_matters(expression) {
            timed.insert(value);
        } else {
            pure.insert(value);
            pending.extend(operands(expression));
        }
    }
    let mut recomputed: Vec<Expr> = Vec::new();
    for value in pure {
        let depends = operands(&function.expressions[value])
            .iter()
            .any(|operand| timed.contains(operand) || recomputed.contains(operand));
        if depends {
            recomputed.push(value);
        }
    }
    (timed, recomputed)
}

/// Rebuilds `block` with every handle in `map` replaced, emitting new copies of the expressions
/// that use one, where the originals were emitted.
fn rebuild(block: Block, map: &mut HashMap<Expr, Expr>, function: &mut Function) -> Block {
    let mut out = Block::with_capacity(block.len());
    for (statement, span) in block.span_into_iter() {
        let statement = match statement {
            Statement::Emit(range) => {
                for emit in reemitted(range, map, function) {
                    out.push(emit, span);
                }
                continue;
            }
            Statement::Block(inner) => Statement::Block(rebuild(inner, map, function)),
            Statement::If {
                condition,
                accept,
                reject,
            } => Statement::If {
                condition: map.get(&condition).copied().unwrap_or(condition),
                accept: rebuild(accept, map, function),
                reject: rebuild(reject, map, function),
            },
            Statement::Switch { selector, cases } => Statement::Switch {
                selector: map.get(&selector).copied().unwrap_or(selector),
                cases: cases
                    .into_iter()
                    .map(|case| naga::SwitchCase {
                        body: rebuild(case.body, map, function),
                        ..case
                    })
                    .collect(),
            },
            Statement::Loop {
                body,
                continuing,
                mut break_if,
            } => {
                let body = rebuild(body, map, function);
                let continuing = rebuild(continuing, map, function);
                remap(break_if.as_mut(), map);
                Statement::Loop {
                    body,
                    continuing,
                    break_if,
                }
            }
            mut other => {
                each_statement_operand(&mut other, &mut |value| remap(Some(value), map));
                other
            }
        };
        out.push(statement, span);
    }
    out
}

/// The `Emit`s that replace `Emit(range)`: the expressions that use nothing in `map`, as they
/// were, then new copies of the others over the mapped handles.
fn reemitted(
    range: Range<Expression>,
    map: &mut HashMap<Expr, Expr>,
    function: &mut Function,
) -> Vec<Statement> {
    let start = function.expressions.len();
    let mut kept: Vec<Expr> = Vec::new();
    for value in range.clone() {
        let expression = &function.expressions[value];
        if operands(expression)
            .iter()
            .any(|operand| map.contains_key(operand))
        {
            let copy = remapped(expression, map);
            let span = function.expressions.get_span(value);
            map.insert(value, function.expressions.append(copy, span));
        } else {
            kept.push(value);
        }
    }
    if function.expressions.len() == start {
        return vec![Statement::Emit(range)];
    }
    let mut emits = Vec::new();
    let mut run: Option<(Expr, Expr)> = None;
    for value in kept {
        run = match run {
            Some((first, last)) if last.index() + 1 == value.index() => Some((first, value)),
            Some((first, last)) => {
                emits.push(Statement::Emit(Range::new_from_bounds(first, last)));
                Some((value, value))
            }
            None => Some((value, value)),
        };
    }
    if let Some((first, last)) = run {
        emits.push(Statement::Emit(Range::new_from_bounds(first, last)));
    }
    emits.push(Statement::Emit(function.expressions.range_from(start)));
    emits
}

fn remap(value: Option<&mut Expr>, map: &HashMap<Expr, Expr>) {
    if let Some(value) = value
        && let Some(&mapped) = map.get(value)
    {
        *value = mapped;
    }
}

fn remapped(expression: &Expression, map: &HashMap<Expr, Expr>) -> Expression {
    let mut copy = expression.clone();
    each_operand(&mut copy, &mut |value| remap(Some(value), map));
    copy
}

/// Whether the value of `expression` depends on when it is evaluated, after naga's evaluation
/// rules (`naga::ir`, "Expression evaluation time"): loads, image reads, derivatives and
/// statement results. The rest computes the same value from the same operands anywhere.
const fn timing_matters(expression: &Expression) -> bool {
    match *expression {
        Expression::Load { .. }
        | Expression::ImageSample { .. }
        | Expression::ImageLoad { .. }
        | Expression::Derivative { .. }
        | Expression::CallResult(_)
        | Expression::AtomicResult { .. }
        | Expression::WorkGroupUniformLoadResult { .. }
        | Expression::RayQueryProceedResult
        | Expression::RayQueryGetIntersection { .. }
        | Expression::RayQueryVertexPositions { .. }
        | Expression::SubgroupBallotResult
        | Expression::SubgroupOperationResult { .. }
        | Expression::CooperativeLoad { .. } => true,
        Expression::Literal(_)
        | Expression::Constant(_)
        | Expression::Override(_)
        | Expression::ZeroValue(_)
        | Expression::Compose { .. }
        | Expression::Access { .. }
        | Expression::AccessIndex { .. }
        | Expression::Splat { .. }
        | Expression::Swizzle { .. }
        | Expression::FunctionArgument(_)
        | Expression::GlobalVariable(_)
        | Expression::LocalVariable(_)
        | Expression::ImageQuery { .. }
        | Expression::Unary { .. }
        | Expression::Binary { .. }
        | Expression::Select { .. }
        | Expression::Relational { .. }
        | Expression::Math { .. }
        | Expression::As { .. }
        | Expression::ArrayLength(_)
        | Expression::CooperativeMultiplyAdd { .. } => false,
    }
}

fn operands(expression: &Expression) -> Vec<Expr> {
    let mut found = Vec::new();
    each_operand(&mut expression.clone(), &mut |value| found.push(*value));
    found
}

/// Calls `visit` with each expression `expression` takes as an operand.
fn each_operand(expression: &mut Expression, visit: &mut dyn FnMut(&mut Expr)) {
    match expression {
        Expression::Literal(_)
        | Expression::Constant(_)
        | Expression::Override(_)
        | Expression::ZeroValue(_)
        | Expression::FunctionArgument(_)
        | Expression::GlobalVariable(_)
        | Expression::LocalVariable(_)
        | Expression::CallResult(_)
        | Expression::AtomicResult { .. }
        | Expression::WorkGroupUniformLoadResult { .. }
        | Expression::RayQueryProceedResult
        | Expression::SubgroupBallotResult
        | Expression::SubgroupOperationResult { .. } => {}
        Expression::Compose { components, .. } => components.iter_mut().for_each(visit),
        Expression::Access { base, index } => [base, index].into_iter().for_each(visit),
        Expression::AccessIndex { base: value, .. }
        | Expression::Splat { value, .. }
        | Expression::Swizzle { vector: value, .. }
        | Expression::Load { pointer: value }
        | Expression::Unary { expr: value, .. }
        | Expression::Derivative { expr: value, .. }
        | Expression::Relational {
            argument: value, ..
        }
        | Expression::As { expr: value, .. }
        | Expression::ArrayLength(value)
        | Expression::RayQueryVertexPositions { query: value, .. }
        | Expression::RayQueryGetIntersection { query: value, .. } => visit(value),
        Expression::ImageSample {
            image,
            sampler,
            coordinate,
            array_index,
            offset,
            level,
            depth_ref,
            ..
        } => {
            [image, sampler, coordinate]
                .into_iter()
                .for_each(&mut *visit);
            [array_index, offset, depth_ref]
                .into_iter()
                .flatten()
                .for_each(&mut *visit);
            match level {
                SampleLevel::Auto | SampleLevel::Zero => {}
                SampleLevel::Exact(value) | SampleLevel::Bias(value) => visit(value),
                SampleLevel::Gradient { x, y } => [x, y].into_iter().for_each(visit),
            }
        }
        Expression::ImageLoad {
            image,
            coordinate,
            array_index,
            sample,
            level,
        } => {
            [image, coordinate].into_iter().for_each(&mut *visit);
            [array_index, sample, level]
                .into_iter()
                .flatten()
                .for_each(visit);
        }
        Expression::ImageQuery { image, query } => {
            visit(image);
            if let ImageQuery::Size { level: Some(level) } = query {
                visit(level);
            }
        }
        Expression::Binary { left, right, .. } => [left, right].into_iter().for_each(visit),
        Expression::Select {
            condition,
            accept,
            reject,
        } => [condition, accept, reject].into_iter().for_each(visit),
        Expression::Math {
            arg,
            arg1,
            arg2,
            arg3,
            ..
        } => {
            visit(arg);
            [arg1, arg2, arg3].into_iter().flatten().for_each(visit);
        }
        Expression::CooperativeLoad { data, .. } => {
            [&mut data.pointer, &mut data.stride]
                .into_iter()
                .for_each(visit);
        }
        Expression::CooperativeMultiplyAdd { a, b, c } => [a, b, c].into_iter().for_each(visit),
    }
}

/// `statement` without its nested blocks, which `uses` walks on its own.
fn statement_operands_only(statement: &Statement) -> Statement {
    match *statement {
        Statement::Emit(_) | Statement::Block(_) => Statement::Break,
        Statement::If { condition, .. } => Statement::If {
            condition,
            accept: Block::new(),
            reject: Block::new(),
        },
        Statement::Switch { selector, .. } => Statement::Switch {
            selector,
            cases: Vec::new(),
        },
        Statement::Loop { break_if, .. } => Statement::Loop {
            body: Block::new(),
            continuing: Block::new(),
            break_if,
        },
        ref other => other.clone(),
    }
}

/// Calls `visit` with each expression `statement` names itself, not in its nested blocks.
fn each_statement_operand(statement: &mut Statement, visit: &mut dyn FnMut(&mut Expr)) {
    match statement {
        Statement::Emit(_)
        | Statement::Block(_)
        | Statement::Break
        | Statement::Continue
        | Statement::Kill
        | Statement::ControlBarrier(_)
        | Statement::MemoryBarrier(_) => {}
        Statement::If {
            condition: value, ..
        }
        | Statement::Switch {
            selector: value, ..
        }
        | Statement::WorkGroupUniformLoad {
            pointer: value,
            result: _,
        } => visit(value),
        Statement::Loop { break_if, .. } => break_if.iter_mut().for_each(visit),
        Statement::Return { value } => value.iter_mut().for_each(visit),
        Statement::Store { pointer, value } => [pointer, value].into_iter().for_each(visit),
        Statement::ImageStore {
            image,
            coordinate,
            array_index,
            value,
        } => {
            [image, coordinate, value].into_iter().for_each(&mut *visit);
            array_index.iter_mut().for_each(visit);
        }
        Statement::Atomic {
            pointer,
            fun,
            value,
            result: _,
        } => {
            [pointer, value].into_iter().for_each(&mut *visit);
            atomic_operand(fun, visit);
        }
        Statement::ImageAtomic {
            image,
            coordinate,
            array_index,
            fun,
            value,
        } => {
            [image, coordinate, value].into_iter().for_each(&mut *visit);
            array_index.iter_mut().for_each(&mut *visit);
            atomic_operand(fun, visit);
        }
        Statement::Call { arguments, .. } => arguments.iter_mut().for_each(visit),
        Statement::RayQuery { query, fun } => {
            visit(query);
            match fun {
                RayQueryFunction::Initialize {
                    acceleration_structure,
                    descriptor,
                } => [acceleration_structure, descriptor]
                    .into_iter()
                    .for_each(visit),
                RayQueryFunction::GenerateIntersection { hit_t } => visit(hit_t),
                RayQueryFunction::Proceed { .. }
                | RayQueryFunction::ConfirmIntersection
                | RayQueryFunction::Terminate => {}
            }
        }
        Statement::RayPipelineFunction(RayPipelineFunction::TraceRay {
            acceleration_structure,
            descriptor,
            payload,
        }) => [acceleration_structure, descriptor, payload]
            .into_iter()
            .for_each(visit),
        Statement::SubgroupBallot { predicate, .. } => predicate.iter_mut().for_each(visit),
        Statement::SubgroupGather { mode, argument, .. } => {
            visit(argument);
            gather_operand(mode, visit);
        }
        Statement::SubgroupCollectiveOperation { argument, .. } => visit(argument),
        Statement::CooperativeStore { target, data } => {
            [target, &mut data.pointer, &mut data.stride]
                .into_iter()
                .for_each(visit);
        }
    }
}

fn atomic_operand(fun: &mut AtomicFunction, visit: &mut dyn FnMut(&mut Expr)) {
    if let AtomicFunction::Exchange {
        compare: Some(compare),
    } = fun
    {
        visit(compare);
    }
}

fn gather_operand(mode: &mut GatherMode, visit: &mut dyn FnMut(&mut Expr)) {
    match mode {
        GatherMode::BroadcastFirst | GatherMode::QuadSwap(_) => {}
        GatherMode::Broadcast(value)
        | GatherMode::Shuffle(value)
        | GatherMode::ShuffleDown(value)
        | GatherMode::ShuffleUp(value)
        | GatherMode::ShuffleXor(value)
        | GatherMode::QuadBroadcast(value) => visit(value),
    }
}
