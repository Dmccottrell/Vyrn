//! `use std.html`: reading web pages, as BeautifulSoup does in Python.
//!
//! ```vyrn
//! use std.html
//! use std.http
//!
//! main:
//!     let page = html.parse(http.get("https://example.com").get("text"))
//!     print(page.title)
//!     each link in page.select("a[href]"):
//!         print(link.text, link.attr("href"))
//!     end
//! end
//! ```
//!
//! `select` takes CSS selectors (`"div.price"`, `"#main > p"`,
//! `"table tr td:first-child"`) and gives elements, which can be searched
//! again. Parsing follows the HTML standard, so broken pages read the way a
//! browser would read them.

use std::rc::Rc;

use super::foreign::Foreign;
use super::ops::{err, type_error, Ctx, VmResult};
use super::value::Value;

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

/// `html.escape(text)`: text made safe to put in a page.
pub fn escape(_: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    Ok(Value::str(escape_text(text_arg(args, 0, "the text")?)))
}

pub fn escape_text(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            c => out.push(c),
        }
    }
    out
}

#[cfg(feature = "html")]
mod imp {
    use super::*;
    use scraper::{ElementRef, Html, Selector};

    /// A parsed page, shared by the elements found in it.
    pub struct Document {
        pub html: Rc<Html>,
    }

    /// One element of a page.
    pub struct Element {
        pub html: Rc<Html>,
        pub id: ego_tree::NodeId,
    }

    fn selector(css: &str) -> VmResult<Selector> {
        Selector::parse(css)
            .map_err(|e| err("ValueError", format!("{css:?} is not a CSS selector: {e}")))
    }

    fn element(html: &Rc<Html>, e: ElementRef) -> Value {
        Value::Foreign(Rc::new(Box::new(Element {
            html: html.clone(),
            id: e.id(),
        })))
    }

    fn clean(text: String) -> String {
        text.split_whitespace().collect::<Vec<_>>().join(" ")
    }

    /// `select`, `select_one`, `find_all` and `find` on a page or element.
    fn search(
        html: &Rc<Html>,
        root: Option<ElementRef>,
        name: &str,
        args: &[Value],
    ) -> Option<VmResult<Value>> {
        let one = matches!(name, "select_one" | "find");
        if !matches!(name, "select" | "select_one" | "find_all" | "find") {
            return None;
        }
        let css = match text_arg(args, 0, "the selector") {
            Ok(c) => c,
            Err(e) => return Some(Err(e)),
        };
        let sel = match selector(css) {
            Ok(s) => s,
            Err(e) => return Some(Err(e)),
        };
        let found: Vec<ElementRef> = match root {
            Some(r) => r.select(&sel).collect(),
            None => html.select(&sel).collect(),
        };
        Some(Ok(if one {
            found.first().map_or(Value::None, |e| element(html, *e))
        } else {
            Value::list(found.iter().map(|e| element(html, *e)).collect())
        }))
    }

