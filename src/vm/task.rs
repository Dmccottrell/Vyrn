//! Tasks and channels: the shared state behind `spawn`, `await`, `sleep`
//! and `async each`.
//!
//! Tasks are *stackful*: each one owns its registers and call frames, kept
//! by the VM while the task is suspended. That is why any function can
//! `await`, not only `async` ones: suspending saves the whole stack of calls.
//! A task is visible to programs only through its [`TaskHandle`], which
//! receives the result.

use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::fmt;

use super::value::Value;

pub type TaskId = usize;

/// The task the host called into. Spawned tasks get ids from 1.
pub const ROOT_TASK: TaskId = 0;

thread_local! {
    /// Failures of tasks that were dropped without anyone awaiting them.
    static UNOBSERVED: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

/// Takes the messages of task failures nobody awaited.
pub fn take_unobserved() -> Vec<String> {
    UNOBSERVED.with(|u| std::mem::take(&mut *u.borrow_mut()))
}

/// What a program holds for a running or finished task.
pub struct TaskHandle {
    pub id: TaskId,
    /// The function the task runs, for messages.
    pub name: String,
    /// `None` while the task runs; then its return value or its failure.
    pub result: RefCell<Option<Result<Value, Value>>>,
    /// Tasks waiting for this one to finish.
    pub waiters: RefCell<Vec<TaskId>>,
    /// Whether anyone has awaited the result.
    pub observed: Cell<bool>,
}

impl TaskHandle {
    pub fn new(id: TaskId, name: impl Into<String>) -> TaskHandle {
        TaskHandle {
            id,
            name: name.into(),
            result: RefCell::new(None),
            waiters: RefCell::new(Vec::new()),
            observed: Cell::new(false),
        }
    }

    /// A task that has already finished with `value`.
    pub fn done(value: Value) -> TaskHandle {
        let handle = TaskHandle::new(usize::MAX, "value");
        *handle.result.borrow_mut() = Some(Ok(value));
        handle
    }

    pub fn is_done(&self) -> bool {
        self.result.borrow().is_some()
    }

    /// Records the result and returns the tasks to wake.
    pub fn finish(&self, result: Result<Value, Value>) -> Vec<TaskId> {
        *self.result.borrow_mut() = Some(result);
        std::mem::take(&mut *self.waiters.borrow_mut())
    }
}

impl Drop for TaskHandle {
    fn drop(&mut self) {
        if self.observed.get() {
            return;
        }
        if let Some(Err(failure)) = &*self.result.borrow() {
            let message = match failure {
                Value::Error(e) => format!("{}: {}", e.kind, e.message),
                other => format!("{other:?}"),
            };
            UNOBSERVED.with(|u| {
                u.borrow_mut().push(format!(
                    "task `{}` failed and was never awaited: {message}",
                    self.name
                ))
            });
        }
    }
}

impl fmt::Debug for TaskHandle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "<task {} {}>", self.id, self.name)
    }
}

/// A queue between tasks: unbounded, or holding at most `capacity` values.
#[derive(Debug, Default)]
pub struct Channel {
    pub queue: VecDeque<Value>,
    pub closed: bool,
    /// Tasks waiting for a value.
    pub waiters: Vec<TaskId>,
    /// At most this many values wait in the queue; `send` waits when full.
    pub capacity: Option<usize>,
    /// Values whose senders are waiting for room, oldest first.
    pub senders: VecDeque<(Value, std::rc::Rc<TaskHandle>)>,
}
