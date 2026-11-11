//! `use vyrn.db`: SQLite databases, built into `vyrn`.
//!
//! ```vyrn
//! use vyrn.db
//!
//! main:
//!     using db.open("shop.db") as conn:
//!         conn.execute("create table if not exists items (name text, price real)")
//!         conn.execute("insert into items values (?, ?)", ["pen", 1.5])
//!         each row in conn.query("select * from items where price < ?", [10]):
//!             print(row["name"], row["price"])
//!         end
//!     end
//! end
//! ```
//!
//! Rows come back as maps from column name to value. `?` in the SQL takes
//! the values of a list in order; `:name` takes them from a map. Values
//! are always passed separately from the SQL, so text from users cannot
//! change what a statement does.

#[cfg_attr(not(feature = "db"), allow(unused_imports))]
use super::builtins::opt;
#[cfg_attr(not(feature = "db"), allow(unused_imports))]
use super::foreign::Foreign;
#[cfg_attr(not(feature = "db"), allow(unused_imports))]
use super::ops::{self, err, type_error, Ctx, VmResult};
#[cfg_attr(not(feature = "db"), allow(unused_imports))]
use super::value::{Key, Map, Value};

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

#[cfg(feature = "db")]
mod imp {
    use super::*;
    use rusqlite::types::{Value as Sql, ValueRef};
    use std::cell::RefCell;
    use std::rc::Rc;

    pub struct Database {
        pub path: String,
        pub conn: RefCell<Option<rusqlite::Connection>>,
    }

    fn db_error(e: rusqlite::Error) -> Value {
        err("DatabaseError", e.to_string())
    }

    fn to_sql(v: &Value) -> VmResult<Sql> {
        Ok(match v {
            Value::None => Sql::Null,
            Value::Bool(b) => Sql::Integer(i64::from(*b)),
            Value::Int(i) => Sql::Integer(*i),
            Value::Float(f) => Sql::Real(*f),
            Value::Str(s) => Sql::Text(s.to_string()),
            other => {
                return Err(type_error(format!(
                    "a database value must be a number, a string, a bool or none, not {}",
                    other.type_name()
                )))
            }
        })
    }

    fn from_sql(v: ValueRef) -> Value {
        match v {
            ValueRef::Null => Value::None,
            ValueRef::Integer(i) => Value::Int(i),
            ValueRef::Real(f) => Value::Float(f),
            ValueRef::Text(t) => Value::str(String::from_utf8_lossy(t).into_owned()),
            ValueRef::Blob(b) => Value::list(b.iter().map(|x| Value::Int(i64::from(*x))).collect()),
        }
    }

    /// Prepares `sql` and binds the parameters: a list for `?`, a map for
    /// `:name`. Then runs `f` on the bound statement.
    fn run<T>(
        conn: &rusqlite::Connection,
        sql: &str,
        params: Option<&Value>,
        f: impl FnOnce(&mut rusqlite::Statement) -> rusqlite::Result<T>,
    ) -> VmResult<T> {
        let mut stmt = conn.prepare(sql).map_err(db_error)?;
        let expected = stmt.parameter_count();
        match params {
            None | Some(Value::None) => {}
            Some(Value::Map(m)) => {
                for (k, v) in m.borrow().entries() {
                    let Value::Str(k) = k else {
                        return Err(type_error("named parameters need string keys"));
                    };
                    let name = if k.starts_with([':', '@', '$']) {
                        k.to_string()
                    } else {
                        format!(":{k}")
                    };
                    let i = stmt
                        .parameter_index(&name)
                        .map_err(db_error)?
                        .ok_or_else(|| {
                            err(
                                "DatabaseError",
                                format!("the SQL has no parameter `{name}`"),
                            )
                        })?;
                    stmt.raw_bind_parameter(i, to_sql(v)?).map_err(db_error)?;
                }
            }
            Some(other) => {
                let values = ops::collect(other)?;
                if values.len() != expected {
                    return Err(err(
                        "DatabaseError",
                        format!(
                            "the SQL takes {expected} values, but {} were given",
                            values.len()
                        ),
                    ));
                }
                for (i, v) in values.iter().enumerate() {
                    stmt.raw_bind_parameter(i + 1, to_sql(v)?)
                        .map_err(db_error)?;
                }
            }
        }
        f(&mut stmt).map_err(db_error)
    }

    fn rows(
        conn: &rusqlite::Connection,
        sql: &str,
        params: Option<&Value>,
        limit: Option<usize>,
    ) -> VmResult<Vec<Value>> {
        run(conn, sql, params, |stmt| {
            let names: Vec<String> = stmt.column_names().map(|s| s.to_string()).collect();
            let mut out = Vec::new();
            let mut rows = stmt.raw_query();
            while let Some(row) = rows.next()? {
                let mut map = Map::default();
                for (i, name) in names.iter().enumerate() {
                    let key = Value::str(name.clone());
                    if let Some(h) = Key::from_value(&key) {
                        map.insert(h, key, from_sql(row.get_ref(i)?));
                    }
                }
                out.push(Value::Map(Rc::new(RefCell::new(map))));
                if limit.is_some_and(|l| out.len() >= l) {
                    break;
                }
            }
            Ok(out)
        })
    }

