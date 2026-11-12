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
        "-4.0 0.0\nMesh(3 points, 1 triangles) Mesh(24 points, 12 triangles) 64\n"
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

#[test]
fn games_raycast_collide_and_slide() {
    let out = run("use vyrn.game\n\
         main:\n\
         \x20   let world = [[0, 1, 4, 1, 1, 1], [0, 1, 10, 0.8, 2, 0.8]]\n\
         \x20   print(game.raycast([0, 1, 0], [0, 0, 1], world)[\"index\"], game.raycast([0, 1.8, 0], [0, 0, 1], world)[\"index\"])\n\
         \x20   print(game.raycast([0, 1, 0], [0, 0, -1], world), game.overlaps3d([0, 0, 0, 1, 1, 1], [0.8, 0, 0, 1, 1, 1]))\n\
         \x20   print(game.slide([0, 1, 4.0], [0.5, 0, 0.5], [[0, 1, 5, 10, 2, 1]], [0.6, 1.8, 0.6]))\n\
         end\n");
    assert_eq!(out, "0 1\nnone true\n[0.5, 1.0, 4.0]\n");
}

#[test]
fn cameras_aim_and_strafe() {
    let out = run("use vyrn.draw\n\
         main:\n\
         \x20   let cam = draw.camera(0, 0, 0)\n\
         \x20   cam.turn(90, 0)\n\
         \x20   cam.strafe(2)\n\
         \x20   print([round(v, 2) each v in cam.direction], [round(v, 2) each v in cam.position])\n\
         end\n");
    assert_eq!(out, "[1.0, 0.0, 0.0] [0.0, 0.0, -2.0]\n");
}

#[test]
fn programs_talk_over_udp_and_tcp() {
    let out = run("use vyrn.net\nuse vyrn.time\n\
         main:\n\
         \x20   let server = net.udp()\n\
         \x20   let client = net.udp()\n\
         \x20   client.send(\"127.0.0.1:{server.port}\", {\"x\": 1, \"names\": [\"a\"]})\n\
         \x20   time.sleep(0.05)\n\
         \x20   let m = server.receive()\n\
         \x20   print(m[\"data\"])\n\
         \x20   server.send(m[\"from\"], \"pong\")\n\
         \x20   time.sleep(0.05)\n\
         \x20   print(client.receive()[\"data\"], client.receive())\n\
         \x20   let lobby = net.listen()\n\
         \x20   let c = net.connect(\"127.0.0.1:{lobby.port}\")\n\
         \x20   time.sleep(0.05)\n\
         \x20   let conn = lobby.accept()\n\
         \x20   c.send(\"hi\")\n\
         \x20   c.send([1, 2])\n\
         \x20   time.sleep(0.05)\n\
         \x20   print(conn.receive_all())\n\
         end\n");
    assert_eq!(
        out,
        "{\"x\": 1, \"names\": [\"a\"]}\npong none\n[\"hi\", [1, 2]]\n"
    );
}

