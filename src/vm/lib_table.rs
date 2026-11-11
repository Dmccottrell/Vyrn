//! `use vyrn.table`: rows and columns of data, as pandas gives Python.
//!
//! ```vyrn
//! use vyrn.table
//!
//! main:
//!     let sales = table.read_csv("sales.csv")
//!     print(sales.head(3))
//!     let big = sales.where("amount", ">", 100).sort("amount", descending = true)
//!     print(big.sum("amount"), big.mean("amount"))
//!     print(sales.group_by("region", "amount", "sum"))
//!     sales.to_csv("copy.csv")
//! end
//! ```
//!
//! A table never changes: `where`, `sort`, `select` and the rest give a new
//! one. Numbers in CSV files become numbers, and empty cells `none`.

use std::rc::Rc;

use super::foreign::Foreign;
use super::ops::{self, err, type_error, Ctx, VmResult};
use super::value::{Key, Map, Value};

#[derive(Clone)]
pub struct Table {
    cols: Vec<String>,
    rows: Vec<Vec<Value>>,
}

fn wrap(t: Table) -> Value {
    Value::Foreign(Rc::new(Box::new(t)))
}

fn text_arg<'a>(args: &'a [Value], i: usize, what: &str) -> VmResult<&'a str> {
    match args.get(i) {
        Some(Value::Str(s)) => Ok(s.as_str()),
        Some(other) => Err(type_error(format!(
            "{what} must be a string, not {}",
            other.type_name()
        ))),
        None => Err(err("CallError", format!("{what} is missing"))),
    }
}

fn arg<'a>(
    pos: &'a [Value],
    named: &'a [(String, Value)],
    i: usize,
    name: &str,
) -> Option<&'a Value> {
    named
        .iter()
        .find(|(n, _)| n == name)
        .map(|(_, v)| v)
        .or_else(|| pos.get(i))
        .filter(|v| !matches!(v, Value::Missing))
}

/// A CSV cell as a value: whole numbers, decimals, empty as `none`.
fn cell(text: &str) -> Value {
    let t = text.trim();
    if t.is_empty() {
        return Value::None;
    }
    if let Ok(i) = t.parse::<i64>() {
        return Value::Int(i);
    }
    if t.chars().any(|c| c.is_ascii_digit()) {
        if let Ok(f) = t.parse::<f64>() {
            return Value::Float(f);
        }
    }
    match t {
        "true" | "True" | "TRUE" => Value::Bool(true),
        "false" | "False" | "FALSE" => Value::Bool(false),
        _ => Value::str(text.to_string()),
    }
}

impl Table {
    fn from_csv(text: &str) -> VmResult<Table> {
        let rows = super::stdlib::csv_rows_pub(text, ',');
        let mut it = rows.into_iter();
        let cols: Vec<String> = it.next().unwrap_or_default();
        let rows = it
            .filter(|r| !(r.len() == 1 && r[0].trim().is_empty()))
            .map(|r| {
                let mut row: Vec<Value> = r.iter().map(|c| cell(c)).collect();
                row.resize(cols.len(), Value::None);
                row.truncate(cols.len());
                row
            })
            .collect();
        Ok(Table { cols, rows })
    }

    /// From a list of maps (keys become columns, in first-seen order) or a
    /// list of lists with `columns` given.
    fn from_rows(rows: &[Value], columns: Option<&Value>) -> VmResult<Table> {
        if let Some(c) = columns {
            let cols: Vec<String> = ops::collect(c)?
                .iter()
                .map(|v| match v {
                    Value::Str(s) => Ok(s.to_string()),
                    _ => Err(type_error("column names are strings")),
                })
                .collect::<VmResult<_>>()?;
            let mut out = Vec::new();
            for r in rows {
                let mut row = ops::collect(r)?;
                row.resize(cols.len(), Value::None);
                row.truncate(cols.len());
                out.push(row);
            }
            return Ok(Table { cols, rows: out });
        }
        let mut cols: Vec<String> = Vec::new();
        for r in rows {
            let Value::Map(m) = r else {
                return Err(type_error(
                    "`table.from_rows` takes maps, or lists with `columns`",
                ));
            };
            for (k, _) in m.borrow().entries() {
                let k = match k {
                    Value::Str(s) => s.to_string(),
                    other => format!("{other:?}"),
                };
                if !cols.contains(&k) {
                    cols.push(k);
                }
            }
        }
        let out = rows
            .iter()
            .map(|r| {
                let Value::Map(m) = r else { unreachable!() };
                let m = m.borrow();
                cols.iter()
                    .map(|c| {
                        Key::from_value(&Value::str(c.clone()))
                            .and_then(|k| m.get(&k).cloned())
                            .unwrap_or(Value::None)
                    })
                    .collect()
            })
            .collect();
        Ok(Table { cols, rows: out })
    }

