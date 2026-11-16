//! The interactive prompt: variables and declarations last for the
//! session, lone expressions show their value, and blocks need more lines.

use vyrn::repl::{Outcome, Output, Session};
use vyrn::vm::Captured;

struct Repl {
    session: Session,
    out: Captured,
}

impl Repl {
    fn new() -> Repl {
        let out = Captured::default();
        Repl {
            session: Session::new(Output::Captured(out.clone())),
            out,
        }
    }

    /// Runs an entry and returns what it printed plus the value it shows.
    fn eval(&mut self, input: &str) -> String {
        let before = self.out.text().len();
        let shown = match self.session.eval(input) {
            Outcome::Done(Some(v)) => v,
            Outcome::Done(None) => String::new(),
            Outcome::NeedsMore => "<more>".to_string(),
            Outcome::Failed(e) => format!("<error> {e}"),
        };
        format!("{}{shown}", &self.out.text()[before..])
    }
}

#[test]
fn variables_and_values_last_for_the_session() {
    let mut r = Repl::new();
    assert_eq!(r.eval("x = 40"), "");
    assert_eq!(r.eval("x + 2"), "42");
    assert_eq!(r.eval("x += 1"), "");
    assert_eq!(r.eval("print(x)"), "41\n");
    assert_eq!(r.eval("names = [\"Ada\", \"Alan\"]"), "");
    assert_eq!(r.eval("names.append(\"Grace\")"), "");
    assert_eq!(r.eval("names"), "[\"Ada\", \"Alan\", \"Grace\"]");
    assert_eq!(r.eval("\"hi\""), "\"hi\"");
    assert_eq!(r.eval("let fixed = 3"), "");
    assert_eq!(r.eval("fixed * x"), "123");
    assert_eq!(r.eval("a, b = 1, 2"), "");
    assert_eq!(r.eval("a + b"), "3");
}

#[test]
fn declarations_last_for_the_session() {
    let mut r = Repl::new();
    assert_eq!(r.eval("create double(n):\n    return n * 2\nend"), "");
    assert_eq!(r.eval("double(21)"), "42");
    assert_eq!(r.eval("struct Point:\n    x: float\n    y: float\nend"), "");
    assert_eq!(r.eval("p = Point(3.0, 4.0)"), "");
    assert_eq!(
        r.eval("create length(p: Point) -> float: return sqrt(p.x * p.x + p.y * p.y)"),
        ""
    );
    assert_eq!(r.eval("length(p)"), "5.0");
    assert_eq!(r.eval("use vyrn.math"), "");
    assert_eq!(r.eval("math.floor(2.7)"), "2");
    // Functions stored in variables keep working as declarations are added.
    assert_eq!(r.eval("f = (n) => double(n) + 1"), "");
    assert_eq!(r.eval("create triple(n): return n * 3"), "");
    assert_eq!(r.eval("f(1) + triple(1)"), "6");
    assert_eq!(r.eval("[3, 1, 2].map(double)"), "[6, 2, 4]");
}

#[test]
fn blocks_need_more_lines() {
    let mut r = Repl::new();
    assert_eq!(r.eval("each i in 1 to 3:"), "<more>");
    assert_eq!(r.eval("each i in 1 to 3:\n    print(i)"), "<more>");
    assert_eq!(r.eval("each i in 1 to 3:\n    print(i)\nend"), "1\n2\n3\n");
    assert_eq!(r.eval("create f(x):"), "<more>");
    assert_eq!(r.eval("if true: print(\"one line\")"), "one line\n");
}

#[test]
fn errors_leave_the_session_usable() {
    let mut r = Repl::new();
    assert_eq!(r.eval("x = 1"), "");
    let e = r.eval("y = [1][9]");
    assert!(e.starts_with("<error> error: IndexError"), "{e}");
    let e = r.eval("x = x + )");
    assert!(e.starts_with("<error>"), "{e}");
    let e = r.eval("nope(1)");
    assert!(e.contains("`nope` is not defined"), "{e}");
    assert!(e.contains("input:1"), "{e}");
    assert_eq!(r.eval("x"), "1");
    // A failed declaration is not kept.
    assert!(r
        .eval("create g(): return missing_name")
        .starts_with("<error>"));
    assert_eq!(r.eval("create g(): return 7"), "");
    assert_eq!(r.eval("g()"), "7");
}
