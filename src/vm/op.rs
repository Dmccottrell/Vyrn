//! The bytecode instruction set.
//!
//! The interpreter is a register machine. Every call frame owns a window of
//! up to 256 value slots ("registers"), and each instruction names the slots
//! it reads and writes directly: `a + b * c` is two instructions, `Mul` and
//! `Add`, instead of the five pushes, pops and operations a stack machine
//! needs. Fewer instructions means fewer trips through the dispatch loop,
//! which is where an interpreter spends most of its time.
//!
//! Each instruction is exactly four bytes: a one-byte opcode and up to three
//! one-byte register operands, or one register and a 16-bit constant index or
//! jump offset. A function's code is a dense array that stays in cache.
//!
//! Conventions:
//! - `dst` is written; every other register operand is read.
//! - Jump offsets are relative to the instruction after the jump.
//! - `k` indexes the function's constant pool, `name` its name table, and
//!   `idx` in `GetGlobal`/`SetGlobal` the program's global table.

/// One bytecode instruction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Op {
    // ---- loads and moves ----
    Move {
        dst: u8,
        src: u8,
    },
    LoadConst {
        dst: u8,
        k: u16,
    },
    /// A small integer stored in the instruction itself.
    LoadInt {
        dst: u8,
        value: i16,
    },
    LoadNone {
        dst: u8,
    },
    LoadBool {
        dst: u8,
        value: bool,
    },
    /// Marks an argument the caller left out, so the callee's prologue can
    /// fill in the parameter's default value.
    LoadMissing {
        dst: u8,
    },
    GetGlobal {
        dst: u8,
        idx: u16,
    },
    SetGlobal {
        src: u8,
        idx: u16,
    },
    /// Reads a variable captured by the running closure.
    GetCapture {
        dst: u8,
        idx: u8,
    },
    /// Moves the value in `reg` into a new shared cell, for a variable that
    /// closures capture.
    MakeCell {
        reg: u8,
    },
    GetCell {
        dst: u8,
        cell: u8,
    },
    SetCell {
        cell: u8,
        src: u8,
    },
    /// Reads a captured variable held in a shared cell.
    GetUpval {
        dst: u8,
        idx: u8,
    },
    SetUpval {
        idx: u8,
        src: u8,
    },
    /// Creates a closure over `proto`, capturing variables from this frame.
    MakeClosure {
        dst: u8,
        proto: u16,
    },

    // ---- arithmetic: integers, floats, strings, lists and tensors ----
    Add {
        dst: u8,
        a: u8,
        b: u8,
    },
    Sub {
        dst: u8,
        a: u8,
        b: u8,
    },
    /// Element-wise on tensors.
    Mul {
        dst: u8,
        a: u8,
        b: u8,
    },
    Div {
        dst: u8,
        a: u8,
        b: u8,
    },
    FloorDiv {
        dst: u8,
        a: u8,
        b: u8,
    },
    Rem {
        dst: u8,
        a: u8,
        b: u8,
    },
    Pow {
        dst: u8,
        a: u8,
        b: u8,
    },
    BitAnd {
        dst: u8,
        a: u8,
        b: u8,
    },
    BitOr {
        dst: u8,
        a: u8,
        b: u8,
    },
    BitXor {
        dst: u8,
        a: u8,
        b: u8,
    },
    Shl {
        dst: u8,
        a: u8,
        b: u8,
    },
    Shr {
        dst: u8,
        a: u8,
        b: u8,
    },
    Neg {
        dst: u8,
        src: u8,
    },
    Pos {
        dst: u8,
        src: u8,
    },
    BitNot {
        dst: u8,
        src: u8,
    },
    Not {
        dst: u8,
        src: u8,
    },

    // ---- comparison ----
    Eq {
        dst: u8,
        a: u8,
        b: u8,
    },
    NotEq {
        dst: u8,
        a: u8,
        b: u8,
    },
    Lt {
        dst: u8,
        a: u8,
        b: u8,
    },
    LtEq {
        dst: u8,
        a: u8,
        b: u8,
    },
    Gt {
        dst: u8,
        a: u8,
        b: u8,
    },
    GtEq {
        dst: u8,
        a: u8,
        b: u8,
    },
    /// `item in container`.
    Contains {
        dst: u8,
        item: u8,
        container: u8,
    },

    // ---- tensors and devices ----
    /// `a @ b`, executed by the tensor backend of the operands' device.
    MatMul {
        dst: u8,
        a: u8,
        b: u8,
    },
    /// `src on target`: moves tensors to a device, or shards a collection
    /// across a group of devices.
    Place {
        dst: u8,
        src: u8,
        target: u8,
    },
    /// Enters an `on device:` block: new tensors are created there.
    PushDevice {
        src: u8,
    },
    PopDevice,

    // ---- control flow ----
    Jump {
        offset: i16,
    },
    JumpIfFalse {
        cond: u8,
        offset: i16,
    },
    JumpIfTrue {
        cond: u8,
        offset: i16,
    },
    JumpIfNotNone {
        src: u8,
        offset: i16,
    },
    JumpIfNotMissing {
        src: u8,
        offset: i16,
    },
    /// Starts a counted loop. `base` holds a range; the instruction unpacks it
    /// into `base` (counter), `base+1` (limit), `base+2` (step) and the loop
    /// variable `base+3`, or jumps past the loop if the range is empty.
    ForPrep {
        base: u8,
        offset: i16,
    },
    /// Advances a counted loop and jumps back to its body while it runs.
    /// One instruction per iteration.
    ForLoop {
        base: u8,
        offset: i16,
    },
    IterInit {
        dst: u8,
        src: u8,
    },
    /// Stores the next item in `dst` and skips the following instruction;
    /// when the iterator is exhausted it falls through to that instruction,
    /// which is the jump out of the loop.
    IterNext {
        iter: u8,
        dst: u8,
    },

    // ---- calls ----
    /// Calls the value in `func` with `argc` arguments in the registers above
    /// it. The callee's frame starts at `func + 1`, so arguments are not
    /// copied, and the result replaces the function in `func`.
    Call {
        func: u8,
        argc: u8,
    },
    /// Like `Call`, with keyword arguments: the last arguments are matched
    /// to parameters by the names in the tuple in the register after them.
    CallKw {
        func: u8,
        argc: u8,
    },
    /// Like `CallMethod`, with keyword arguments named as for `CallKw`.
    CallMethodKw {
        base: u8,
        argc: u8,
        name: u8,
    },
    /// Calls method `name` on the receiver in `base + 1`, with `argc`
    /// arguments after it. The result goes to `base`.
    CallMethod {
        base: u8,
        argc: u8,
        name: u8,
    },
    Return {
        src: u8,
    },
    ReturnNone,

    // ---- collections and objects ----
    NewList {
        dst: u8,
        start: u8,
        count: u8,
    },
    NewTuple {
        dst: u8,
        start: u8,
        count: u8,
    },
    NewMap {
        dst: u8,
    },
    ListPush {
        list: u8,
        src: u8,
    },
    /// Builds a range from `base` (start), `base+1` (end) and `base+2` (step).
    MakeRange {
        dst: u8,
        base: u8,
        inclusive: bool,
    },
    /// Builds a slice from `base`, `base+1` and `base+2`; `none` means omitted.
    MakeSlice {
        dst: u8,
        base: u8,
    },
    GetIndex {
        dst: u8,
        obj: u8,
        index: u8,
    },
    /// Moves a list element out, leaving `none`, so it can be changed
    /// without a copy. Used by `for mut`, which has the list to itself
    /// until it puts the element back.
    TakeIndex {
        dst: u8,
        obj: u8,
        index: u8,
    },
    SetIndex {
        obj: u8,
        index: u8,
        src: u8,
    },
    GetField {
        dst: u8,
        obj: u8,
        name: u8,
    },
    SetField {
        obj: u8,
        name: u8,
        src: u8,
    },
    /// Reads a field by position, for pattern matching.
    GetSlot {
        dst: u8,
        obj: u8,
        slot: u8,
    },
    Len {
        dst: u8,
        src: u8,
    },
    /// Builds an instance of type `ty` from the frame's first registers,
    /// one per field. Used by generated constructors.
    MakeInstance {
        dst: u8,
        ty: u16,
    },
    /// Converts `count` registers from `start` to text and joins them.
    Concat {
        dst: u8,
        start: u8,
        count: u8,
    },

    // ---- patterns and failures ----
    /// Whether `src` has the type named `name` (or a base of it).
    IsType {
        dst: u8,
        src: u8,
        name: u8,
    },
    Fail {
        src: u8,
    },
    /// Installs a handler: a failure jumps to the target with the failure
    /// value in `err`.
    TryBegin {
        err: u8,
        offset: i16,
    },
    TryEnd,

    // ---- tasks ----
    Spawn {
        dst: u8,
        src: u8,
    },
    Await {
        dst: u8,
        src: u8,
    },
}

#[cfg(test)]
mod tests {
    use super::Op;

    #[test]
    fn instructions_are_four_bytes() {
        assert_eq!(std::mem::size_of::<Op>(), 4);
    }
}