    fn col(&self, name: &str) -> VmResult<usize> {
        self.cols.iter().position(|c| c == name).ok_or_else(|| {
            err(
                "KeyError",
                format!(
                    "no column {name:?}; the columns are {}",
                    self.cols.join(", ")
                ),
            )
        })
    }

    fn row_map(&self, row: &[Value]) -> Value {
        let mut map = Map::default();
        for (c, v) in self.cols.iter().zip(row) {
            let key = Value::str(c.clone());
            if let Some(h) = Key::from_value(&key) {
                map.insert(h, key, v.clone());
            }
        }
        Value::Map(Rc::new(std::cell::RefCell::new(map)))
    }

    fn numbers(&self, i: usize) -> Vec<f64> {
        self.rows
            .iter()
            .filter_map(|r| match &r[i] {
                Value::Int(x) => Some(*x as f64),
                Value::Float(x) => Some(*x),
                _ => None,
            })
            .collect()
    }

    fn sum(&self, i: usize) -> Value {
        let all_int = self
            .rows
            .iter()
            .all(|r| matches!(r[i], Value::Int(_) | Value::None));
        if all_int {
            let mut total: i64 = 0;
            for r in &self.rows {
                if let Value::Int(x) = r[i] {
                    total = total.saturating_add(x);
                }
            }
            return Value::Int(total);
        }
        Value::Float(self.numbers(i).iter().sum())
    }

    fn aggregate(&self, i: usize, how: &str) -> VmResult<Value> {
        let nums = self.numbers(i);
        Ok(match how {
            "sum" => self.sum(i),
            "count" => Value::Int(
                self.rows
                    .iter()
                    .filter(|r| !matches!(r[i], Value::None))
                    .count() as i64,
            ),
            "mean" | "average" => {
                if nums.is_empty() {
                    Value::None
                } else {
                    Value::Float(nums.iter().sum::<f64>() / nums.len() as f64)
                }
            }
            "median" => {
                if nums.is_empty() {
                    return Ok(Value::None);
                }
                let mut v = nums.clone();
                v.sort_by(|a, b| a.total_cmp(b));
                let n = v.len();
                Value::Float(if n % 2 == 1 {
                    v[n / 2]
                } else {
                    (v[n / 2 - 1] + v[n / 2]) / 2.0
                })
            }
            "min" | "max" => {
                let mut best: Option<Value> = None;
                for r in &self.rows {
                    let v = &r[i];
                    if matches!(v, Value::None) {
                        continue;
                    }
                    best = Some(match best {
                        None => v.clone(),
                        Some(b) => {
                            let o = ops::order(v, &b)?;
                            if (how == "min" && o.is_lt()) || (how == "max" && o.is_gt()) {
                                v.clone()
                            } else {
                                b
                            }
                        }
                    });
                }
                best.unwrap_or(Value::None)
            }
            other => {
                return Err(err(
                    "ValueError",
                    format!("unknown summary {other:?}: use sum, mean, median, min, max or count"),
                ))
            }
        })
    }

    fn to_csv(&self, ctx: &Ctx) -> String {
        let field = |s: String| {
            if s.contains([',', '"', '\n']) {
                format!("\"{}\"", s.replace('"', "\"\""))
            } else {
                s
            }
        };
        let mut out = self
            .cols
            .iter()
            .map(|c| field(c.clone()))
            .collect::<Vec<_>>()
            .join(",");
        out.push('\n');
        for r in &self.rows {
            let line: Vec<String> = r
                .iter()
                .map(|v| match v {
                    Value::None => String::new(),
                    v => field(ops::display(&ctx.program, v)),
                })
                .collect();
            out.push_str(&line.join(","));
            out.push('\n');
        }
        out
    }