#[test]
fn sounds_are_made_saved_and_loaded() {
    let dir = std::env::temp_dir().join(format!("vyrn-sound-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let wav = dir.join("beep.wav");
    let out = run(&format!(
        "use vyrn.sound\n\
         main:\n\
         \x20   let beep = sound.tone(440, 0.5, wave = \"square\")\n\
         \x20   beep.save({wav:?})\n\
         \x20   let back = sound.load({wav:?})\n\
         \x20   print(back.duration, back.channels, back.sample_rate)\n\
         \x20   let p = back.play(volume = 0.1)\n\
         \x20   p.stop()\n\
         \x20   print(p.playing)\n\
         end\n"
    ));
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(out, "0.5 1 44100\nfalse\n");
}

#[test]
fn the_gpu_draws_the_same_picture_as_the_software_renderer() {
    let out = run("use vyrn.draw\n\
         create scene(c, cam):\n\
         \x20   c.clear(\"#87b5e0\")\n\
         \x20   c.box(cam, [0, -0.05, 0, 30, 0.1, 30], \"#5d7a4a\")\n\
         \x20   c.box(cam, [0, 1, 5, 2, 2, 2], \"tomato\")\n\
         \x20   c.box(cam, [0.5, 1, 3.5, 1, 1, 1], [0, 0, 255, 128])\n\
         \x20   c.box(cam, [0, 1, 0.5, 6, 2, 3], \"red\")\n\
         \x20   c.rect(95, 45, 10, 10, \"white\")\n\
         end\n\
         main:\n\
         \x20   var seen = []\n\
         \x20   each on in [false, true]:\n\
         \x20       if draw.use_gpu(on) == on:\n\
         \x20           let c = draw.canvas(200, 100)\n\
         \x20           scene(c, draw.camera(0, 1.6, -3))\n\
         \x20           seen.append([c.get_pixel(x, y) each x, y in [(100, 50), (100, 5), (20, 95), (110, 70), (190, 60)]])\n\
         \x20       end\n\
         \x20   end\n\
         \x20   draw.use_gpu(false)\n\
         \x20   print(len(seen) == 1 or seen[0] == seen[1], seen[0])\n\
         end\n");
    assert_eq!(
        out,
        "true [\"#ffffff\", \"#87b5e0\", \"#932422\", \"#932422\", \"#932422\"]\n"
    );
}

#[test]
fn textures_lights_and_fog_look_the_same_on_the_gpu() {
    let out = run("use vyrn.draw\n\
         create checker():\n\
         \x20   let t = draw.canvas(4, 4)\n\
         \x20   t.clear(\"white\")\n\
         \x20   each x, y in [(0, 0), (2, 0), (1, 1), (3, 1), (0, 2), (2, 2), (1, 3), (3, 3)]:\n\
         \x20       t.pixel(x, y, \"black\")\n\
         \x20   end\n\
         \x20   return t.to_image()\n\
         end\n\
         create picture(tex):\n\
         \x20   let c = draw.canvas(200, 100)\n\
         \x20   let cam = draw.camera(0, 2, 0)\n\
         \x20   c.clear(\"#202020\")\n\
         \x20   c.ambient(0.3)\n\
         \x20   c.light([0, 1, 0], \"white\", 0.7)\n\
         \x20   c.box(cam, [0, -0.05, 6, 8, 0.1, 8], \"white\", texture = tex)\n\
         \x20   c.point_light(2, 1, 3, \"red\", 2.0, 4)\n\
         \x20   c.box(cam, [2, 0.5, 4, 1, 1, 1], \"white\")\n\
         \x20   return [c.get_pixel(x, y) each y in 0 until 100 each x in 0 until 200]\n\
         end\n\
         main:\n\
         \x20   let tex = checker()\n\
         \x20   draw.use_gpu(false)\n\
         \x20   let soft = picture(tex)\n\
         \x20   # The floor shows both squares; the box is lit red.\n\
         \x20   let floor = soft[70 * 200:]\n\
         \x20   print(floor.count(\"#ffffff\") > 500, floor.count(\"#000000\") > 500, soft[80 * 200 + 145], soft[20 * 200 + 20])\n\
         \x20   if draw.use_gpu(true):\n\
         \x20       let gpu = picture(tex)\n\
         \x20       var differ = 0\n\
         \x20       each i in 0 until len(soft):\n\
         \x20           if soft[i] != gpu[i]: differ += 1\n\
         \x20       end\n\
         \x20       # Only texel edges far away may round differently.\n\
         \x20       print(differ < len(soft) / 10)\n\
         \x20   else:\n\
         \x20       print(true)\n\
         \x20   end\n\
         \x20   draw.use_gpu(false)\n\
         \x20   let c = draw.canvas(100, 100)\n\
         \x20   c.clear(\"black\")\n\
         \x20   c.fog(\"#8090a0\", start = 1, end = 5)\n\
         \x20   c.box(draw.camera(0, 0, -10), [0, 0, 0, 4, 4, 1], \"white\")\n\
         \x20   print(c.get_pixel(50, 50), c.get_pixel(1, 1))\n\
         end\n");
    assert_eq!(out, "true true #ea7b78 #202020\ntrue\n#8090a0 #000000\n");
}

/// A tiny skinned model: an "arm" of two joints (a root, and an elbow one
/// unit up) with one triangle whose top corner hangs on the elbow, and an
/// animation "bend" turning the elbow a quarter turn about z in a second.
fn arm_gltf() -> String {
    let mut b: Vec<u8> = Vec::new();
    let f = |b: &mut Vec<u8>, xs: &[f32]| {
        xs.iter()
            .for_each(|x| b.extend_from_slice(&x.to_le_bytes()))
    };
    // Positions (offset 0, 36 bytes).
    f(&mut b, &[0.0, 0.0, 0.0, 0.2, 0.0, 0.0, 0.0, 2.0, 0.0]);
    // Joints, four u16 per point (36, 24 bytes).
    for j in [0u16, 0, 1] {
        for k in [j, 0, 0, 0] {
            b.extend_from_slice(&k.to_le_bytes());
        }
    }
    // Weights (60, 48 bytes).
    f(
        &mut b,
        &[1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0],
    );
    // Inverse bind matrices (108, 128 bytes): identity, then down by one.
    f(
        &mut b,
        &[
            1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1.,
        ],
    );
    f(
        &mut b,
        &[
            1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1., 0., 0., -1., 0., 1.,
        ],
    );
    // Key times (236, 8 bytes) and rotations (244, 32 bytes).
    f(&mut b, &[0.0, 1.0]);
    let s = std::f32::consts::FRAC_1_SQRT_2;
    f(&mut b, &[0.0, 0.0, 0.0, 1.0, 0.0, 0.0, s, s]);
    // Indices (276, 6 bytes, padded to 8).
    for i in [0u16, 1, 2, 0] {
        b.extend_from_slice(&i.to_le_bytes());
    }
    const TABLE: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut b64 = String::new();
    for chunk in b.chunks(3) {
        let n = (u32::from(chunk[0]) << 16)
            | (u32::from(*chunk.get(1).unwrap_or(&0)) << 8)
            | u32::from(*chunk.get(2).unwrap_or(&0));
        for k in 0..4 {
            if k <= chunk.len() {
                b64.push(TABLE[(n >> (18 - 6 * k) & 63) as usize] as char);
            } else {
                b64.push('=');
            }
        }
    }
    format!(
        r#"{{"asset":{{"version":"2.0"}},"scene":0,"scenes":[{{"nodes":[0,2]}}],
"nodes":[{{"name":"root","children":[1]}},{{"name":"elbow","translation":[0,1,0]}},{{"mesh":0,"skin":0}}],
"skins":[{{"joints":[0,1],"inverseBindMatrices":3}}],
"meshes":[{{"primitives":[{{"attributes":{{"POSITION":0,"JOINTS_0":1,"WEIGHTS_0":2}},"indices":6}}]}}],
"animations":[{{"name":"bend","channels":[{{"sampler":0,"target":{{"node":1,"path":"rotation"}}}}],"samplers":[{{"input":4,"output":5}}]}}],
"buffers":[{{"byteLength":{len},"uri":"data:application/octet-stream;base64,{b64}"}}],
"bufferViews":[{{"buffer":0,"byteOffset":0,"byteLength":36}},{{"buffer":0,"byteOffset":36,"byteLength":24}},{{"buffer":0,"byteOffset":60,"byteLength":48}},{{"buffer":0,"byteOffset":108,"byteLength":128}},{{"buffer":0,"byteOffset":236,"byteLength":8}},{{"buffer":0,"byteOffset":244,"byteLength":32}},{{"buffer":0,"byteOffset":276,"byteLength":6}}],
"accessors":[{{"bufferView":0,"componentType":5126,"count":3,"type":"VEC3","min":[0,0,0],"max":[0.2,2,0]}},{{"bufferView":1,"componentType":5123,"count":3,"type":"VEC4"}},{{"bufferView":2,"componentType":5126,"count":3,"type":"VEC4"}},{{"bufferView":3,"componentType":5126,"count":2,"type":"MAT4"}},{{"bufferView":4,"componentType":5126,"count":2,"type":"SCALAR","min":[0],"max":[1]}},{{"bufferView":5,"componentType":5126,"count":2,"type":"VEC4"}},{{"bufferView":6,"componentType":5123,"count":3,"type":"SCALAR"}}]}}"#,
        len = b.len()
    )
}