    impl Database {
        fn with<T>(&self, f: impl FnOnce(&rusqlite::Connection) -> VmResult<T>) -> VmResult<T> {
            match &*self.conn.borrow() {
                Some(c) => f(c),
                None => Err(err(
                    "DatabaseError",
                    format!("the database {} is closed", self.path),
                )),
            }
        }
    }

    impl Foreign for Database {
        fn type_name(&self) -> &'static str {
            "Database"
        }

        fn display(&self) -> String {
            format!("Database({})", super::super::stdlib::quote(&self.path))
        }

        fn call_method(
            &self,
            _: &mut Ctx,
            name: &str,
            args: &[Value],
            _: &[(String, Value)],
        ) -> VmResult<Value> {
            match name {
                // `conn.execute(sql, params)`: how many rows changed.
                "execute" => {
                    let sql = text_arg(args, 0, "the SQL")?;
                    self.with(|c| {
                        // Several statements without parameters run as a
                        // script, as `executescript` would.
                        if opt(args, 1).is_none() && sql.trim_end().trim_end_matches(';').contains(';') {
                            c.execute_batch(sql).map_err(db_error)?;
                            return Ok(Value::Int(0));
                        }
                        run(c, sql, opt(args, 1), |stmt| stmt.raw_execute())
                            .map(|n| Value::Int(n as i64))
                    })
                }
                // `conn.execute_many(sql, rows)`: one statement per row.
                "execute_many" => {
                    let sql = text_arg(args, 0, "the SQL")?;
                    let rows = ops::collect(args.get(1).unwrap_or(&Value::None))?;
                    self.with(|c| {
                        let mut total = 0i64;
                        for row in &rows {
                            total += run(c, sql, Some(row), |stmt| stmt.raw_execute())? as i64;
                        }
                        Ok(Value::Int(total))
                    })
                }
                // `conn.query(sql, params)`: every row, as maps.
                "query" => {
                    let sql = text_arg(args, 0, "the SQL")?;
                    self.with(|c| rows(c, sql, opt(args, 1), None).map(Value::list))
                }
                // `conn.query_one(sql, params)`: the first row, or none.
                "query_one" => {
                    let sql = text_arg(args, 0, "the SQL")?;
                    self.with(|c| {
                        Ok(rows(c, sql, opt(args, 1), Some(1))?
                            .into_iter()
                            .next()
                            .unwrap_or(Value::None))
                    })
                }
                // `conn.value(sql, params)`: the first column of the first
                // row, or none: `conn.value("select count(*) from items")`.
                "value" => {
                    let sql = text_arg(args, 0, "the SQL")?;
                    self.with(|c| {
                        run(c, sql, opt(args, 1), |stmt| {
                            let mut rows = stmt.raw_query();
                            Ok(match rows.next()? {
                                Some(row) => from_sql(row.get_ref(0)?),
                                None => Value::None,
                            })
                        })
                    })
                }
                "last_id" => self.with(|c| Ok(Value::Int(c.last_insert_rowid()))),
                "tables" => self.with(|c| {
                    let names = rows(
                        c,
                        "select name from sqlite_master where type = 'table' and name not like 'sqlite_%' order by name",
                        None,
                        None,
                    )?;
                    Ok(Value::list(
                        names
                            .iter()
                            .filter_map(|m| match m {
                                Value::Map(m) => m.borrow().entries().first().map(|(_, v)| v.clone()),
                                _ => None,
                            })
                            .collect(),
                    ))
                }),
                "begin" => self.with(|c| c.execute_batch("begin").map(|_| Value::None).map_err(db_error)),
                "commit" => self.with(|c| c.execute_batch("commit").map(|_| Value::None).map_err(db_error)),
                "rollback" => self.with(|c| c.execute_batch("rollback").map(|_| Value::None).map_err(db_error)),
                "close" => {
                    self.conn.borrow_mut().take();
                    Ok(Value::None)
                }
                _ => Err(err("FieldError", format!("a Database has no method `{name}`"))),
            }
        }

        fn get_attr(&self, name: &str) -> VmResult<Value> {
            match name {
                "path" => Ok(Value::str(self.path.clone())),
                _ => Err(err(
                    "FieldError",
                    format!("a Database has no field `{name}`"),
                )),
            }
        }

        fn as_any(&self) -> &dyn std::any::Any {
            self
        }
    }

    pub fn open(path: &str) -> VmResult<Value> {
        let conn = if path == ":memory:" {
            rusqlite::Connection::open_in_memory()
        } else {
            rusqlite::Connection::open(path)
        }
        .map_err(|e| err("DatabaseError", format!("cannot open {path}: {e}")))?;
        Ok(Value::Foreign(Rc::new(Box::new(Database {
            path: path.to_string(),
            conn: RefCell::new(Some(conn)),
        }))))
    }
}

/// `db.open(path)`: a SQLite database file, made if it is not there.
/// `":memory:"` is a database that lives only while the program runs.
pub fn db_open(_: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    let path = text_arg(args, 0, "the database path")?;
    #[cfg(feature = "db")]
    {
        imp::open(path)
    }
    #[cfg(not(feature = "db"))]
    {
        Err(err(
            "Unavailable",
            format!("`db.open({path:?})` needs a build with databases (the `db` feature)"),
        ))
    }
}