    /// An aligned text table, at most 20 rows.
    fn render(&self) -> String {
        let shown = self.rows.len().min(20);
        let show = |v: &Value| -> String {
            match v {
                Value::None => String::new(),
                Value::Str(s) => s.to_string(),
                Value::Float(f) => super::ops::fmt_float(*f),
                Value::Int(i) => i.to_string(),
                Value::Bool(b) => b.to_string(),
                other => other.type_name().to_string(),
            }
        };
        let cells: Vec<Vec<String>> = self.rows[..shown]
            .iter()
            .map(|r| r.iter().map(show).collect())
            .collect();
        let widths: Vec<usize> = self
            .cols
            .iter()
            .enumerate()
            .map(|(i, c)| {
                cells
                    .iter()
                    .map(|r| r[i].chars().count())
                    .chain([c.chars().count()])
                    .max()
                    .unwrap_or(0)
            })
            .collect();
        let numeric: Vec<bool> = (0..self.cols.len())
            .map(|i| {
                self.rows
                    .iter()
                    .all(|r| matches!(r[i], Value::Int(_) | Value::Float(_) | Value::None))
            })
            .collect();
        let line = |vals: Vec<String>| -> String {
            vals.iter()
                .enumerate()
                .map(|(i, v)| {
                    if numeric[i] {
                        format!("{v:>w$}", w = widths[i])
                    } else {
                        format!("{v:<w$}", w = widths[i])
                    }
                })
                .collect::<Vec<_>>()
                .join("  ")
                .trim_end()
                .to_string()
        };
        let mut out = vec![line(self.cols.clone())];
        out.push(
            widths
                .iter()
                .map(|w| "-".repeat(*w))
                .collect::<Vec<_>>()
                .join("  "),
        );
        for r in cells {
            out.push(line(r));
        }
        if self.rows.len() > shown {
            out.push(format!("... {} more rows", self.rows.len() - shown));
        }
        out.join("\n")
    }
}

fn matches(v: &Value, op: &str, x: &Value) -> VmResult<bool> {
    if matches!(v, Value::None) && !matches!(op, "==" | "!=") {
        return Ok(false);
    }
    Ok(match op {
        "==" => ops::equals(v, x),
        "!=" => !ops::equals(v, x),
        "<" | "<=" | ">" | ">=" => {
            let o = match ops::order(v, x) {
                Ok(o) => o,
                Err(_) => return Ok(false),
            };
            match op {
                "<" => o.is_lt(),
                "<=" => o.is_le(),
                ">" => o.is_gt(),
                _ => o.is_ge(),
            }
        }
        "contains" => match (v, x) {
            (Value::Str(s), Value::Str(t)) => s.contains(t.as_str()),
            _ => false,
        },
        "startswith" => match (v, x) {
            (Value::Str(s), Value::Str(t)) => s.starts_with(t.as_str()),
            _ => false,
        },
        "in" => ops::contains(v, x).unwrap_or(false),
        other => {
            return Err(err(
                "ValueError",
                format!("unknown comparison {other:?}: use ==, !=, <, <=, >, >=, contains, startswith or in"),
            ))
        }
    })
}

