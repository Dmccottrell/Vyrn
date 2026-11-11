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

#[cfg(feature = "draw")]
#[test]
fn drawing_on_a_canvas() {
    let png = temp("pic.png");
    let src = format!(
        "use std.draw\n\
         main:\n\
         \x20   let c = draw.canvas(100, 80)\n\
         \x20   c.clear(\"black\")\n\
         \x20   c.rect(10, 10, 20, 10, \"#ff0000\")\n\
         \x20   c.circle(60, 20, 8, [0, 0, 255])\n\
         \x20   c.line(0, 79, 99, 79, \"white\")\n\
         \x20   c.rect(70, 50, 20, 20, \"lime\", fill = false)\n\
         \x20   c.text(\"A\", 40, 40, \"yellow\", size = 1)\n\
         \x20   print(c.get_pixel(15, 15), c.get_pixel(60, 20), c.get_pixel(50, 79), c.get_pixel(5, 5))\n\
         \x20   print(c.get_pixel(70, 60), c.get_pixel(80, 60), c.get_pixel(500, 5))\n\
         \x20   var lit = 0\n\
         \x20   each y in 40 until 48:\n        each x in 40 until 48:\n            if c.get_pixel(x, y) != \"#000000\": lit += 1\n        end\n    end\n\
         \x20   print(lit > 10, draw.text_width(\"abc\"), draw.rgb(255, 128, 0))\n\
         \x20   c.save(\"{png}\")\n\
         \x20   let img = draw.image(\"{png}\")\n\
         \x20   let d = draw.canvas(100, 80)\n\
         \x20   d.image(img, 0, 0)\n\
         \x20   print(img.width, img.height, d.get_pixel(15, 15), d.get_pixel(60, 20))\n\
         \x20   print(draw.overlaps(0, 0, 10, 10, 5, 5, 10, 10), draw.overlaps(0, 0, 10, 10, 20, 0, 5, 5))\n\
         end\n"
    );
    assert_eq!(
        run(&src),
        "#ff0000 #0000ff #ffffff #000000\n#c0ca33 #000000 none\ntrue 48 #ff8000\n100 80 #ff0000 #0000ff\ntrue false\n"
    );
    let e = error("use std.draw\nmain: print(draw.canvas(10, 10).get_pixel(1, 1, \"x\").foo)\n");
    assert!(!e.is_empty());
    let e = error("use std.draw\nmain:\n    let c = draw.canvas(10, 10)\n    c.rect(0, 0, 5, 5, \"not-a-colour\")\nend\n");
    assert!(e.contains("is not a colour"), "{e}");
}

#[cfg(feature = "draw")]
#[test]
fn turtles_draw_by_moving() {
    let out = run(
        "use std.draw\n\
         main:\n\
         \x20   let c = draw.canvas(200, 200)\n\
         \x20   let t = draw.turtle(c)\n\
         \x20   t.color(\"red\")\n\
         \x20   each _ in 0 until 4:\n        t.forward(50)\n        t.left(90)\n    end\n\
         \x20   print(round(t.x), round(t.y), t.heading)\n\
         \x20   print(c.get_pixel(125, 100), c.get_pixel(150, 75), c.get_pixel(125, 50), c.get_pixel(110, 90))\n\
         \x20   t.pen_up()\n    t.goto(-50, -50)\n    t.pen_down()\n\
         \x20   t.begin_fill()\n    t.forward(20)\n    t.left(90)\n    t.forward(20)\n    t.left(90)\n    t.forward(20)\n    t.end_fill()\n\
         \x20   print(c.get_pixel(60, 140))\n\
         end\n",
    );
    assert_eq!(out, "0 0 360.0\n#e53935 #e53935 #e53935 #000000\n#e53935\n");
}

#[cfg(feature = "draw")]
#[test]
fn three_d_hides_what_is_behind() {
    // The near red cube must cover the far blue one wherever they overlap,
    // whichever is drawn first.
    for order in ["near_first", "far_first"] {
        let draw_near = "    c.cube(cam, 0, 0, 0, size = 1, color = \"#ff0000\")\n";
        let draw_far = "    c.cube(cam, 0, 0, 3, size = 3, color = \"#0000ff\")\n";
        let body = if order == "near_first" {
            format!("{draw_near}{draw_far}")
        } else {
            format!("{draw_far}{draw_near}")
        };
        let src = format!(
            "use std.draw\nmain:\n    let c = draw.canvas(200, 200)\n    let cam = draw.camera(0, 0, -4)\n{body}\
             \x20   let centre = c.get_pixel(100, 100)\n    let edge = c.get_pixel(100, 60)\n\
             \x20   print(centre.startswith(\"#\"), centre != \"#000000\", edge != \"#000000\", centre != edge)\n\
             \x20   print(c.get_pixel(1, 1))\nend\n"
        );
        assert_eq!(run(&src), "true true true true\n#000000\n", "{order}");
    }
    let out = run(
        "use std.draw\nmain:\n    let cam = draw.camera(0, 0, -5)\n    cam.look_at(0, 0, 0)\n    cam.forward(1)\n    print(cam.z, cam.yaw)\n\
         \x20   let m = draw.mesh([[0, 0, 0], [1, 0, 0], [0, 1, 0]], [[0, 1, 2]])\n    print(m, draw.cube_mesh(), draw.sphere_mesh(4).triangles)\nend\n",
    );
    assert_eq!(
        out,
        "-4.0 0.0\nMesh(3 points, 1 triangles) Mesh(8 points, 12 triangles) 64\n"
    );
}

#[cfg(feature = "draw")]
#[test]
fn windows_without_a_screen_explain() {
    if std::env::var_os("DISPLAY").is_some() || std::env::var_os("WAYLAND_DISPLAY").is_some() {
        return;
    }
    let e = error("use std.draw\nmain: draw.window(100, 100, \"x\")\n");
    assert!(e.starts_with("DrawError: cannot open a window"), "{e}");
    assert!(e.contains("draw.canvas"), "{e}");
}