    impl Element {
        fn get(&self) -> ElementRef<'_> {
            self.html
                .tree
                .get(self.id)
                .and_then(ElementRef::wrap)
                .expect("an element of its own document")
        }
    }

    impl Foreign for Document {
        fn type_name(&self) -> &'static str {
            "Document"
        }

        fn display(&self) -> String {
            format!(
                "Document({} elements)",
                self.html
                    .tree
                    .nodes()
                    .filter(|n| n.value().is_element())
                    .count()
            )
        }

        fn get_attr(&self, name: &str) -> VmResult<Value> {
            match name {
                "title" => {
                    let sel = selector("title")?;
                    Ok(self
                        .html
                        .select(&sel)
                        .next()
                        .map(|t| Value::str(clean(t.text().collect())))
                        .unwrap_or(Value::None))
                }
                "text" => Ok(Value::str(clean(self.html.root_element().text().collect()))),
                "html" => Ok(Value::str(self.html.root_element().html())),
                _ => Err(err(
                    "FieldError",
                    format!("a Document has no field `{name}`"),
                )),
            }
        }

        fn call_method(
            &self,
            _: &mut Ctx,
            name: &str,
            args: &[Value],
            _: &[(String, Value)],
        ) -> VmResult<Value> {
            search(&self.html, None, name, args).unwrap_or_else(|| {
                Err(err(
                    "FieldError",
                    format!("a Document has no method `{name}`"),
                ))
            })
        }

        fn as_any(&self) -> &dyn std::any::Any {
            self
        }
    }

    impl Foreign for Element {
        fn type_name(&self) -> &'static str {
            "Element"
        }

        fn display(&self) -> String {
            let e = self.get();
            let text = clean(e.text().collect());
            let short: String = text.chars().take(40).collect();
            format!(
                "<{}> {}{}",
                e.value().name(),
                short,
                if text.chars().count() > 40 { "..." } else { "" }
            )
        }

        fn get_attr(&self, name: &str) -> VmResult<Value> {
            let e = self.get();
            Ok(match name {
                // The text inside, with runs of spaces and line breaks made
                // single spaces.
                "text" => Value::str(clean(e.text().collect())),
                // The text exactly as written.
                "raw_text" => Value::str(e.text().collect::<String>()),
                "tag" => Value::str(e.value().name().to_string()),
                "html" => Value::str(e.inner_html()),
                "outer_html" => Value::str(e.html()),
                "id" => e
                    .value()
                    .id()
                    .map(|s| Value::str(s.to_string()))
                    .unwrap_or(Value::None),
                "classes" => Value::list(
                    e.value()
                        .classes()
                        .map(|c| Value::str(c.to_string()))
                        .collect(),
                ),
                "attrs" => {
                    let mut map = super::super::value::Map::default();
                    for (k, v) in e.value().attrs() {
                        let key = Value::str(k.to_string());
                        if let Some(h) = super::super::value::Key::from_value(&key) {
                            map.insert(h, key, Value::str(v.to_string()));
                        }
                    }
                    Value::Map(Rc::new(std::cell::RefCell::new(map)))
                }
                "children" => Value::list(
                    e.children()
                        .filter_map(ElementRef::wrap)
                        .map(|c| element(&self.html, c))
                        .collect(),
                ),
                "parent" => e
                    .parent()
                    .and_then(ElementRef::wrap)
                    .map(|p| element(&self.html, p))
                    .unwrap_or(Value::None),
                _ => {
                    return Err(err(
                        "FieldError",
                        format!("an Element has no field `{name}`"),
                    ))
                }
            })
        }

        fn call_method(
            &self,
            _: &mut Ctx,
            name: &str,
            args: &[Value],
            _: &[(String, Value)],
        ) -> VmResult<Value> {
            if let Some(r) = search(&self.html, Some(self.get()), name, args) {
                return r;
            }
            match name {
                // `link.attr("href")`: the attribute, or none.
                "attr" => {
                    let a = text_arg(args, 0, "the attribute name")?;
                    Ok(self
                        .get()
                        .attr(a)
                        .map(|v| Value::str(v.to_string()))
                        .unwrap_or(Value::None))
                }
                "has_class" => {
                    let c = text_arg(args, 0, "the class")?;
                    Ok(Value::Bool(self.get().value().classes().any(|x| x == c)))
                }
                _ => Err(err(
                    "FieldError",
                    format!("an Element has no method `{name}`"),
                )),
            }
        }

        fn as_any(&self) -> &dyn std::any::Any {
            self
        }
    }

    pub fn parse(text: &str) -> Value {
        Value::Foreign(Rc::new(Box::new(Document {
            html: Rc::new(Html::parse_document(text)),
        })))
    }
}

/// `html.parse(text)`: a page to search with CSS selectors.
pub fn parse(_: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    let text = text_arg(args, 0, "the HTML")?;
    #[cfg(feature = "html")]
    {
        Ok(imp::parse(text))
    }
    #[cfg(not(feature = "html"))]
    {
        let _ = text;
        Err(err(
            "Unavailable",
            "`html.parse` needs a build with the `html` feature",
        ))
    }
}
