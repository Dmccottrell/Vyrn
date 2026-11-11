//! `fake()`: stand-ins for testing code without its real dependencies.
//!
//! ```vyrn
//! create greeting(clock):
//!     return "Good morning" if clock() < 12 else "Good afternoon"
//! end
//!
//! create test_greeting():
//!     let clock = fake(returns = 9)
//!     assert(greeting(clock) == "Good morning")
//!     assert(clock.call_count == 1)
//! end
//! ```
//!
//! A fake can be called like a function, and records every call. It gives
//! back `returns`, or the items of `returns_each` one call at a time, or
//! fails with `fails`. Any method asked of it is another fake (remembered,
//! so its calls can be checked), so one fake can stand in for a whole
//! object: `api = fake(methods = {"weather": fake(returns = "sunny")})`.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;

use super::foreign::Foreign;
use super::ops::{err, type_error, Ctx, VmResult};
use super::value::{Key, Map, Value};

pub struct Fake {
    returns: Value,
    each: RefCell<Option<VecDeque<Value>>>,
    fails: Option<Value>,
    calls: RefCell<Vec<Value>>,
    methods: RefCell<Vec<(String, Rc<Box<dyn Foreign>>)>>,
}

fn wrap(f: Fake) -> Value {
    Value::Foreign(Rc::new(Box::new(f)))
}

fn absent(v: Option<&Value>) -> bool {
    matches!(v, None | Some(Value::Missing))
}

/// `fake(returns, returns_each, fails, methods)`
pub fn fake(_: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    let returns = match args.first() {
        Some(v) if !matches!(v, Value::Missing) => v.clone(),
        _ => Value::None,
    };
    let each = if absent(args.get(1)) {
        None
    } else {
        Some(super::ops::collect(&args[1])?.into_iter().collect())
    };
    let fails = if absent(args.get(2)) {
        None
    } else {
        Some(args[2].clone())
    };
    let mut methods = Vec::new();
    if !absent(args.get(3)) {
        let Value::Map(m) = &args[3] else {
            return Err(type_error(
                "`methods` is a map from method name to a fake: {\"save\": fake()}",
            ));
        };
        for (k, v) in m.borrow().entries() {
            let (Value::Str(name), Value::Foreign(f)) = (k, v) else {
                return Err(type_error(
                    "`methods` maps names to fakes: {\"save\": fake(returns = true)}",
                ));
            };
            methods.push((name.to_string(), f.clone()));
        }
    }
    Ok(wrap(Fake {
        returns,
        each: RefCell::new(each),
        fails,
        calls: RefCell::new(Vec::new()),
        methods: RefCell::new(methods),
    }))
}

impl Fake {
    fn method(&self, name: &str) -> Rc<Box<dyn Foreign>> {
        if let Some((_, f)) = self.methods.borrow().iter().find(|(n, _)| n == name) {
            return f.clone();
        }
        let child: Rc<Box<dyn Foreign>> = Rc::new(Box::new(Fake {
            returns: Value::None,
            each: RefCell::new(None),
            fails: None,
            calls: RefCell::new(Vec::new()),
            methods: RefCell::new(Vec::new()),
        }));
        self.methods
            .borrow_mut()
            .push((name.to_string(), child.clone()));
        child
    }
}

impl Foreign for Fake {
    fn type_name(&self) -> &'static str {
        "fake"
    }

    fn display(&self) -> String {
        format!("fake(called {} times)", self.calls.borrow().len())
    }

    fn get_attr(&self, name: &str) -> VmResult<Value> {
        Ok(match name {
            // Every call's arguments, oldest first: `[[1, 2], ["x"]]`.
            "calls" => Value::list(self.calls.borrow().clone()),
            "call_count" => Value::Int(self.calls.borrow().len() as i64),
            "called" => Value::Bool(!self.calls.borrow().is_empty()),
            "last_call" => self.calls.borrow().last().cloned().unwrap_or(Value::None),
            _ => Value::Foreign(self.method(name)),
        })
    }

    fn call(&self, _: &mut Ctx, args: &[Value], kwargs: &[(String, Value)]) -> VmResult<Value> {
        let mut recorded: Vec<Value> = args.to_vec();
        if !kwargs.is_empty() {
            let mut m = Map::default();
            for (k, v) in kwargs {
                let key = Value::str(k.clone());
                if let Some(h) = Key::from_value(&key) {
                    m.insert(h, key, v.clone());
                }
            }
            recorded.push(Value::Map(Rc::new(RefCell::new(m))));
        }
        self.calls.borrow_mut().push(Value::list(recorded));
        if let Some(e) = &self.fails {
            return Err(e.clone());
        }
        if let Some(queue) = self.each.borrow_mut().as_mut() {
            return queue.pop_front().ok_or_else(|| {
                err(
                    "FakeError",
                    "the fake has given every value in `returns_each`",
                )
            });
        }
        Ok(self.returns.clone())
    }

    fn call_method(
        &self,
        ctx: &mut Ctx,
        name: &str,
        args: &[Value],
        kwargs: &[(String, Value)],
    ) -> VmResult<Value> {
        // `f.called_with(1, 2)`: whether any call had exactly these
        // arguments.
        if name == "called_with" {
            let wanted = Value::list(args.to_vec());
            return Ok(Value::Bool(
                self.calls
                    .borrow()
                    .iter()
                    .any(|c| super::ops::equals(c, &wanted)),
            ));
        }
        if name == "reset" {
            self.calls.borrow_mut().clear();
            return Ok(Value::None);
        }
        self.method(name).call(ctx, args, kwargs)
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}
