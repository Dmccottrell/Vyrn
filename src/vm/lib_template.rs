//! `use std.template`: web pages and other text made from data.
//!
//! ```vyrn
//! use std.template
//!
//! main:
//!     let page = template.render("""
//!         <h1>{{title}}</h1>
//!         {{#each items}}<li>{{name}}: {{price}}</li>{{/each}}
//!         {{#if empty}}<p>Nothing yet.</p>{{else}}<p>{{count}} items</p>{{/if}}
//!     """, {"title": "Shop", "items": items, "empty": len(items) == 0, "count": len(items)})
//! end
//! ```
//!
//! `{{name}}` inserts a value with HTML characters escaped, so text from
//! users cannot become part of the page; `{{{name}}}` inserts it as is.
//! `{{a.b}}` reads a map key or a struct field. Inside `{{#each list}}`,
//! `{{this}}` is the item, `{{@index}}` its position, and names are looked
//! up in the item first. `{{! comment }}` is dropped.

use super::ops::{self, err, type_error, Ctx, VmResult};
use super::value::{Key, Value};

#[derive(Debug)]
enum Node {
    Text(String),
    Var {
        path: String,
        raw: bool,
    },
    Each {
        path: String,
        body: Vec<Node>,
    },
    If {
        path: String,
        negate: bool,
        then: Vec<Node>,
        other: Vec<Node>,
    },
}

fn parse(src: &str) -> VmResult<Vec<Node>> {
    let mut pos = 0;
    let (nodes, end) = parse_until(src, &mut pos, None)?;
    if let Some(tag) = end {
        return Err(err(
            "TemplateError",
            format!("`{{{{{tag}}}}}` has no block to close"),
        ));
    }
    Ok(nodes)
}

/// Parses until `{{/name}}` (or `{{else}}` inside an `if`). Returns the
/// nodes and the tag that stopped it.
fn parse_until(
    src: &str,
    pos: &mut usize,
    closing: Option<&str>,
) -> VmResult<(Vec<Node>, Option<String>)> {
    let mut nodes = Vec::new();
    loop {
        let rest = &src[*pos..];
        let Some(open) = rest.find("{{") else {
            if !rest.is_empty() {
                nodes.push(Node::Text(rest.to_string()));
            }
            *pos = src.len();
            return match closing {
                Some(name) => Err(err(
                    "TemplateError",
                    format!("`{{{{#{name}}}}}` is never closed with `{{{{/{name}}}}}`"),
                )),
                None => Ok((nodes, None)),
            };
        };
        if open > 0 {
            nodes.push(Node::Text(rest[..open].to_string()));
        }
        let after = &rest[open..];
        let triple = after.starts_with("{{{");
        let (inner_start, close_pat) = if triple { (3, "}}}") } else { (2, "}}") };
        let Some(close) = after[inner_start..].find(close_pat) else {
            return Err(err("TemplateError", "a `{{` is never closed with `}}`"));
        };
        let tag = after[inner_start..inner_start + close].trim().to_string();
        *pos += open + inner_start + close + close_pat.len();
        if triple {
            nodes.push(Node::Var {
                path: tag,
                raw: true,
            });
            continue;
        }
        if tag.starts_with('!') {
            continue;
        }
        if let Some(name) = tag.strip_prefix('/') {
            let name = name.trim();
            return if Some(name) == closing.map(|c| c.split_whitespace().next().unwrap_or(c)) {
                Ok((nodes, Some(format!("/{name}"))))
            } else {
                Err(err(
                    "TemplateError",
                    format!("`{{{{/{name}}}}}` does not close the open block"),
                ))
            };
        }
        if tag == "else" {
            if closing == Some("if") || closing == Some("unless") {
                return Ok((nodes, Some("else".into())));
            }
            return Err(err("TemplateError", "`{{else}}` outside an `{{#if}}`"));
        }
        if let Some(block) = tag.strip_prefix('#') {
            let mut words = block.split_whitespace();
            let kind = words.next().unwrap_or("").to_string();
            let path = words.collect::<Vec<_>>().join(" ");
            match kind.as_str() {
                "each" => {
                    let (body, _) = parse_until(src, pos, Some("each"))?;
                    nodes.push(Node::Each { path, body });
                }
                "if" | "unless" => {
                    let (then, stop) = parse_until(src, pos, Some(kind.as_str()))?;
                    let other = if stop.as_deref() == Some("else") {
                        parse_until(src, pos, Some(kind.as_str()))?.0
                    } else {
                        Vec::new()
                    };
                    nodes.push(Node::If {
                        path,
                        negate: kind == "unless",
                        then,
                        other,
                    });
                }
                _ => {
                    return Err(err(
                        "TemplateError",
                        format!("unknown block `{{{{#{kind}}}}}`; use #each, #if or #unless"),
                    ))
                }
            }
            continue;
        }
        nodes.push(Node::Var {
            path: tag,
            raw: false,
        });
    }
}

