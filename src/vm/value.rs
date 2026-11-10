//! Runtime values.
//!
//! There is no garbage collector. Heap values are reference counted (`Rc`),
//! so memory is released the moment the last reference goes away, exactly as
//! the language's ownership model promises. Structs are value types: they
//! sit behind an `Rc` but are copied on write (`Rc::make_mut`), so sharing
//! one is cheap and changing one never affects another. Classes are
//! reference types and are shared on purpose.

use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt;
use std::rc::Rc;

use super::program::Proto;
use super::task::{Channel, TaskHandle};
use super::tensor::{Device, DeviceTensor, Tensor};
use super::tokenizer::Tokenizer;

/// An interned name: a field, method or type name.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Symbol(pub u32);

#[derive(Clone, Debug, Default)]
pub struct Interner {
    names: Vec<String>,
    index: HashMap<String, Symbol>,
}

impl Interner {
    pub fn intern(&mut self, name: &str) -> Symbol {
        if let Some(&symbol) = self.index.get(name) {
            return symbol;
        }
        let symbol = Symbol(self.names.len() as u32);
        self.names.push(name.to_string());
        self.index.insert(name.to_string(), symbol);
        symbol
    }

    pub fn name(&self, symbol: Symbol) -> &str {
        &self.names[symbol.0 as usize]
    }

    pub fn get(&self, name: &str) -> Option<Symbol> {
        self.index.get(name).copied()
    }
}

/// A value held in a register. Sixteen bytes (checked by a test).
#[derive(Clone, Debug)]
pub enum Value {
    None,
    Bool(bool),
    Int(i64),
    Float(f64),
    Str(Rc<String>),
    List(Rc<RefCell<Vec<Value>>>),
    Tuple(Rc<Vec<Value>>),
    Map(Rc<RefCell<Map>>),
    Range(Rc<RangeValue>),
    Slice(Rc<SliceValue>),
    Ellipsis,
    /// An instance of a struct or an enum variant. Copied on write.
    Struct(Rc<Instance>),
    /// An instance of a class. Shared.
    Object(Rc<RefCell<Instance>>),
    /// An enum type, used as a namespace for its variants.
    Type(u16),
    Function(Rc<Closure>),
    Native(u16),
    Tensor(Rc<DeviceTensor>),
    Device(Device),
    /// The builtin `gpu`, indexed to pick devices: `gpu[0]`, `gpu[0 to 3]`.
    Gpus,
    DeviceGroup(Rc<Vec<Device>>),
    /// A `region` arena. Placement onto it is accepted; memory is released
    /// by reference counting when the values are dropped.
    Region,
    Iter(Rc<RefCell<Iter>>),
    /// A running or finished task: what calling an `async fn` returns.
    Task(Rc<TaskHandle>),
    /// A queue between tasks.
    Channel(Rc<RefCell<Channel>>),
    /// A runtime error raised by the VM itself.
    Error(Rc<ErrorValue>),
    Tokenizer(Rc<Tokenizer>),
    /// A variable shared between a function and the closures inside it.
    /// Never visible to programs: reading the variable reads the cell.
    Cell(Rc<RefCell<Value>>),
    /// An argument the caller left out. Never visible to programs.
    Missing,
}

impl Value {
    pub fn str(text: impl Into<String>) -> Value {
        Value::Str(Rc::new(text.into()))
    }

    pub fn list(items: Vec<Value>) -> Value {
        Value::List(Rc::new(RefCell::new(items)))
    }

    pub fn tuple(items: Vec<Value>) -> Value {
        Value::Tuple(Rc::new(items))
    }

    pub fn tensor(tensor: Tensor) -> Value {
        Value::Tensor(Rc::new(DeviceTensor::new(tensor)))
    }

