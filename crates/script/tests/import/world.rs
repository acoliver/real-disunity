//! Builds the stack and `CallInfo` records Lua 5.3 holds while a coroutine is stopped in a
//! yielding native function of `fixtures/cases.lua`, in the data model `lua_resume::import`
//! reads. The prototypes come from the stock Lua 5.3 compile of that file, `cases.luac`.

use lua_resume::capture::{Cell, Value};
use lua_resume::import::{SavedClosure, SavedFrame, SavedHeap, SavedThread, status};
use lua_resume::lua53::chunk::read_chunk;
use lua_resume::lua53::op::{call_kind, register_a};

pub const SOURCE: &str = include_str!("../fixtures/cases.lua");
const COMPILED: &[u8] = include_bytes!("../fixtures/cases.luac");
/// The chunk name the 5.3 build saved for the file.
pub const SAVED_SOURCE: &[u8] = b"@Lua/cases.lua";

pub struct World {
    pub heap: SavedHeap,
    pub thread: SavedThread,
    next_func: usize,
}

/// The line `function name(` or `local function name(` starts on.
fn line_of(name: &str) -> u32 {
    let line = SOURCE
        .lines()
        .position(|line| {
            line.trim_start()
                .trim_start_matches("local ")
                .starts_with(&format!("function {name}("))
        })
        .unwrap_or_else(|| panic!("no function {name}"));
    u32::try_from(line).unwrap() + 1
}

impl World {
    /// A heap with the prototypes of `cases.luac` and cell 0 holding `_G`.
    pub fn new() -> Self {
        let chunk = read_chunk(COMPILED).unwrap();
        let mut heap = SavedHeap {
            protos: chunk.main.flatten().iter().map(|f| f.0.erased()).collect(),
            ..SavedHeap::default()
        };
        heap.cells.push(Cell {
            value: Value::Permanent("_G".to_owned()),
        });
        Self {
            heap,
            thread: SavedThread {
                stack: vec![Value::Nil],
                ..SavedThread::default()
            },
            next_func: 1,
        }
    }

    pub fn cell(&mut self, value: Value) -> usize {
        self.heap.cells.push(Cell { value });
        self.heap.cells.len() - 1
    }

    /// A closure of the function called `name`; `_ENV` shares cell 0 and the other upvalues
    /// are taken from `cells` by name.
    pub fn closure(&mut self, name: &str, cells: &[(&str, usize)]) -> usize {
        let line = line_of(name);
        let proto = self
            .heap
            .protos
            .iter()
            .position(|p| p.line_defined == line)
            .unwrap_or_else(|| panic!("no prototype at line {line}"));
        let upvalues = self.heap.protos[proto]
            .upvalues
            .iter()
            .map(|upvalue| {
                let upvalue = String::from_utf8_lossy(&upvalue.name).into_owned();
                if upvalue == "_ENV" {
                    return 0;
                }
                cells
                    .iter()
                    .find(|(found, _)| *found == upvalue)
                    .unwrap_or_else(|| panic!("no cell for upvalue {upvalue}"))
                    .1
            })
            .collect();
        self.heap.closures.push(SavedClosure { proto, upvalues });
        self.heap.closures.len() - 1
    }

    /// Pushes the frame running `closure`, stopped at the first call. `registers` are the
    /// slots below the callee and `callee` the value in the callee register.
    pub fn frame(&mut self, closure: usize, registers: &[Value], callee: Value) {
        let proto = &self.heap.protos[self.heap.closures[closure].proto];
        let func = self.next_func;
        let pc = proto
            .code
            .iter()
            .position(|word| call_kind(*word).is_some())
            .expect("a call");
        let window = usize::from(register_a(proto.code[pc]));
        assert_eq!(registers.len(), window, "registers below the callee");
        self.thread.stack.truncate(func);
        self.thread.stack.push(Value::Closure(closure));
        self.thread.stack.extend(registers.iter().cloned());
        self.thread.stack.push(callee);
        self.next_func = func + 1 + window;
        self.thread.frames.push(SavedFrame {
            func,
            base: func + 1,
            saved_pc: pc + 1,
            call_status: status::LUA,
            continuation: None,
        });
    }

    /// The yielding native on top of the innermost Lua frame.
    pub fn yielded(&mut self) {
        self.thread.frames.push(SavedFrame {
            func: self.next_func,
            base: 0,
            saved_pc: 0,
            call_status: 0,
            continuation: None,
        });
    }

    /// The world of `nested(5)` stopped in the `coroutine.yield` of `leaf`.
    pub fn nested_in_leaf() -> Self {
        let mut world = Self::new();
        let leaf_cell = world.cell(Value::Nil);
        let leaf = world.closure("leaf", &[]);
        world.heap.cells[leaf_cell].value = Value::Closure(leaf);
        let nested = world.closure("nested", &[("leaf", leaf_cell)]);
        world.frame(nested, &[Value::Integer(5)], Value::Closure(leaf));
        world.frame(
            leaf,
            &[Value::Integer(5)],
            Value::Permanent("coroutine.yield".to_owned()),
        );
        world.yielded();
        world
    }
}