/// Looks a dotted path up in the scopes, innermost first.
fn lookup(ctx: &Ctx, scopes: &[(Value, i64)], path: &str) -> Option<Value> {
    if path == "this" || path == "." {
        return scopes.last().map(|(v, _)| v.clone());
    }
    if path == "@index" {
        return scopes.last().map(|(_, i)| Value::Int(*i));
    }
    let mut parts = path.split('.');
    let first = parts.next()?;
    let mut value = scopes
        .iter()
        .rev()
        .find_map(|(v, _)| field(ctx, v, first))?;
    for p in parts {
        value = field(ctx, &value, p)?;
    }
    Some(value)
}

fn field(ctx: &Ctx, v: &Value, name: &str) -> Option<Value> {
    match v {
        Value::Map(m) => {
            Key::from_value(&Value::str(name)).and_then(|k| m.borrow().get(&k).cloned())
        }
        Value::Struct(inst) => struct_field(ctx, inst.ty, &inst.fields, name),
        Value::Object(o) => {
            let o = o.borrow();
            struct_field(ctx, o.ty, &o.fields, name)
        }
        Value::List(items) => name
            .parse::<usize>()
            .ok()
            .and_then(|i| items.borrow().get(i).cloned()),
        Value::Foreign(f) => f.get_attr(name).ok(),
        _ => None,
    }
}

fn struct_field(ctx: &Ctx, ty: u16, fields: &[Value], name: &str) -> Option<Value> {
    let def = ctx.program.types.get(ty as usize)?;
    let i = def
        .fields
        .iter()
        .position(|f| ctx.program.symbols.name(*f) == name)?;
    fields.get(i).cloned()
}

fn render(
    ctx: &Ctx,
    nodes: &[Node],
    scopes: &mut Vec<(Value, i64)>,
    out: &mut String,
) -> VmResult<()> {
    for node in nodes {
        match node {
            Node::Text(t) => out.push_str(t),
            Node::Var { path, raw } => {
                let v = lookup(ctx, scopes, path).unwrap_or(Value::None);
                let text = match v {
                    Value::None => String::new(),
                    other => ops::display(&ctx.program, &other),
                };
                if *raw {
                    out.push_str(&text);
                } else {
                    out.push_str(&super::lib_html::escape_text(&text));
                }
            }
            Node::Each { path, body } => {
                let Some(list) = lookup(ctx, scopes, path) else {
                    continue;
                };
                let items = match &list {
                    Value::None => Vec::new(),
                    Value::Map(m) => m
                        .borrow()
                        .entries()
                        .iter()
                        .map(|(k, v)| Value::tuple(vec![k.clone(), v.clone()]))
                        .collect(),
                    other => ops::collect(other).map_err(|_| {
                        type_error(format!(
                            "`{{{{#each {path}}}}}` needs a list, not {}",
                            other.type_name()
                        ))
                    })?,
                };
                for (i, item) in items.into_iter().enumerate() {
                    scopes.push((item, i as i64));
                    let r = render(ctx, body, scopes, out);
                    scopes.pop();
                    r?;
                }
            }
            Node::If {
                path,
                negate,
                then,
                other,
            } => {
                let truth = lookup(ctx, scopes, path).is_some_and(|v| v.truthy());
                let branch = if truth != *negate { then } else { other };
                render(ctx, branch, scopes, out)?;
            }
        }
    }
    Ok(())
}

fn render_text(ctx: &Ctx, src: &str, data: Option<&Value>) -> VmResult<String> {
    let nodes = parse(src)?;
    let mut scopes = vec![(data.cloned().unwrap_or(Value::None), 0)];
    let mut out = String::new();
    render(ctx, &nodes, &mut scopes, &mut out)?;
    Ok(out)
}

/// `template.render(text, data)`: the text with `{{...}}` filled in from
/// `data` (a map, or a struct).
pub fn render_fn(ctx: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    let Some(Value::Str(src)) = args.first() else {
        return Err(type_error(
            "`template.render` takes the template text first",
        ));
    };
    render_text(ctx, src, args.get(1)).map(Value::str)
}

/// `template.render_file(path, data)`: the same, with the template in a
/// file (such as `templates/index.html`).
pub fn render_file(ctx: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    let Some(Value::Str(path)) = args.first() else {
        return Err(type_error("`template.render_file` takes a path first"));
    };
    let src = std::fs::read_to_string(path.as_str()).map_err(|e| {
        err(
            if e.kind() == std::io::ErrorKind::NotFound {
                "NotFound"
            } else {
                "IoError"
            },
            format!("cannot read {path}: {e}"),
        )
    })?;
    render_text(ctx, &src, args.get(1)).map(Value::str)
}