    /// The type name used in error messages and by `catch` and `is`.
    pub fn type_name(&self) -> &'static str {
        match self {
            Value::None => "none",
            Value::Bool(_) => "bool",
            Value::Int(_) => "int",
            Value::Float(_) => "float",
            Value::Str(_) => "str",
            Value::List(_) => "list",
            Value::Tuple(_) => "tuple",
            Value::Map(_) => "map",
            Value::Range(_) => "range",
            Value::Slice(_) => "slice",
            Value::Ellipsis => "ellipsis",
            Value::Struct(_) => "struct",
            Value::Object(_) => "object",
            Value::Type(_) => "type",
            Value::Function(_) | Value::Native(_) => "function",
            Value::Tensor(_) => "tensor",
            Value::Device(_) | Value::Gpus | Value::DeviceGroup(_) => "device",
            Value::Region => "region",
            Value::Iter(_) => "iterator",
            Value::Task(_) => "task",
            Value::Channel(_) => "channel",
            Value::Error(_) => "error",
            Value::Tokenizer(_) => "tokenizer",
            Value::Cell(_) => "cell",
            Value::Missing => "missing",
        }
    }

    /// Whether the value holds a reference to heap memory, which must be
    /// released when the value is dropped. Scalars own nothing.
    #[inline]
    pub fn owns_heap(&self) -> bool {
        !matches!(
            self,
            Value::None
                | Value::Bool(_)
                | Value::Int(_)
                | Value::Float(_)
                | Value::Ellipsis
                | Value::Type(_)
                | Value::Native(_)
                | Value::Device(_)
                | Value::Gpus
                | Value::Region
                | Value::Missing
        )
    }

    pub fn truthy(&self) -> bool {
        match self {
            Value::None | Value::Missing => false,
            Value::Bool(b) => *b,
            Value::Int(i) => *i != 0,
            Value::Float(f) => *f != 0.0,
            Value::Str(s) => !s.is_empty(),
            Value::List(l) => !l.borrow().is_empty(),
            Value::Tuple(t) => !t.is_empty(),
            Value::Map(m) => !m.borrow().is_empty(),
            _ => true,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RangeValue {
    pub start: i64,
    /// Exclusive, whatever the range was written with.
    pub stop: i64,
    pub step: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SliceValue {
    pub start: Option<i64>,
    pub stop: Option<i64>,
    pub step: Option<i64>,
}

#[derive(Clone, Debug)]
pub struct Instance {
    /// Index into the program's type table.
    pub ty: u16,
    pub fields: Vec<Value>,
}

pub struct Closure {
    pub proto: Rc<Proto>,
    pub captures: Vec<Value>,
}

impl fmt::Debug for Closure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "<fn {}>", self.proto.name)
    }
}

#[derive(Debug)]
pub struct ErrorValue {
    pub kind: &'static str,
    pub message: String,
}

/// A hashable form of a value, for map keys.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Key {
    None,
    Bool(bool),
    Int(i64),
    Str(Rc<String>),
    Tuple(Vec<Key>),
}

impl Key {
    pub fn from_value(value: &Value) -> Option<Key> {
        Some(match value {
            Value::None => Key::None,
            Value::Bool(b) => Key::Bool(*b),
            Value::Int(i) => Key::Int(*i),
            Value::Float(f) if f.fract() == 0.0 && f.abs() < 9.0e15 => Key::Int(*f as i64),
            Value::Str(s) => Key::Str(s.clone()),
            Value::Tuple(items) => {
                Key::Tuple(items.iter().map(Key::from_value).collect::<Option<_>>()?)
            }
            _ => return None,
        })
    }
}

/// An insertion-ordered map.
#[derive(Clone, Debug, Default)]
pub struct Map {
    entries: Vec<(Value, Value)>,
    index: HashMap<Key, usize>,
}

impl Map {
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn get(&self, key: &Key) -> Option<&Value> {
        self.index.get(key).map(|&i| &self.entries[i].1)
    }

    pub fn contains(&self, key: &Key) -> bool {
        self.index.contains_key(key)
    }

    pub fn insert(&mut self, key: Key, key_value: Value, value: Value) {
        match self.index.get(&key) {
            Some(&i) => self.entries[i].1 = value,
            None => {
                self.index.insert(key, self.entries.len());
                self.entries.push((key_value, value));
            }
        }
    }

    pub fn entries(&self) -> &[(Value, Value)] {
        &self.entries
    }
}

#[derive(Debug)]
pub enum Iter {
    /// Iterates a list as it is, so appends during the loop are seen.
    List {
        items: Rc<RefCell<Vec<Value>>>,
        next: usize,
    },
    Values {
        items: Vec<Value>,
        next: usize,
    },
    Range {
        next: i64,
        stop: i64,
        step: i64,
    },
}

impl Iter {
    pub fn next_value(&mut self) -> Option<Value> {
        match self {
            Iter::List { items, next } => {
                let value = items.borrow().get(*next).cloned();
                *next += 1;
                value
            }
            Iter::Values { items, next } => {
                let value = items.get(*next).cloned();
                *next += 1;
                value
            }
            Iter::Range { next, stop, step } => {
                let running = if *step > 0 {
                    *next < *stop
                } else {
                    *next > *stop
                };
                if !running {
                    return None;
                }
                let value = *next;
                *next = next.saturating_add(*step);
                Some(Value::Int(value))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Value;

    #[test]
    fn values_are_sixteen_bytes() {
        assert_eq!(std::mem::size_of::<Value>(), 16);
    }
}
