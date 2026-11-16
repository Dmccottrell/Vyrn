//! The part of the Prelude written in Vyrn (`src/prelude.vyrn`): the
//! functions that take a function, such as `sorted(xs, key = ...)` and
//! `xs.map(f)`. Its declarations are added to every program at compile
//! time, after the program's own, and skipped where the program declares
//! the same name, so a program's `map` or `sorted` always wins.

use std::collections::HashSet;

use crate::ast::{Item, ItemKind, Module, UseKind};

pub const SOURCE: &str = include_str!("prelude.vyrn");

/// Where the Prelude's spans start, far past any file a program loads
/// (files are placed at `index << 24`).
pub const BASE: u32 = 0xFE << 24;

/// The names a module declares at its top level.
pub fn declared_names(module: &Module) -> HashSet<String> {
    let mut names = HashSet::new();
    for item in &module.items {
        match &item.kind {
            ItemKind::Fn(f) => {
                names.insert(f.name.name.clone());
            }
            ItemKind::Aggregate(a) => {
                names.insert(a.name.name.clone());
            }
            ItemKind::Enum(e) => {
                names.insert(e.name.name.clone());
            }
            ItemKind::Const(c) => {
                names.insert(c.name.name.clone());
            }
            ItemKind::TypeAlias(t) => {
                names.insert(t.name.name.clone());
            }
            ItemKind::Trait(t) => {
                names.insert(t.name.name.clone());
            }
            ItemKind::Extern(e) => {
                names.extend(e.functions.iter().map(|f| f.name.name.clone()));
            }
            ItemKind::Use(u) => match &u.kind {
                UseKind::Module { alias } => {
                    if let Some(name) = alias.as_ref().or(u.path.segments.last()) {
                        names.insert(name.name.clone());
                    }
                }
                UseKind::Names(list) => {
                    for n in list {
                        names.insert(n.alias.as_ref().unwrap_or(&n.name).name.clone());
                    }
                }
            },
            _ => {}
        }
    }
    names
}

fn items() -> Vec<Item> {
    thread_local! {
        static ITEMS: Vec<Item> = {
            let (module, errors) = crate::parser::parse_at(SOURCE, BASE);
            assert!(errors.is_empty(), "the Prelude does not parse: {errors:?}");
            module.items
        };
    }
    ITEMS.with(Clone::clone)
}

/// `module` with the Prelude's declarations it does not override.
pub fn with_prelude(module: &Module) -> Module {
    let mine = declared_names(module);
    let mut out = module.clone();
    for item in items() {
        let name = match &item.kind {
            ItemKind::Fn(f) => &f.name.name,
            _ => continue,
        };
        if !mine.contains(name) {
            out.items.push(item);
        }
    }
    out
}

/// Whether a declaration came from the Prelude.
pub fn is_prelude_offset(offset: u32) -> bool {
    offset >= BASE
}