impl Foreign for Table {
    fn type_name(&self) -> &'static str {
        "Table"
    }

    fn display(&self) -> String {
        self.render()
    }

    fn get_attr(&self, name: &str) -> VmResult<Value> {
        match name {
            "columns" => Ok(Value::list(
                self.cols.iter().map(|c| Value::str(c.clone())).collect(),
            )),
            "rows" => Ok(Value::list(
                self.rows.iter().map(|r| self.row_map(r)).collect(),
            )),
            "shape" => Ok(Value::tuple(vec![
                Value::Int(self.rows.len() as i64),
                Value::Int(self.cols.len() as i64),
            ])),
            _ => Err(err("FieldError", format!("a Table has no field `{name}`"))),
        }
    }

    fn call_method(
        &self,
        ctx: &mut Ctx,
        name: &str,
        pos: &[Value],
        named: &[(String, Value)],
    ) -> VmResult<Value> {
        let a = |i: usize, n: &str| arg(pos, named, i, n);
        let col_arg = |i: usize, n: &str| -> VmResult<usize> {
            match a(i, n) {
                Some(Value::Str(s)) => self.col(s),
                _ => Err(err("CallError", format!("`.{name}()` needs a column name"))),
            }
        };
        let count = |i: usize, n: &str, d: i64| -> VmResult<usize> {
            Ok(match a(i, n) {
                Some(v) => super::builtins::integer(v, "the count")?.max(0) as usize,
                None => d as usize,
            })
        };
        match name {
            "head" => {
                let n = count(0, "n", 5)?;
                Ok(wrap(Table {
                    cols: self.cols.clone(),
                    rows: self.rows.iter().take(n).cloned().collect(),
                }))
            }
            "tail" => {
                let n = count(0, "n", 5)?;
                let skip = self.rows.len().saturating_sub(n);
                Ok(wrap(Table {
                    cols: self.cols.clone(),
                    rows: self.rows[skip..].to_vec(),
                }))
            }
            "row" => {
                let i =
                    super::builtins::integer(a(0, "index").unwrap_or(&Value::Int(0)), "the row")?;
                let at = super::tensor::normalize_index(i, self.rows.len()).ok_or_else(|| {
                    err(
                        "IndexError",
                        format!(
                            "row {i} is out of range for a table of {} rows",
                            self.rows.len()
                        ),
                    )
                })?;
                Ok(self.row_map(&self.rows[at]))
            }
            "column" => {
                let i = col_arg(0, "name")?;
                Ok(Value::list(
                    self.rows.iter().map(|r| r[i].clone()).collect(),
                ))
            }
            "where" => {
                let i = col_arg(0, "column")?;
                let op = text_arg(pos, 1, "the comparison")?;
                let x = a(2, "value").cloned().unwrap_or(Value::None);
                let mut rows = Vec::new();
                for r in &self.rows {
                    if matches(&r[i], op, &x)? {
                        rows.push(r.clone());
                    }
                }
                Ok(wrap(Table {
                    cols: self.cols.clone(),
                    rows,
                }))
            }
            "sort" => {
                let i = col_arg(0, "column")?;
                let desc = a(1, "descending").is_some_and(Value::truthy);
                let mut rows = self.rows.clone();
                let mut failure = None;
                // `none` sorts last either way.
                rows.sort_by(|x, y| match (&x[i], &y[i]) {
                    (Value::None, Value::None) => std::cmp::Ordering::Equal,
                    (Value::None, _) => std::cmp::Ordering::Greater,
                    (_, Value::None) => std::cmp::Ordering::Less,
                    (p, q) => {
                        let o = ops::order(p, q).unwrap_or_else(|e| {
                            failure.get_or_insert(e);
                            std::cmp::Ordering::Equal
                        });
                        if desc {
                            o.reverse()
                        } else {
                            o
                        }
                    }
                });
                if let Some(e) = failure {
                    return Err(e);
                }
                Ok(wrap(Table {
                    cols: self.cols.clone(),
                    rows,
                }))
            }
            "select" => {
                let names = ops::collect(a(0, "columns").unwrap_or(&Value::None))?;
                let idx: Vec<usize> = names
                    .iter()
                    .map(|n| match n {
                        Value::Str(s) => self.col(s),
                        _ => Err(type_error("column names are strings")),
                    })
                    .collect::<VmResult<_>>()?;
                Ok(wrap(Table {
                    cols: idx.iter().map(|i| self.cols[*i].clone()).collect(),
                    rows: self
                        .rows
                        .iter()
                        .map(|r| idx.iter().map(|i| r[*i].clone()).collect())
                        .collect(),
                }))
            }
            "add_column" => {
                let col = text_arg(pos, 0, "the column name")?.to_string();
                let values = ops::collect(a(1, "values").unwrap_or(&Value::None))?;
                if values.len() != self.rows.len() {
                    return Err(err(
                        "ValueError",
                        format!(
                            "the table has {} rows but {} values were given",
                            self.rows.len(),
                            values.len()
                        ),
                    ));
                }
                let mut t = self.clone();
                t.cols.push(col);
                for (r, v) in t.rows.iter_mut().zip(values) {
                    r.push(v);
                }
                Ok(wrap(t))
            }
            "sum" | "mean" | "average" | "median" | "min" | "max" | "count" => {
                let i = col_arg(0, "column")?;
                self.aggregate(i, name)
            }
            "unique" => {
                let i = col_arg(0, "column")?;
                let mut seen: Vec<Value> = Vec::new();
                for r in &self.rows {
                    if !seen.iter().any(|s| ops::equals(s, &r[i])) {
                        seen.push(r[i].clone());
                    }
                }
                Ok(Value::list(seen))
            }
            // `t.group_by("region", "amount", "sum")`: one row per region.
            "group_by" => {
                let g = col_arg(0, "column")?;
                let how = match a(2, "how") {
                    Some(Value::Str(s)) => s.to_string(),
                    _ => "count".to_string(),
                };
                let v = match a(1, "value") {
                    Some(Value::Str(s)) => self.col(s)?,
                    _ if how == "count" => g,
                    _ => return Err(err("CallError", "`group_by` needs the column to summarise")),
                };
                let mut keys: Vec<Value> = Vec::new();
                let mut groups: Vec<Vec<Vec<Value>>> = Vec::new();
                for r in &self.rows {
                    match keys.iter().position(|k| ops::equals(k, &r[g])) {
                        Some(at) => groups[at].push(r.clone()),
                        None => {
                            keys.push(r[g].clone());
                            groups.push(vec![r.clone()]);
                        }
                    }
                }
                let mut rows = Vec::new();
                for (k, rs) in keys.into_iter().zip(groups) {
                    let sub = Table {
                        cols: self.cols.clone(),
                        rows: rs,
                    };
                    let value = if how == "count" {
                        Value::Int(sub.rows.len() as i64)
                    } else {
                        sub.aggregate(v, &how)?
                    };
                    rows.push(vec![k, value]);
                }
                let label = if how == "count" {
                    "count".to_string()
                } else {
                    format!("{}_{how}", self.cols[v])
                };
                Ok(wrap(Table {
                    cols: vec![self.cols[g].clone(), label],
                    rows,
                }))
            }
            "to_csv" => {
                let text = self.to_csv(ctx);
                match a(0, "path") {
                    Some(Value::Str(path)) => {
                        std::fs::write(path.as_str(), text)
                            .map_err(|e| err("IoError", format!("cannot write {path}: {e}")))?;
                        Ok(Value::None)
                    }
                    _ => Ok(Value::str(text)),
                }
            }
            _ => Err(err("FieldError", format!("a Table has no method `{name}`"))),
        }
    }

    fn len(&self) -> VmResult<usize> {
        Ok(self.rows.len())
    }

    /// `each row in t`: the rows, as maps.
    fn items(&self) -> VmResult<Vec<Value>> {
        Ok(self.rows.iter().map(|r| self.row_map(r)).collect())
    }

    fn json(&self) -> Option<Value> {
        Some(Value::list(
            self.rows.iter().map(|r| self.row_map(r)).collect(),
        ))
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

/// `table.read_csv(path)`
pub fn read_csv(_: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    let path = text_arg(args, 0, "the path")?;
    let text = std::fs::read_to_string(path).map_err(|e| {
        err(
            if e.kind() == std::io::ErrorKind::NotFound {
                "NotFound"
            } else {
                "IoError"
            },
            format!("cannot read {path}: {e}"),
        )
    })?;
    Table::from_csv(&text).map(wrap)
}

/// `table.from_csv(text)`
pub fn from_csv(_: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    Table::from_csv(text_arg(args, 0, "the CSV text")?).map(wrap)
}

/// `table.from_rows(rows, columns = none)`: from maps, or lists and the
/// column names.
pub fn from_rows(_: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    let rows = ops::collect(args.first().unwrap_or(&Value::None))?;
    Table::from_rows(
        &rows,
        args.get(1)
            .filter(|v| !matches!(v, Value::Missing | Value::None)),
    )
    .map(wrap)
}
