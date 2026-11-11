//! The application libraries: vyrn.db, vyrn.draw, vyrn.html, vyrn.template,
//! vyrn.table, vyrn.chart and vyrn.mail.

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
        "use vyrn.db\n\
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
    let e = error("use vyrn.db\nmain:\n    let c = db.open(\":memory:\")\n    c.execute(\"select * from missing\")\nend\n");
    assert!(
        e.starts_with("DatabaseError: no such table: missing"),
        "{e}"
    );
    let e = error("use vyrn.db\nmain:\n    let c = db.open(\":memory:\")\n    c.execute(\"create table t (a int)\")\n    c.execute(\"insert into t values (?)\", [1, 2])\nend\n");
    assert!(e.contains("takes 1 values, but 2 were given"), "{e}");
    let e = error("use vyrn.db\nmain:\n    let c = db.open(\":memory:\")\n    c.close()\n    c.query(\"select 1\")\nend\n");
    assert!(e.contains("is closed"), "{e}");
}

#[cfg(feature = "draw")]
#[test]
fn drawing_on_a_canvas() {
    let png = temp("pic.png");
    let src = format!(
        "use vyrn.draw\n\
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
    let e = error("use vyrn.draw\nmain: print(draw.canvas(10, 10).get_pixel(1, 1, \"x\").foo)\n");
    assert!(!e.is_empty());
    let e = error("use vyrn.draw\nmain:\n    let c = draw.canvas(10, 10)\n    c.rect(0, 0, 5, 5, \"not-a-colour\")\nend\n");
    assert!(e.contains("is not a colour"), "{e}");
}

#[cfg(feature = "draw")]
#[test]
fn turtles_draw_by_moving() {
    let out = run(
        "use vyrn.draw\n\
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
    assert_eq!(out, "0 0 0.0\n#e53935 #e53935 #e53935 #000000\n#e53935\n");
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
            "use vyrn.draw\nmain:\n    let c = draw.canvas(200, 200)\n    let cam = draw.camera(0, 0, -4)\n{body}\
             \x20   let centre = c.get_pixel(100, 100)\n    let edge = c.get_pixel(100, 60)\n\
             \x20   print(centre.startswith(\"#\"), centre != \"#000000\", edge != \"#000000\", centre != edge)\n\
             \x20   print(c.get_pixel(1, 1))\nend\n"
        );
        assert_eq!(run(&src), "true true true true\n#000000\n", "{order}");
    }
    let out = run(
        "use vyrn.draw\nmain:\n    let cam = draw.camera(0, 0, -5)\n    cam.look_at(0, 0, 0)\n    cam.forward(1)\n    print(cam.z, cam.yaw)\n\
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
    let e = error("use vyrn.draw\nmain: draw.window(100, 100, \"x\")\n");
    assert!(e.starts_with("DrawError: cannot open a window"), "{e}");
    assert!(e.contains("draw.canvas"), "{e}");
}

#[cfg(feature = "html")]
#[test]
fn parsing_html() {
    let out = run(
        "use vyrn.html\n\
         main:\n\
         \x20   let page = html.parse(\"\"\"\n\
         <html><head><title> Shop  </title></head><body>\n\
         <div id=\"main\"><h1 class=\"big title\">Books &amp; more</h1>\n\
         <ul><li class=\"item\" data-price=\"12\">Rust <b>book</b></li><li class=\"item sale\" data-price=\"8\">Vyrn guide</li></ul>\n\
         <a href=\"/a\">first</a><a>no link</a><a href=\"/b\">second</a>\n\
         <p>unclosed <p>tags</div></body></html>\"\"\")\n\
         \x20   print(page.title)\n\
         \x20   let h = page.select_one(\"h1\")\n\
         \x20   print(h.text, h.tag, h.classes, h.has_class(\"big\"), h.parent.id)\n\
         \x20   each li in page.select(\"li.item\"): print(li.text, int(li.attr(\"data-price\")), li.attr(\"nope\"))\n\
         \x20   print([a.attr(\"href\") each a in page.select(\"a[href]\")], len(page.select(\"p\")))\n\
         \x20   print(page.select_one(\"li.sale\").text, page.select_one(\"table\"), page.select_one(\"#main\").select(\"b\")[0].html)\n\
         \x20   print(html.escape(\"<a href='x'>&</a>\"))\n\
         end\n",
    );
    assert_eq!(
        out,
        "Shop\nBooks & more h1 [\"big\", \"title\"] true main\nRust book 12 none\nVyrn guide 8 none\n[\"/a\", \"/b\"] 2\nVyrn guide none book\n&lt;a href=&#39;x&#39;&gt;&amp;&lt;/a&gt;\n"
    );
    assert!(
        error("use vyrn.html\nmain: html.parse(\"<p>\").select(\"p[[\")\n")
            .contains("is not a CSS selector")
    );
}

#[test]
fn templates() {
    let out = run(
        "use vyrn.template\n\
         struct Item:\n    name: str\n    price: float\nend\n\
         main:\n\
         \x20   let items = [Item(\"pen\", 1.5), Item(\"<script>\", 2.0)]\n\
         \x20   let page = template.render(r\"<h1>{{title}}</h1>{{! a comment }}{{#each items}}<li>{{@index}}. {{name}}: {{price}}</li>{{/each}}{{#if empty}}none{{else}}{{count}} items{{/if}}{{#unless empty}}!{{/unless}} {{{raw}}} {{user.name}} {{missing}}.\", {\"title\": \"Shop & Co\", \"items\": items, \"empty\": false, \"count\": len(items), \"raw\": \"<b>hi</b>\", \"user\": {\"name\": \"Ada\"}})\n\
         \x20   print(page)\n\
         \x20   print(template.render(r\"{{#each xs}}[{{this}}]{{/each}}\", {\"xs\": [1, 2, 3]}))\n\
         end\n",
    );
    assert_eq!(
        out,
        "<h1>Shop &amp; Co</h1><li>0. pen: 1.5</li><li>1. &lt;script&gt;: 2.0</li>2 items! <b>hi</b> Ada .\n[1][2][3]\n"
    );
    assert!(
        error("use vyrn.template\nmain: template.render(r\"{{#each xs}}oops\", {})\n")
            .contains("never closed")
    );
    assert!(
        error("use vyrn.template\nmain: template.render(r\"{{/if}}\", {})\n")
            .contains("does not close")
    );
    // Template syntax in an ordinary string is explained.
    let e = match run_source("main: print(\"{{name}}\")\n") {
        Err(RunError::Compile(errors)) => errors[0].hint.clone().unwrap_or_default(),
        _ => String::new(),
    };
    assert!(e.contains("raw string"), "{e}");
}

#[test]
fn tables_of_data() {
    let out = run(
        "use vyrn.table\n\
         main:\n\
         \x20   let t = table.from_csv(\"region,amount,rep\\nnorth,120,Ada\\nsouth,80,Alan\\nnorth,150,\\neast,60.5,Grace\\n\")\n\
         \x20   print(t.shape, t.columns, len(t))\n\
         \x20   print(t.sum(\"amount\"), t.mean(\"amount\"), t.median(\"amount\"), t.min(\"amount\"), t.max(\"region\"), t.count(\"rep\"))\n\
         \x20   print(t.where(\"amount\", \">\", 100).column(\"rep\"), t.where(\"rep\", \"contains\", \"A\").column(\"region\"))\n\
         \x20   print(t.sort(\"amount\", descending = true).column(\"amount\"), t.sort(\"rep\").column(\"rep\"))\n\
         \x20   print(t.group_by(\"region\", \"amount\", \"sum\").rows)\n\
         \x20   print(t.group_by(\"region\").column(\"count\"), t.unique(\"region\"), t.row(-1))\n\
         \x20   let u = t.select([\"rep\", \"amount\"]).add_column(\"double\", [x * 2 each x in t.column(\"amount\")])\n\
         \x20   print(u.head(2))\n\
         \x20   print(u.to_csv().lines())\n\
         \x20   let m = table.from_rows([{\"a\": 1, \"b\": \"x\"}, {\"a\": 2, \"c\": true}])\n\
         \x20   print(m.columns, m.rows)\n\
         \x20   each row in table.from_rows([[1, 2], [3, 4]], [\"p\", \"q\"]): print(row[\"p\"] + row[\"q\"])\n\
         end\n",
    );
    assert_eq!(
        out,
        "(4, 3) [\"region\", \"amount\", \"rep\"] 4\n\
         410.5 102.625 100.0 60.5 south 3\n\
         [\"Ada\", none] [\"north\", \"south\"]\n\
         [150, 120, 80, 60.5] [\"Ada\", \"Alan\", \"Grace\", none]\n\
         [{\"region\": \"north\", \"amount_sum\": 270}, {\"region\": \"south\", \"amount_sum\": 80}, {\"region\": \"east\", \"amount_sum\": 60.5}]\n\
         [2, 1, 1] [\"north\", \"south\", \"east\"] {\"region\": \"east\", \"amount\": 60.5, \"rep\": \"Grace\"}\n\
         rep   amount  double\n\
         ----  ------  ------\n\
         Ada      120     240\n\
         Alan      80     160\n\
         [\"rep,amount,double\", \"Ada,120,240\", \"Alan,80,160\", \",150,300\", \"Grace,60.5,121.0\"]\n\
         [\"a\", \"b\", \"c\"] [{\"a\": 1, \"b\": \"x\", \"c\": none}, {\"a\": 2, \"b\": none, \"c\": true}]\n\
         3\n7\n"
    );
    let e = error("use vyrn.table\nmain: table.from_csv(\"a,b\\n1,2\\n\").sum(\"c\")\n");
    assert!(e.contains("no column \"c\"; the columns are a, b"), "{e}");
}

#[test]
fn charts() {
    let svg = temp("c.svg");
    let out = run(&format!(
        "use vyrn.chart\nuse vyrn.fs\n\
         main:\n\
         \x20   chart.bar([\"a\", \"b\"], [3, 7], title = \"Votes\").save(\"{svg}\")\n\
         \x20   let text = fs.read_text(\"{svg}\")\n\
         \x20   print(text.startswith(\"<svg\"), \"Votes\" in text, text.count(\"<rect\"))\n\
         \x20   let s = chart.line([1, 2, 3], {{\"x\": [1, 4, 9], \"y\": [2, 3, 4]}}).svg()\n\
         \x20   print(s.count(\"<line\") > 4, \">x<\" in s and \">y<\" in s)\n\
         \x20   print(chart.pie([\"p\", \"q\"], [1, 3]).svg().count(\"<polygon\"), chart.hist([1, 2, 2, 3], bins = 3).svg().count(\"<rect\"))\n\
         end\n"
    ));
    assert_eq!(out, "true true 3\ntrue true\n2 4\n");
    assert!(
        error("use vyrn.chart\nmain: chart.line([1, 2], [1, 2, 3])\n")
            .contains("2 x values but 3 y values")
    );
    #[cfg(feature = "draw")]
    {
        let png = temp("c.png");
        run(&format!("use vyrn.chart\nmain: chart.scatter([1, 2, 3], [3, 1, 2], title = \"Dots\").save(\"{png}\")\n"));
        let out = run(&format!(
            "use vyrn.draw\nmain:\n    let img = draw.image(\"{png}\")\n    print(img.width, img.height)\nend\n"
        ));
        assert_eq!(out, "640 400\n");
    }
}

#[cfg(feature = "mail")]
#[test]
fn sending_mail() {
    use std::io::{BufRead, BufReader, Write};
    // A tiny SMTP server that accepts one message and hands it back.
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = std::thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        let mut reader = BufReader::new(stream.try_clone().unwrap());
        let mut out = stream;
        let mut data = String::new();
        let mut in_data = false;
        writeln!(out, "220 test ready\r").unwrap();
        loop {
            let mut line = String::new();
            if reader.read_line(&mut line).unwrap() == 0 {
                break;
            }
            if in_data {
                if line == ".\r\n" {
                    in_data = false;
                    write!(out, "250 queued\r\n").unwrap();
                } else {
                    data.push_str(&line);
                }
                continue;
            }
            let upper = line.to_uppercase();
            if upper.starts_with("EHLO") || upper.starts_with("HELO") {
                write!(out, "250-test\r\n250 OK\r\n").unwrap();
            } else if upper.starts_with("DATA") {
                in_data = true;
                write!(out, "354 go\r\n").unwrap();
            } else if upper.starts_with("QUIT") {
                write!(out, "221 bye\r\n").unwrap();
                break;
            } else {
                data.push_str(&line);
                write!(out, "250 OK\r\n").unwrap();
            }
        }
        data
    });
    run(&format!(
        "use vyrn.mail\nmain:\n    mail.send(to = [\"a@example.com\", \"b@example.com\"], subject = \"Hi there\", body = \"Hello from Vyrn\", sender = \"me@example.com\", server = \"127.0.0.1\", port = {port}, security = \"none\")\nend\n"
    ));
    let data = server.join().unwrap();
    assert!(data.contains("MAIL FROM:<me@example.com>"), "{data}");
    assert!(
        data.contains("RCPT TO:<a@example.com>") && data.contains("RCPT TO:<b@example.com>"),
        "{data}"
    );
    assert!(data.contains("Subject: Hi there"), "{data}");
    assert!(data.contains("Hello from Vyrn"), "{data}");
    let e = error("use vyrn.mail\nmain: mail.send(to = \"not an address\", sender = \"me@example.com\", server = \"x\")\n");
    assert!(e.contains("is not an email address"), "{e}");
}

#[test]
fn databases_store_records_without_sql() {
    let out = run("use vyrn.db\n\
         struct Pet:\n    name: str\n    age: int\nend\n\
         main:\n\
         \x20   let conn = db.open(\":memory:\")\n\
         \x20   conn.create_table(\"pets\", {\"name\": \"text\", \"age\": \"int\"})\n\
         \x20   conn.insert(\"pets\", Pet(\"Rex\", 3))\n\
         \x20   let id = conn.insert(\"pets\", {\"name\": \"Tom\", \"age\": 5})\n\
         \x20   conn.update(\"pets\", {\"id\": id}, {\"age\": 6})\n\
         \x20   print(conn.find_one(\"pets\", {\"name\": \"Tom\"}))\n\
         \x20   print(conn.delete(\"pets\", {\"name\": \"Rex\"}), conn.all(\"pets\"))\n\
         end\n");
    assert_eq!(
        out,
        "{\"id\": 2, \"name\": \"Tom\", \"age\": 6}\n1 [{\"id\": 2, \"name\": \"Tom\", \"age\": 6}]\n"
    );
}