#[test]
fn animated_models_bend_their_skin_with_the_skeleton() {
    let dir = std::env::temp_dir().join(format!("vyrn-gltf-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("arm.gltf");
    std::fs::write(&path, arm_gltf()).unwrap();
    let p = path.display().to_string().replace('\\', "/");
    let out = run(&format!(
        "use vyrn.draw\n\
         main:\n\
         \x20   let arm = draw.load_model(\"{p}\")\n\
         \x20   print(arm, arm.animations, arm.duration(\"bend\"), arm.joints)\n\
         \x20   print(arm.mesh().vertices[2], arm.mesh(\"bend\", 0.5).vertices[2], arm.mesh(\"bend\", 1, loop = false).vertices[2])\n\
         \x20   # Looping: 1.5 seconds is half way through again.\n\
         \x20   print(arm.mesh(\"bend\", 1.5).vertices[2], arm.mesh(\"bend\", 0.5).vertices[0])\n\
         \x20   let c = draw.canvas(60, 60)\n\
         \x20   c.clear(\"black\")\n\
         \x20   c.model(draw.camera(0, 1, -4), arm, 0, 0, 0, animation = \"bend\", time = 0.25)\n\
         \x20   attempt:\n\
         \x20       arm.mesh(\"wave\")\n\
         \x20   catch ValueError as e:\n\
         \x20       print(e)\n\
         \x20   end\n\
         end\n"
    ));
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(
        out,
        "Model(3 points, 1 animations) [\"bend\"] 1.0 2\n\
         [0.0, 2.0, 0.0] [-0.70711, 1.70711, 0.0] [-1.0, 1.0, 0.0]\n\
         [-0.70711, 1.70711, 0.0] [0.0, 0.0, 0.0]\n\
         ValueError: this model has no animation \"wave\"; it has bend\n"
    );
}

#[cfg(feature = "sound")]
#[test]
fn sounds_in_the_world_pan_and_fade_from_the_listener() {
    let out = run("use vyrn.sound\nuse vyrn.draw\n\
         main:\n\
         \x20   let beep = sound.tone(440, 0.05, volume = 0.0)\n\
         \x20   sound.listener(0, 0, 0)\n\
         \x20   let right = beep.play(at = [3, 0, 0])\n\
         \x20   print(right.balance, beep.play(at = [0, 0, 1]).balance, beep.play(at = [0, 0, 40], range = 30).balance)\n\
         \x20   sound.listener(0, 0, 0, yaw = 90)\n\
         \x20   print(right.balance)\n\
         \x20   right.move_to(0, 0, -3)\n\
         \x20   print(right.balance)\n\
         \x20   let cam = draw.camera(0, 0, 0)\n\
         \x20   sound.listener(cam)\n\
         \x20   print(right.balance, beep.play().balance)\n\
         end\n");
    assert_eq!(
        out,
        "[0.0, 0.795] [0.967, 0.967] [0.0, 0.0]\n[0.563, 0.563]\n[0.0, 0.795]\n[0.563, 0.563] none\n"
    );
}

#[cfg(feature = "physics")]
#[test]
fn physics_bodies_fall_collide_and_characters_walk() {
    let out = run("use vyrn.physics\n\
         main:\n\
         \x20   let world = physics.world()\n\
         \x20   let floor = world.box(0, -0.5, 0, 20, 1, 20, fixed = true, name = \"floor\")\n\
         \x20   let crate = world.box(0, 5, 0, 1, 1, 1, mass = 2, name = \"crate\")\n\
         \x20   var hits = []\n\
         \x20   each _ in 0 until 120:\n\
         \x20       world.step(1 / 60)\n\
         \x20       each pair in world.collisions(): hits.append(sorted([b.name each b in pair]))\n\
         \x20   end\n\
         \x20   print(crate.y.round(2), crate.mass, crate.size, hits, [b.name each b in crate.touching()])\n\
         \x20   let hit = world.raycast([0, 10, 0], [0, -1, 0])\n\
         \x20   print(hit[\"body\"].name, hit[\"distance\"].round(2), world.raycast([0, 10, 0], [0, 1, 0]))\n\
         \x20   crate.push(0, 10, 0)\n\
         \x20   world.step(1 / 60)\n\
         \x20   print(crate.velocity[1] > 3)\n\
         \x20   world.box(5, 1, 0, 1, 2, 10, fixed = true)\n\
         \x20   world.box(-3, 0.1, 0, 2, 0.2, 2, fixed = true)\n\
         \x20   let player = world.character(1, 1, 0)\n\
         \x20   each _ in 0 until 100:\n\
         \x20       player.move(0.1, -0.3, 0)\n\
         \x20       world.step(1 / 60)\n\
         \x20   end\n\
         \x20   print(player.x.round(1), player.grounded)\n\
         \x20   # Out of the way, then walk up the step.\n\
         \x20   world.remove(crate)\n\
         \x20   player.move_to(1, 1, 0)\n\
         \x20   var peak = 0.0\n\
         \x20   each _ in 0 until 120:\n\
         \x20       player.move(-0.05, -0.3, 0)\n\
         \x20       world.step(1 / 60)\n\
         \x20       if player.x < -2.5 and player.x > -3.5: peak = max(peak, player.y)\n\
         \x20   end\n\
         \x20   print(peak.round(2), player.y.round(2))\n\
         \x20   print(len(world.bodies))\n\
         end\n");
    assert_eq!(
        out,
        "0.5 2.0 [1.0, 1.0, 1.0] [[\"crate\", \"floor\"]] [\"floor\"]\ncrate 9.0 none\ntrue\n4.1 true\n1.12 0.92\n4\n"
    );
}

#[test]
fn a_tiny_model_generates_text_and_files_map() {
    let dir = std::env::temp_dir().join(format!("vyrn-nn-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let d = dir.display().to_string().replace('\\', "/");
    let out = run(&format!(
        "use vyrn.nn\nuse vyrn.fs\nuse vyrn.math\nuse vyrn.random\n\
         const TEXT = \"the cat sat on the mat. the cat ate the rat.\"\n\
         main:\n\
         \x20   let letters = sorted(list(set(TEXT)))\n\
         \x20   let index = {{c: i each i, c in enumerate(letters)}}\n\
         \x20   let n = len(letters)\n\
         \x20   var counts = [[0.01 each _ in 0 until n] each _ in 0 until n]\n\
         \x20   each i in 0 until len(TEXT) - 1: counts[index[TEXT[i]]][index[TEXT[i + 1]]] += 1\n\
         \x20   nn.save(\"{d}/bigram.safetensors\", {{\"table\": tensor([[math.log(x) each x in row] each row in counts])}})\n\
         \x20   let weights = nn.load(\"{d}/bigram.safetensors\")[\"table\"]\n\
         \x20   create step(tokens: list) -> list: return [weights[tokens[-1], j] each j in 0 until n]\n\
         \x20   let start = [index[\"t\"]]\n\
         \x20   print(\"t\" + \"\".join([letters[t] each t in nn.generate(step, start, max_new = 12, temperature = 0)]))\n\
         \x20   print(\"t\" + \"\".join([letters[t] each t in nn.generate(step, start, temperature = 0, stop_at = index[\" \"])]))\n\
         \x20   random.seed(3)\n\
         \x20   let a = list(nn.generate(step, start, 20, temperature = 1.0, top_k = 3))\n\
         \x20   random.seed(3)\n\
         \x20   print(a == list(nn.generate(step, start, 20, temperature = 1.0, top_k = 3)), len(a))\n\
         \x20   print(nn.sample([0.1, 5.0, 0.2], temperature = 0), nn.sample(tensor([[1.0, 0.0], [0.0, 9.0]]), 0), nn.sample([1.0, 1.0, 50.0], top_p = 0.5))\n\
         \x20   fs.write_text(\"{d}/log.txt\", \"INFO start\\nERROR disk\\nINFO ok\\nERROR net\\n\")\n\
         \x20   using fs.map(\"{d}/log.txt\") as data:\n\
         \x20       print(len(data), data.count_lines(), data.find(\"ERROR\"), data.find(\"ERROR\", 12), data.text(0, 10), data.bytes(0, 2))\n\
         \x20       print([line each line in data.lines() if line.startswith(\"ERROR\")])\n\
         \x20   end\n\
         \x20   let t = tensor([1.5, -2.0, 0.25])\n\
         \x20   nn.save(\"{d}/t.safetensors\", {{\"t\": t}})\n\
         \x20   let raw = fs.map(\"{d}/t.safetensors\")\n\
         \x20   print(fs.tensor_view(raw, \"f32\", [3], raw.size - 12))\n\
         end\n"
    ));
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(
        out,
        "the the the t\nthe\ntrue 20\n1 1 2\n40 4 11 30 INFO start [73, 78]\n[\"ERROR disk\", \"ERROR net\"]\ntensor([1.5, -2.0, 0.25])\n"
    );
}

#[test]
fn line_charts_take_text_on_the_x_axis() {
    let out = run("use vyrn.chart\n\
         main:\n\
         \x20   let c = chart.line([\"Jan\", \"Feb\", \"Mar\"], {\"a\": [1, 3, 2]}, title = \"°C\")\n\
         \x20   let svg = c.svg()\n\
         \x20   print(\"Jan\" in svg, \"Mar\" in svg)\n\
         end\n");
    assert_eq!(out, "true true\n");
}
