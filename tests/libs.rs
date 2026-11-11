//! The application libraries: std.db, std.draw, std.html, std.template,
//! std.table, std.chart and std.mail.

use vyrn::vm::{run_source, RunError};

fn run(src: &str) -> String {
    match run_source(src) {
        Ok(out) => out,
        Err(RunError::Compile(errors)) => {
            let report: String = errors.iter().map(|e| e.render("test.vyrn", src)).collect();
            panic!("compile errors:\n{report}");
        }
        Err(RunError::Runtime(e)) => panic!("runtime error: {e}"),
    }
}

fn error(src: &str) -> String {
    match run_source(src) {
        Err(RunError::Runtime(e)) => format!("{}: {}", e.kind, e.message),
        other => panic!("expected a runtime error, got {:?}", other.map(|_| ())),
    }
}

fn temp(name: &str) -> String {
    let dir = std::env::temp_dir().join(format!("vyrn-libs-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir.join(name).display().to_string().replace('\\', "/")
}

#[cfg(feature = "db")]
#[test]
fn sqlite_databases() {
    let path = temp("shop.db");
    let _ = std::fs::remove_file(&path);
    let src = format!(
        "use std.db\n\
         main:\n\
         \x20   using db.open(\"{path}\") as conn:\n\
         \x20       conn.execute(\"create table items (id integer primary key, name text not null, price real, stock int)\")\n\
         \x20       conn.execute(\"insert into items (name, price, stock) values (?, ?, ?)\", [\"pen\", 1.5, 10])\n\
         \x20       print(conn.last_id())\n\
         \x20       conn.execute_many(\"insert into items (name, price, stock) values (?, ?, ?)\", [[\"book\", 12.0, 3], [\"lamp\", 30.25, none]])\n\
         \x20       conn.execute(\"update items set stock = stock - 1 where name = :name\", {{\"name\": \"pen\"}})\n\
         \x20       each row in conn.query(\"select name, price, stock from items where price < ? order by price\", [20]):\n\
         \x20           print(row[\"name\"], row[\"price\"], row[\"stock\"])\n\
         \x20       end\n\
         \x20       print(conn.query_one(\"select * from items where name = ?\", [\"lamp\"]))\n\
         \x20       print(conn.query_one(\"select * from items where name = ?\", [\"nothing\"]))\n\
         \x20       print(conn.value(\"select count(*) from items\"), conn.tables())\n\
         \x20       conn.begin()\n\
         \x20       conn.execute(\"delete from items\")\n\
         \x20       conn.rollback()\n\
         \x20       print(conn.value(\"select sum(price) from items\"))\n\
         \x20       let evil = \"x'; drop table items; --\"\n\
         \x20       print(conn.query(\"select * from items where name = ?\", [evil]), conn.tables())\n\
         \x20   end\n\
         \x20   let again = db.open(\"{path}\")\n\
         \x20   print(again.value(\"select name from items where id = 2\"))\n\
         end\n"
    );
    assert_eq!(
        run(&src),
        "1\npen 1.5 9\nbook 12.0 3\n{\"id\": 3, \"name\": \"lamp\", \"price\": 30.25, \"stock\": none}\nnone\n3 [\"items\"]\n43.75\n[] [\"items\"]\nbook\n"
    );
    let e = error("use std.db\nmain:\n    let c = db.open(\":memory:\")\n    c.execute(\"select * from missing\")\nend\n");
    assert!(
        e.starts_with("DatabaseError: no such table: missing"),
        "{e}"
    );
    let e = error("use std.db\nmain:\n    let c = db.open(\":memory:\")\n    c.execute(\"create table t (a int)\")\n    c.execute(\"insert into t values (?)\", [1, 2])\nend\n");
    assert!(e.contains("takes 1 values, but 2 were given"), "{e}");
    let e = error("use std.db\nmain:\n    let c = db.open(\":memory:\")\n    c.close()\n    c.query(\"select 1\")\nend\n");
    assert!(e.contains("is closed"), "{e}");
}
