// Learn Vyrn: the course website.
//
// Everything runs in the browser. The course (course.json, made by
// `vyrn learn export`) is the lessons; the Vyrn toolchain itself
// (vyrn.wasm) runs every program in a worker, so a loop that never ends
// can be stopped. Progress and code are kept in this browser only.
"use strict";
(() => {
  const $ = (id) => document.getElementById(id);
  const esc = (s) => String(s).replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");
  const view = $("view");
  const TIME_LIMIT_MS = 10000;

  // ==== saved progress =====================================================

  const STORE = "learn-vyrn-v1";
  // done: finished lessons; code (and codeAt, when it changed): each
  // editor; days: what was done each day; goal: lessons a day.
  const fresh = () => ({ done: {}, code: {}, codeAt: {}, last: null, hints: {}, days: {}, goal: 3 });
  let saved = fresh();
  try {
    const raw = localStorage.getItem(STORE);
    if (raw) saved = Object.assign(saved, JSON.parse(raw));
  } catch (_) { /* private window: progress lasts until the tab closes */ }
  function save() {
    try { localStorage.setItem(STORE, JSON.stringify(saved)); } catch (_) {}
  }

  // ==== theme ==============================================================

  function setTheme(t) {
    if (t) document.documentElement.dataset.theme = t;
    else delete document.documentElement.dataset.theme;
  }
  try { setTheme(localStorage.getItem("learn-vyrn-theme")); } catch (_) {}
  $("theme").addEventListener("click", () => {
    const dark = document.documentElement.dataset.theme
      ? document.documentElement.dataset.theme === "dark"
      : matchMedia("(prefers-color-scheme: dark)").matches;
    const next = dark ? "light" : "dark";
    setTheme(next);
    try { localStorage.setItem("learn-vyrn-theme", next); } catch (_) {}
  });

  // ==== highlighting =======================================================

  const KEYWORDS = new Set(("always and any as async attempt await by catch const defer each else end enum export extern " +
    "fail fails false for if impl in is kernel let loop main move mut none not object on or own par ptr pub ref region " +
    "return route routes shared skip spawn stop strict struct tensor to trait true type unsafe until use using var weak when yield").split(" "));
  const TYPES = new Set("int float bool str list map set any i8 i16 i32 i64 u8 u16 u32 u64 f16 bf16 f32 f64 q4 q8 vec2 vec3 vec4".split(" "));
  function highlight(text) {
    const token = /(#[^\n]*)|(r?"""[\s\S]*?(?:"""|$)|r?"(?:[^"\\\n]|\\.)*"?|r?'(?:[^'\\\n]|\\.)*'?)|(\b\d[\d_]*(?:\.\d[\d_]*)?(?:[eE][+-]?\d+)?\b)|([A-Za-z_]\w*)(\s*\()?/g;
    let out = "", last = 0, m;
    while ((m = token.exec(text))) {
      out += esc(text.slice(last, m.index));
      if (m[1]) out += `<span class="t-com">${esc(m[1])}</span>`;
      else if (m[2]) out += `<span class="t-str">${esc(m[2])}</span>`;
      else if (m[3]) out += `<span class="t-num">${m[3]}</span>`;
      else {
        const word = m[4];
        const declares = word === "create" && /^[ \t]+[A-Za-z_]\w*\s*[[(]/.test(text.slice(token.lastIndex - (m[5] || "").length));
        const cls = KEYWORDS.has(word) || declares ? "t-kw" : TYPES.has(word) ? "t-type" : m[5] ? "t-fn" : "";
        out += cls ? `<span class="${cls}">${word}</span>` : word;
        if (m[5]) out += esc(m[5]);
      }
      last = token.lastIndex;
    }
    return out + esc(text.slice(last));
  }

  // ==== markdown ===========================================================

  function inline(text) {
    // Code spans first, so nothing inside them is touched.
    return text.split(/(`+[^`]*?`+)/).map((part, i) => {
      if (i % 2 === 1) return `<code>${esc(part.replace(/^`+|`+$/g, ""))}</code>`;
      let s = esc(part);
      s = s.replace(/\[([^\]]+)\]\(([^)\s]+)\)/g, (_, t, u) =>
        /^(https?:|#)/.test(u) ? `<a href="${u}" target="_blank" rel="noopener">${t}</a>` : t);
      s = s.replace(/\*\*(\S(?:[^*]*?\S)?)\*\*/g, "<strong>$1</strong>");
      s = s.replace(/(^|[\s(])\*(\S(?:[^*]*?\S)?)\*(?=[\s.,;:!?)]|$)/g, "$1<em>$2</em>");
      return s;
    }).join("");
  }

  // Turns a lesson's markdown into HTML. Code blocks become placeholders
  // (`<div data-block="n">`) filled in by `mountBlocks`, so they can have
  // buttons.
  function markdown(text, blocks) {
    const lines = text.split("\n");
    let html = "", i = 0;
    const isBlockStart = (l) => /^(`{3,}|#{1,6}\s|>|\||\s*([-*]|\d+\.)\s|<\/?(details|summary))/.test(l);
    while (i < lines.length) {
      const line = lines[i];
      let m;
      if (!line.trim()) { i++; continue; }
      if ((m = line.match(/^(`{3,})\s*(.*)$/))) {
        const fence = m[1], info = m[2].trim();
        const body = [];
        i++;
        while (i < lines.length && !(lines[i].startsWith(fence) && !lines[i].slice(fence.length).trim())) body.push(lines[i++]);
        i++;
        blocks.push({ info, code: body.join("\n") + "\n" });
        html += `<div data-block="${blocks.length - 1}"></div>`;
        continue;
      }
      if ((m = line.match(/^(#{1,6})\s+(.*)$/))) {
        const level = Math.min(m[1].length + 1, 4);
        const exercise = /^exercise$/i.test(m[2].trim());
        html += `<h${level}${exercise ? ' class="exercise-head"' : ""}>${inline(m[2])}</h${level}>`;
        i++;
        continue;
      }
      if (/^<\/?(details|summary)/.test(line)) { i++; continue; }
      if (line.startsWith(">")) {
        const body = [];
        while (i < lines.length && lines[i].startsWith(">")) body.push(lines[i++].replace(/^> ?/, ""));
        if (/^Hint:/.test(body[0]) && blocks.hideHints) continue;
        html += `<blockquote>${markdown(body.join("\n"), blocks)}</blockquote>`;
        continue;
      }
      if (line.startsWith("|") && i + 1 < lines.length && /^\|[\s:|-]+\|?\s*$/.test(lines[i + 1])) {
        const cells = (l) => l.trim().replace(/^\||\|$/g, "").split(/(?<!\\)\|/).map((c) => inline(c.trim()));
        const head = cells(line);
        i += 2;
        let rows = "";
        while (i < lines.length && lines[i].startsWith("|")) rows += `<tr>${cells(lines[i++]).map((c) => `<td>${c}</td>`).join("")}</tr>`;
        html += `<table><thead><tr>${head.map((c) => `<th>${c}</th>`).join("")}</tr></thead><tbody>${rows}</tbody></table>`;
        continue;
      }
      if ((m = line.match(/^(\s*)([-*]|\d+\.)\s+/))) {
        const ordered = /\d/.test(m[2]);
        const items = [];
        while (i < lines.length) {
          const l = lines[i];
          const item = l.match(/^\s*([-*]|\d+\.)\s+(.*)$/);
          if (item) { items.push(item[2]); i++; continue; }
          if (l.trim() && /^\s+/.test(l) && items.length) { items[items.length - 1] += " " + l.trim(); i++; continue; }
          break;
        }
        const tag = ordered ? "ol" : "ul";
        html += `<${tag}>${items.map((t) => `<li>${inline(t)}</li>`).join("")}</${tag}>`;
        continue;
      }
      const para = [];
      while (i < lines.length && lines[i].trim() && !(para.length && isBlockStart(lines[i]))) para.push(lines[i++]);
      html += `<p>${inline(para.join(" "))}</p>`;
    }
    return html;
  }

  // ==== running programs ===================================================

  const WORKER = `
let instance = null;
const enc = new TextEncoder(), dec = new TextDecoder();
onmessage = async (event) => {
  const { module, source, input } = event.data;
  try {
    if (!instance) instance = await WebAssembly.instantiate(module, {});
    const x = instance.exports;
    // The module has no clock or random source of its own: give it ours.
    x.vyrn_set_time(Date.now() / 1000);
    const seed = crypto.getRandomValues(new Uint8Array(32));
    const sp = x.vyrn_alloc(32);
    new Uint8Array(x.memory.buffer, sp, 32).set(seed);
    x.vyrn_seed(sp, 32);
    x.vyrn_free(sp, 32);
    const withInput = input !== null && input !== undefined;
    const bytes = enc.encode(withInput ? source + "\\0" + input : source);
    const ptr = x.vyrn_alloc(bytes.length);
    new Uint8Array(x.memory.buffer, ptr, bytes.length).set(bytes);
    const started = performance.now();
    const reply = (withInput ? x.vyrn_run_input : x.vyrn_run)(ptr, bytes.length);
    const ms = performance.now() - started;
    x.vyrn_free(ptr, bytes.length);
    const len = new DataView(x.memory.buffer).getUint32(reply, true);
    const out = new Uint8Array(x.memory.buffer, reply + 4, len).slice();
    x.vyrn_free(reply, len + 4);
    postMessage({ ok: out[0] === 0, output: dec.decode(out.subarray(1)), ms });
  } catch (e) {
    instance = null;
    postMessage({ ok: false, output: "error: the program crashed Vyrn in the browser (" + e.message + ")\\n", ms: 0 });
  }
};`;
  const runner = {
    module: null,
    worker: null,
    pending: null,
    timer: 0,
    ready: null,
    url: URL.createObjectURL(new Blob([WORKER], { type: "text/javascript" })),
    fresh() {
      if (this.worker) this.worker.terminate();
      this.worker = new Worker(this.url);
      this.worker.onmessage = (e) => this.finish(e.data);
      this.worker.onerror = () => this.finish({ ok: false, output: "error: the program crashed Vyrn in the browser\n", ms: 0 }, true);
    },
    finish(result, broken) {
      clearTimeout(this.timer);
      const p = this.pending;
      this.pending = null;
      if (broken) this.fresh();
      if (p) p(result);
    },
    async run(source, input) {
      await this.ready;
      if (this.pending) this.stop();
      return new Promise((resolve) => {
        this.pending = resolve;
        this.timer = setTimeout(() => {
          this.finish({ ok: false, stopped: true, output: `Stopped after ${TIME_LIMIT_MS / 1000} seconds. Is there a loop that never ends?\n`, ms: 0 }, true);
        }, TIME_LIMIT_MS);
        this.worker.postMessage({ module: this.module, source, input: input ?? null });
      });
    },
    stop() {
      if (this.pending) this.finish({ ok: false, stopped: true, output: "Stopped.\n", ms: 0 }, true);
    },
  };

  function outputHtml(text) {
    return text.split("\n").map((line) => {
      const safe = esc(line);
      if (/^error/.test(line)) return `<span class="err">${safe}</span>`;
      if (/^\s*= help:/.test(line)) return `<span class="help">${safe}</span>`;
      if (/^\s*(-->|\d*\s*\|)/.test(line)) return `<span class="dim">${safe}</span>`;
      return safe;
    }).join("\n");
  }

  // The same rule as `vyrn learn check`: trailing spaces and blank lines don't count.
  const clean = (s) => {
    const l = s.split("\n").map((x) => x.replace(/\s+$/, ""));
    while (l.length && l[l.length - 1] === "") l.pop();
    return l;
  };
  const sameOutput = (a, b) => clean(a).join("\n") === clean(b).join("\n");

  // ==== the course =========================================================

  let course = null;
  const lessons = []; // every lesson, in order
  const byId = new Map();
  const dayOf = new Map();

  function unitOf(dayNumber) {
    return course.units.find((u) => dayNumber >= u.first && dayNumber <= u.last);
  }
  function dayTitle(day) { return day.title.replace(/^Day \d+:\s*/, ""); }
  const doneCount = (list) => list.filter((l) => saved.done[l.id]).length;

  function renderOverall() {
    const n = doneCount(lessons);
    const pct = lessons.length ? Math.round((100 * n) / lessons.length) : 0;
    $("overall").innerHTML = `<div class="bar"><i style="width:${pct}%"></i></div><span>${n} / ${lessons.length}</span>`;
  }

  function crumbs(parts) {
    $("crumbs").innerHTML = parts.map((p) => (p.href ? `<a href="${p.href}">${esc(p.text)}</a>` : `<span>${esc(p.text)}</span>`)).join('<span class="sep">›</span>');
  }

  function nextUnfinished(list) {
    return list.find((l) => !saved.done[l.id]) || list[0];
  }

  // ==== home ===============================================================

  const SAMPLE = `create grade(score: int) -> str:
    if score >= 90: return "A"
    else if score >= 75: return "B"
    return "C"
end

main:
    each name, score in [("Ada", 94), ("Alan", 81)]:
        print("{name}: {grade(score)}")
    end
end`;

  function home() {
    crumbs([]);
    const resume = saved.last && byId.has(saved.last) ? saved.last : null;
    const exercises = lessons.filter((l) => l.exercise).length;
    let units = "";
    for (const u of course.units) {
      const days = course.days.filter((d) => d.number >= u.first && d.number <= u.last);
      const all = days.flatMap((d) => d.lessons);
      const n = doneCount(all);
      const cards = days.map((d) => {
        const finished = doneCount(d.lessons);
        const complete = finished === d.lessons.length;
        return `<a class="day-card" href="#/day/${d.number}">
          <span class="num"><span>Day ${d.number}</span>${complete ? '<span class="done">✓ done</span>' : finished ? `<span>${finished}/${d.lessons.length}</span>` : ""}</span>
          <span class="title">${esc(dayTitle(d))}</span>
          <span class="dots">${d.lessons.map((l) => `<i class="${saved.done[l.id] ? "on" : ""}" title="${esc(l.id + " " + l.title)}"></i>`).join("")}</span>
        </a>`;
      }).join("");
      units += `<section class="unit">
        <div class="unit-head"><h2>${esc(u.title)}</h2><span class="count">Days ${u.first}–${u.last} · ${n}/${all.length} lessons</span>
        <div class="bar" aria-hidden="true"><i style="width:${all.length ? (100 * n) / all.length : 0}%"></i></div></div>
        <div class="days">${cards}</div>
      </section>`;
    }
    view.innerHTML = `<div class="home">
      <section class="hero">
        <div>
          <h1>Learn to code in <span>Vyrn</span>, one day at a time</h1>
          <p>${course.days.length} days of short lessons, from your first <code>print</code> to web apps, data and games. Read a little, run the examples, then write the code yourself and check it, right here in your browser.</p>
          <div class="actions">
            <a class="btn primary" href="#/lesson/${resume || nextUnfinished(lessons).id}">${resume ? "Continue: " + esc(resume + " " + byId.get(resume).title) : "Start Day 1"}</a>
            ${resume ? '<a class="btn" href="#/day/1">Day 1</a>' : ""}
          </div>
          <div class="stats">
            <span><b>${course.days.length}</b>days</span>
            <span><b>${lessons.length}</b>lessons</span>
            <span><b>${exercises}</b>exercises</span>
            <span><b>${course.units.length}</b>units</span>
          </div>
        </div>
        <div class="hero-card" aria-label="A small Vyrn program and what it prints">
          <pre>${highlight(SAMPLE)}</pre>
          <div class="out">Ada: A\nAlan: B</div>
        </div>
      </section>
      <div class="home-tools">${todayCard()}${searchBox()}</div>
      ${units}
    </div>`;
    wireGoal();
    wireSearch();
  }

  // ==== a lesson ===========================================================

  let current = null; // { lesson, editor, ... }

  function codeBlock(label, code, opts = {}) {
    const body = opts.vyrn ? highlight(code.replace(/\n$/, "")) : esc(code.replace(/\n$/, ""));
    return `<div class="code-block ${opts.cls || ""}">
      <div class="bar-top"><span class="label">${label}</span>${opts.badge || ""}<span class="grow"></span>${opts.buttons || ""}</div>
      <pre>${body}</pre>${opts.after || ""}</div>`;
  }

  function mountBlocks(root, blocks, lesson) {
    let vyrnIndex = 0;
    const desktop = new Set(lesson.desktop || []);
    blocks.forEach((b, n) => {
      const slot = root.querySelector(`[data-block="${n}"]`);
      if (!slot) return;
      const info = b.info;
      let html = "";
      if (info === "vyrn") {
        const index = vyrnIndex++;
        const offline = desktop.has(index);
        html = codeBlock("Example", b.code, {
          vyrn: true,
          badge: offline ? '<span class="badge" title="Uses something only Vyrn on a computer has">runs on your computer</span>' : "",
          buttons: `<button class="btn small ghost" data-act="edit">Edit</button>${offline ? "" : '<button class="btn small" data-act="run">▶ Run</button>'}`,
          after: '<div class="run-out" hidden><pre></pre></div>',
        });
      } else if (info.startsWith("vyrn starter")) {
        html = lesson.exercise ? "" : codeBlock("Start with", b.code, { vyrn: true });
      } else if (info.startsWith("vyrn")) {
        html = codeBlock("Program", b.code, {
          vyrn: true,
          badge: '<span class="badge" title="This one needs Vyrn on your computer: a window, a database, the network or a file">runs on your computer</span>',
          buttons: '<button class="btn small ghost" data-act="edit">Edit</button>',
        });
      } else if (info === "text expect") {
        html = codeBlock("Expected output", b.code, { cls: "output" });
      } else if (info === "text input") {
        html = codeBlock("Typed in by the checker", b.code, { cls: "output" });
      } else if (info === "text") {
        const prev = blocks[n - 1];
        const isOutput = prev && prev.info === "vyrn";
        html = codeBlock(isOutput ? "Output" : "Text", b.code, { cls: "output" });
      } else if (/^(bash|sh|powershell|shell)$/.test(info)) {
        html = codeBlock("Terminal", b.code, { cls: "shell" });
      } else {
        html = codeBlock(esc(info || "Code"), b.code, {});
      }
      slot.outerHTML = html;
    });
    // Wire buttons on example blocks.
    root.querySelectorAll(".code-block").forEach((el) => {
      const code = el.querySelector("pre").textContent + "\n";
      const run = el.querySelector('[data-act="run"]');
      const edit = el.querySelector('[data-act="edit"]');
      if (edit) edit.addEventListener("click", () => {
        if (current && current.lesson.exercise && !confirm("Replace the code in the editor with this example? Your exercise code is saved; Reset brings back the starting code.")) return;
        current.setCode(code, true);
        current.editor.focus();
      });
      if (run) run.addEventListener("click", async () => {
        const out = el.querySelector(".run-out");
        out.hidden = false;
        out.querySelector("pre").innerHTML = '<span class="dim">Running…</span>';
        run.disabled = true;
        const r = await runner.run(code, "");
        run.disabled = false;
        out.querySelector("pre").innerHTML = r.output ? outputHtml(r.output.replace(/\n$/, "")) : '<span class="dim">(printed nothing)</span>';
      });
    });
  }

  function lessonPage(id) {
    const lesson = byId.get(id);
    if (!lesson) { location.hash = "#/"; return; }
    const day = dayOf.get(id);
    const unit = unitOf(day.number);
    const index = lessons.indexOf(lesson);
    const prev = lessons[index - 1], next = lessons[index + 1];
    saved.last = id;
    save();
    crumbs([
      { text: unit ? unit.title.replace(/^Unit \d+:\s*/, "") : "Course", href: "#/" },
      { text: `Day ${day.number}`, href: `#/day/${day.number}` },
      { text: `${lesson.id} ${lesson.title}` },
    ]);

    const ex = lesson.exercise;
    const blocks = [];
    blocks.hideHints = !!ex;
    const first = day.lessons[0] === lesson;
    const prose = `
      <p class="kicker">Day ${day.number} · ${esc(dayTitle(day))}</p>
      <nav class="lesson-list" aria-label="Lessons today">${day.lessons.map((l) =>
        `<a href="#/lesson/${l.id}" class="${l === lesson ? "here" : ""} ${saved.done[l.id] ? "finished" : ""}" title="${esc(l.title)}">${l.id}</a>`).join("")}</nav>
      <h1>${esc(lesson.id)} ${inline(lesson.title)}</h1>
      ${ex ? '<button class="btn small to-editor" type="button" id="to-editor">The exercise is in the editor below ↓</button>' : ""}
      ${first && day.intro ? `<div class="intro">${markdown(day.intro, [])}</div>` : ""}
      ${markdown(lesson.text, blocks)}`;

    const fileName = ex ? `exercise_${lesson.id.replace(".", "_")}.vyrn` : "scratch.vyrn";
    const starter = ex ? ex.starter : firstExample(blocks) || 'main:\n    print("Hello!")\nend\n';
    const hintsShown = saved.hints[id] || 0;

    view.innerHTML = `<div class="lesson">
      <section class="prose-pane" id="prose-pane">
        <article class="prose" id="prose">${prose}</article>
        <nav class="lesson-nav" aria-label="Previous and next lesson">
          ${prev ? `<a class="btn" href="#/lesson/${prev.id}">← ${esc(prev.id)}</a>` : "<span></span>"}
          <span class="where">${saved.done[id] ? '<span class="done-mark">✓ Finished</span>' : `Lesson ${index + 1} of ${lessons.length}`}</span>
          ${next ? `<a class="btn primary" id="next" href="#/lesson/${next.id}">${ex && !saved.done[id] ? "Skip" : "Next"}: ${esc(next.id)} →</a>` : '<a class="btn primary" id="next" href="#/">Finish the course 🎉</a>'}
        </nav>
      </section>
      <section class="bench" aria-label="Code editor">
        <div class="bench-head">
          <span class="file">${fileName}</span>
          <button class="btn small ghost" id="reset" type="button" title="Go back to the starting code">Reset</button>
        </div>
        <div class="editor">
          <div class="gutter" id="gutter" aria-hidden="true">1</div>
          <div class="stack">
            <pre class="code" id="highlight" aria-hidden="true"></pre>
            <textarea id="source" spellcheck="false" autocapitalize="off" autocomplete="off" autocorrect="off" aria-label="Vyrn code"></textarea>
          </div>
        </div>
        <div class="actions-row">
          <button class="btn ${ex ? "" : "primary"}" id="run" type="button">▶ Run</button>
          ${ex ? '<button class="btn primary" id="check" type="button">Check answer</button>' : ""}
          ${ex && ex.hints.length ? `<button class="btn ghost" id="hint" type="button"></button>` : ""}
          ${ex && ex.solution ? '<button class="btn ghost" id="solution" type="button">Solution</button>' : ""}
          <button class="btn ghost" id="stop" type="button" hidden>Stop</button>
          <span class="keys"><kbd>Ctrl</kbd>+<kbd>Enter</kbd> ${ex ? "checks" : "runs"}</span>
        </div>
        <div class="console" id="console" aria-live="polite"></div>
      </section>
    </div>`;

    $("prose-pane").scrollTop = 0;
    window.scrollTo(0, 0);

    const source = $("source"), hl = $("highlight"), gutter = $("gutter"), consoleEl = $("console");
    function refresh(store = true) {
      hl.innerHTML = highlight(source.value) + "\n";
      gutter.textContent = Array.from({ length: source.value.split("\n").length }, (_, i) => i + 1).join("\n");
      sync();
      if (store) {
        saved.code[id] = source.value;
        saved.codeAt[id] = Date.now();
        save();
        account.saveCode(id, source.value);
      }
    }
    function sync() {
      hl.scrollTop = source.scrollTop; hl.scrollLeft = source.scrollLeft; gutter.scrollTop = source.scrollTop;
    }
    current = {
      lesson,
      editor: source,
      setCode(code, store) { source.value = code; refresh(store); },
    };
    source.value = saved.code[id] ?? starter;
    refresh(false);
    source.addEventListener("input", () => refresh());
    source.addEventListener("scroll", sync);
    let escaped = false;
    function insert(text) {
      const { selectionStart: a, selectionEnd: b, value } = source;
      source.value = value.slice(0, a) + text + value.slice(b);
      source.selectionStart = source.selectionEnd = a + text.length;
      refresh();
    }
    source.addEventListener("keydown", (e) => {
      if ((e.ctrlKey || e.metaKey) && e.key === "Enter") { e.preventDefault(); (ex ? check : run)(); return; }
      if (e.key === "Escape") { escaped = true; return; }
      if (e.key === "Tab" && !escaped && !e.shiftKey) { e.preventDefault(); insert("    "); return; }
      escaped = false;
      if (e.key === "Enter") {
        const before = source.value.slice(0, source.selectionStart);
        const line = before.slice(before.lastIndexOf("\n") + 1);
        let indent = line.match(/^\s*/)[0];
        if (/:\s*(#.*)?$/.test(line) && !/^\s*#/.test(line)) indent += "    ";
        e.preventDefault();
        insert("\n" + indent);
      }
    });

    mountBlocks($("prose"), blocks, lesson);
    const toEditor = $("to-editor");
    if (toEditor) toEditor.addEventListener("click", () => document.querySelector(".bench").scrollIntoView({ behavior: "smooth" }));

    // ---- the console ----
    const offline = ex && ex.browser === false;
    let hintCount = Math.min(hintsShown, ex ? ex.hints.length : 0);
    function hintsHtml() {
      return ex ? ex.hints.slice(0, hintCount).map((h, i) => `<div class="hint"><b>Hint ${ex.hints.length > 1 ? i + 1 : ""}</b> ${inline(h)}</div>`).join("") : "";
    }
    function inputBox() {
      if (ex && ex.input) return `<div class="task"><div class="label">The checker types</div><pre>${esc(ex.input.replace(/\n$/, ""))}</pre></div>`;
      if (ex) return "";
      return `<details class="input-box"><summary class="label">Input for <code>input()</code></summary><textarea id="stdin" rows="2" placeholder="One answer per line"></textarea></details>`;
    }
    function intro() {
      let s = "";
      if (offline) {
        s += `<div class="verdict note">This exercise uses ${esc(ex.needs || "something")}, which the browser doesn't have. Write it here, then run it on your computer with <code>vyrn learn ${lesson.id}</code> and <code>vyrn learn check ${lesson.id}</code>.
          ${saved.done[id] ? "" : '<button class="btn small" id="mark">Mark as done</button>'}</div>`;
      }
      if (ex) s += `<div class="task"><div class="label">Expected output</div><pre>${esc(ex.expect.replace(/\n$/, ""))}</pre></div>`;
      if (saved.done[id] && ex) s += '<div class="verdict good">✓ You solved this one.</div>';
      if (!ex) s += '<p class="placeholder">Try the examples, change them, break them. Press Run to see what your code prints.</p>';
      return s + inputBox() + hintsHtml();
    }
    function show(html) {
      consoleEl.innerHTML = html;
      const mark = $("mark");
      if (mark) mark.addEventListener("click", () => { complete(); show(intro()); });
    }
    show(intro());

    function complete() {
      if (!saved.done[id]) {
        saved.done[id] = true;
        save();
        account.markDone(id);
        track("lessons");
        renderOverall();
        const tag = document.querySelector(`.lesson-list a[href="#/lesson/${id}"]`);
        if (tag) tag.classList.add("finished");
        document.querySelector(".lesson-nav .where").innerHTML = '<span class="done-mark">✓ Finished</span>';
        const nextBtn = $("next");
        if (nextBtn && next) nextBtn.textContent = `Next: ${next.id} →`;
      }
    }

    const runBtn = $("run"), checkBtn = $("check"), stopBtn = $("stop");
    function busy(on) {
      runBtn.disabled = on;
      if (checkBtn) checkBtn.disabled = on;
      stopBtn.hidden = !on;
    }
    stopBtn.addEventListener("click", () => runner.stop());

    async function run() {
      const stdinEl = $("stdin");
      const input = ex ? ex.input || "" : stdinEl ? stdinEl.value : "";
      const keep = stdinEl ? stdinEl.value : "";
      const open = stdinEl ? stdinEl.closest("details").open : false;
      busy(true);
      track("runs");
      show('<div class="label">Output</div><pre class="dim">Running…</pre>');
      const r = await runner.run(source.value, input && !input.endsWith("\n") ? input + "\n" : input);
      busy(false);
      show(`<div class="label">Output${r.ms ? ` <span class="dim">· ${r.ms.toFixed(0)} ms</span>` : ""}</div>
        <pre>${r.output ? outputHtml(r.output.replace(/\n$/, "")) : '<span class="placeholder">The program printed nothing.</span>'}</pre>
        ${ex ? `<div class="task"><div class="label">Expected output</div><pre>${esc(ex.expect.replace(/\n$/, ""))}</pre></div>` : ""}
        ${inputBox()}${hintsHtml()}`);
      const again = $("stdin");
      if (again) { again.value = keep; again.closest("details").open = open || !!keep; }
    }

    async function check() {
      if (!ex) return run();
      busy(true);
      show('<div class="label">Checking</div><pre class="dim">Running your code…</pre>');
      const r = await runner.run(source.value, ex.input || "");
      busy(false);
      const passed = r.ok && sameOutput(r.output, ex.expect);
      track("checks");
      account.attempt(id, passed);
      if (passed) {
        complete();
        show(`<div class="verdict good">✓ Correct! ${next ? `<a class="btn small primary" href="#/lesson/${next.id}">Next: ${esc(next.id + " " + next.title)} →</a>` : ""}</div>
          <div class="label">Your output</div><pre>${outputHtml(r.output.replace(/\n$/, ""))}</pre>`);
        return;
      }
      const got = clean(r.output), want = clean(ex.expect);
      let first = got.findIndex((l, i) => l !== want[i]);
      if (first < 0) first = got.length;
      const mark = (lines, bad) => lines.map((l, i) => (i === bad ? `<span class="line-bad">${esc(l) || " "}</span>` : esc(l))).join("\n");
      let why;
      if (offline && /Unavailable/.test(r.output)) why = `This one needs ${esc(ex.needs || "Vyrn on your computer")}: check it there with <code>vyrn learn check ${lesson.id}</code>.`;
      else if (r.stopped) why = "Your program didn't finish.";
      else if (!r.ok) why = "Your program stopped with an error.";
      else why = `Not yet: line ${first + 1} is different.`;
      show(`<div class="verdict bad">✗ ${why}</div>
        ${!r.ok ? `<pre>${outputHtml(r.output.replace(/\n$/, ""))}</pre>` : `<div class="compare">
          <div><div class="label">Your output</div><pre>${got.length ? mark(got, first) : '<span class="placeholder">(nothing)</span>'}</pre></div>
          <div><div class="label">Expected</div><pre>${mark(want, first)}</pre></div></div>`}
        ${inputBox()}${hintsHtml()}`);
    }

    runBtn.addEventListener("click", run);
    if (checkBtn) checkBtn.addEventListener("click", check);
    $("reset").addEventListener("click", () => {
      if (source.value !== starter && !confirm("Go back to the starting code? Your changes will be lost.")) return;
      current.setCode(starter, true);
      show(intro());
    });
    const hintBtn = $("hint");
    function hintLabel() {
      const left = ex.hints.length - hintCount;
      hintBtn.textContent = left ? `Hint${ex.hints.length > 1 ? ` (${left} left)` : ""}` : "No more hints";
      hintBtn.disabled = !left;
    }
    if (hintBtn) {
      hintLabel();
      hintBtn.addEventListener("click", () => {
        hintCount = Math.min(hintCount + 1, ex.hints.length);
        saved.hints[id] = hintCount;
        save();
        hintLabel();
        const old = consoleEl.querySelectorAll(".hint");
        old.forEach((h) => h.remove());
        consoleEl.insertAdjacentHTML("beforeend", hintsHtml());
        consoleEl.lastElementChild.scrollIntoView({ block: "nearest", behavior: "smooth" });
      });
    }
    const solBtn = $("solution");
    if (solBtn) solBtn.addEventListener("click", () => {
      if (!confirm("Show the solution? It replaces your code in the editor (Reset brings back the starting code). Trying a hint first helps you learn more.")) return;
      current.setCode(ex.solution, true);
      show(intro() + '<div class="verdict note">This is one solution; yours can differ and still be right. Read it, then press Check answer.</div>');
    });

    // A lesson without an exercise is finished by reading it and moving on.
    const nextBtn = $("next");
    if (!ex) nextBtn.addEventListener("click", complete);
  }

  function firstExample(blocks) {
    const b = blocks.find((b) => b.info === "vyrn");
    return b ? b.code : null;
  }

  // ==== daily tracking =====================================================
  //
  // Each day (the learner's own calendar day) counts lessons finished,
  // answers checked and programs run. The daily goal, the streak and the
  // activity calendar come from these counts. Kept in the browser, and in
  // the account when signed in.

  const dayKey = (d) => `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;
  const todayKey = () => dayKey(new Date());
  const addDays = (d, n) => { const x = new Date(d); x.setDate(x.getDate() + n); return x; };
  const parseDay = (k) => { const [y, m, d] = k.split("-").map(Number); return new Date(y, m - 1, d); };
  const lessonsOn = (k) => (saved.days[k] ? saved.days[k].lessons : 0);

  function track(kind, n = 1) {
    const k = todayKey();
    const d = saved.days[k] || (saved.days[k] = { lessons: 0, checks: 0, runs: 0 });
    d[kind] = (d[kind] || 0) + n;
    save();
    account.bump(k, kind, n);
    renderStreak();
  }

  // Days in a row with at least one lesson finished. Today counts once
  // something is done; until then yesterday's streak is still alive.
  function streaks(days = saved.days) {
    const on = (k) => days[k] && days[k].lessons > 0;
    let d = new Date();
    if (!on(dayKey(d))) d = addDays(d, -1);
    let current = 0;
    while (on(dayKey(d))) { current++; d = addDays(d, -1); }
    let longest = 0, run = 0, prev = null;
    for (const k of Object.keys(days).filter(on).sort()) {
      const day = parseDay(k);
      run = prev && dayKey(addDays(prev, 1)) === k ? run + 1 : 1;
      longest = Math.max(longest, run);
      prev = day;
    }
    return { current, longest, activeDays: Object.keys(days).filter(on).length };
  }

  function renderStreak() {
    const chip = $("streak-chip");
    if (!chip) return;
    const { current } = streaks();
    const today = lessonsOn(todayKey());
    const met = today >= saved.goal;
    chip.innerHTML = `<span class="flame ${current ? "lit" : ""}" aria-hidden="true">🔥</span><b>${current}</b><span class="goal-mini ${met ? "met" : ""}">${Math.min(today, saved.goal)}/${saved.goal} today</span>`;
    chip.setAttribute("aria-label", `${current} day streak, ${today} of ${saved.goal} lessons today`);
  }

  // A calendar of the last `weeks` weeks, one square per day, darker for
  // more lessons (one hue, light to dark).
  function heatmap(days, weeks) {
    const end = new Date();
    const start = addDays(end, -(weeks * 7 - 1) - end.getDay()); // starts on a Sunday
    const level = (n) => (n <= 0 ? 0 : n === 1 ? 1 : n <= 3 ? 2 : n <= 5 ? 3 : 4);
    let cols = "", months = "";
    let lastMonth = -1;
    for (let w = 0; w < weeks + 1; w++) {
      let cells = "";
      const weekStart = addDays(start, w * 7);
      if (weekStart > end) break;
      if (weekStart.getMonth() !== lastMonth) {
        months += `<span style="grid-column:${w + 1}">${weekStart.toLocaleString(undefined, { month: "short" })}</span>`;
        lastMonth = weekStart.getMonth();
      }
      for (let i = 0; i < 7; i++) {
        const d = addDays(weekStart, i);
        if (d > end) { cells += '<i class="future"></i>'; continue; }
        const k = dayKey(d);
        const n = days[k] ? days[k].lessons : 0;
        const label = `${d.toLocaleDateString(undefined, { weekday: "short", month: "short", day: "numeric" })}: ${n} lesson${n === 1 ? "" : "s"}`;
        cells += `<i class="l${level(n)}${k === todayKey() ? " today" : ""}" title="${label}" aria-label="${label}"></i>`;
      }
      cols += `<div class="hm-col">${cells}</div>`;
    }
    return `<div class="heatmap" role="img" aria-label="Lessons finished each day, last ${weeks} weeks">
      <div class="hm-months">${months}</div><div class="hm-grid">${cols}</div>
      <div class="hm-legend"><span>Less</span><i class="l0"></i><i class="l1"></i><i class="l2"></i><i class="l3"></i><i class="l4"></i><span>More</span></div>
    </div>`;
  }

  function goalRing(done, goal) {
    const r = 26, c = 2 * Math.PI * r, f = Math.min(done / goal, 1);
    return `<svg class="ring" viewBox="0 0 64 64" aria-hidden="true">
      <circle cx="32" cy="32" r="${r}" class="ring-bg"/>
      <circle cx="32" cy="32" r="${r}" class="ring-fg ${f >= 1 ? "met" : ""}" stroke-dasharray="${c}" stroke-dashoffset="${c * (1 - f)}"/>
    </svg>`;
  }

  function todayCard(onMe = false) {
    const today = lessonsOn(todayKey());
    const s = streaks();
    const met = today >= saved.goal;
    return `<section class="today-card" aria-label="Today">
      <div class="today-goal">
        <div class="ring-wrap">${goalRing(today, saved.goal)}<span class="ring-num">${today}<small>/${saved.goal}</small></span></div>
        <div>
          <div class="label">Today's goal</div>
          <div class="today-msg">${met ? "Goal reached. Nice work! 🎉" : today ? `${saved.goal - today} more lesson${saved.goal - today === 1 ? "" : "s"} to go` : "Finish a lesson to start today"}</div>
          <label class="goal-pick">Lessons a day
            <select id="goal">${[1, 2, 3, 5, 8].map((n) => `<option ${n === saved.goal ? "selected" : ""}>${n}</option>`).join("")}</select>
          </label>
        </div>
      </div>
      <div class="today-stats">
        <span><b>🔥 ${s.current}</b>day streak</span>
        <span><b>${s.longest}</b>best streak</span>
        <span><b>${s.activeDays}</b>days learning</span>
      </div>
      ${heatmap(saved.days, 17)}
      ${onMe ? "" : '<a class="see-more" href="#/me">Your progress →</a>'}
    </section>`;
  }

  function wireGoal() {
    const g = $("goal");
    if (g) g.addEventListener("change", () => {
      saved.goal = Number(g.value);
      save();
      account.setGoal(saved.goal);
      route();
    });
  }

  // ==== your progress ======================================================

  function mePage() {
    crumbs([{ text: "Your progress" }]);
    const s = streaks();
    const done = doneCount(lessons);
    const solved = lessons.filter((l) => l.exercise && saved.done[l.id]).length;
    const exercises = lessons.filter((l) => l.exercise).length;
    const total = (k) => Object.values(saved.days).reduce((a, d) => a + (d[k] || 0), 0);
    const units = course.units.map((u) => {
      const all = course.days.filter((d) => d.number >= u.first && d.number <= u.last).flatMap((d) => d.lessons);
      const n = doneCount(all);
      return `<div class="unit-row"><span>${esc(u.title)}</span><div class="bar"><i style="width:${(100 * n) / all.length}%"></i></div><span class="num">${n}/${all.length}</span></div>`;
    }).join("");
    view.innerHTML = `<div class="home narrow">
      <h1 class="page-title">Your progress</h1>
      ${account.user ? `<p class="muted">Signed in as ${esc(account.user.email)}. Your progress is saved to your account.</p>` :
        account.ready ? '<p class="muted"><button class="btn small primary" id="me-signin">Create a free account</button> to keep your progress on every device.</p>' : ""}
      <div class="stat-grid">
        <div class="stat"><b>🔥 ${s.current}</b><span>day streak</span></div>
        <div class="stat"><b>${s.longest}</b><span>longest streak</span></div>
        <div class="stat"><b>${done}</b><span>of ${lessons.length} lessons</span></div>
        <div class="stat"><b>${solved}</b><span>of ${exercises} exercises solved</span></div>
        <div class="stat"><b>${s.activeDays}</b><span>days learning</span></div>
        <div class="stat"><b>${total("runs")}</b><span>programs run</span></div>
      </div>
      ${todayCard(true)}
      <h2 class="section-title">The last year</h2>
      <div class="card">${heatmap(saved.days, 52)}</div>
      <h2 class="section-title">Units</h2>
      <div class="card">${units}</div>
      <p><a class="btn primary" href="#/lesson/${(saved.last && byId.has(saved.last) ? byId.get(saved.last) : nextUnfinished(lessons)).id}">Continue learning →</a></p>
    </div>`;
    wireGoal();
    const b = $("me-signin");
    if (b) b.addEventListener("click", () => authDialog("signup"));
  }

  // ==== search =============================================================

  function searchBox() {
    return `<div class="search" role="search">
      <input id="search" type="search" placeholder="Search lessons: loops, SQL, files…" aria-label="Search lessons" autocomplete="off">
      <ul id="search-results" class="search-results" hidden></ul>
    </div>`;
  }

  function wireSearch() {
    const input = $("search"), list = $("search-results");
    if (!input) return;
    const index = lessons.map((l) => ({ l, hay: `${l.id} ${l.title} ${dayTitle(dayOf.get(l.id))} ${l.text}`.toLowerCase() }));
    input.addEventListener("input", () => {
      const words = input.value.toLowerCase().split(/\s+/).filter(Boolean);
      if (!words.length) { list.hidden = true; return; }
      const hits = index
        .map(({ l, hay }) => {
          if (!words.every((w) => hay.includes(w))) return null;
          const title = `${l.id} ${l.title}`.toLowerCase();
          return { l, score: words.filter((w) => title.includes(w)).length };
        })
        .filter(Boolean)
        .sort((a, b) => b.score - a.score)
        .slice(0, 8);
      list.innerHTML = hits.length
        ? hits.map(({ l }) => `<li><a href="#/lesson/${l.id}">${saved.done[l.id] ? '<span class="tick">✓</span>' : ""}<b>${esc(l.id)}</b> ${esc(l.title)}<span class="muted"> · Day ${dayOf.get(l.id).number}</span></a></li>`).join("")
        : '<li class="muted none">No lessons match.</li>';
      list.hidden = false;
    });
    input.addEventListener("keydown", (e) => {
      if (e.key === "Enter") { const a = list.querySelector("a"); if (a) location.hash = a.getAttribute("href"); }
      if (e.key === "Escape") { input.value = ""; list.hidden = true; }
    });
  }
  document.addEventListener("keydown", (e) => {
    if (e.key === "/" && !/^(INPUT|TEXTAREA|SELECT)$/.test(document.activeElement.tagName)) {
      const s = $("search");
      if (s) { e.preventDefault(); s.focus(); }
    }
  });

  // ==== accounts ===========================================================
  //
  // Optional: with a Supabase project in config.js, learners can sign up and
  // their progress, code and daily activity follow them to any device.
  // Without one, everything still works and stays in this browser.

  const SUPABASE_JS = "https://cdn.jsdelivr.net/npm/@supabase/supabase-js@2.45.4/dist/umd/supabase.min.js";
  const account = {
    client: null,
    user: null,
    admin: false,
    ready: false,
    codeTimers: {},
    async init() {
      const cfg = window.LEARN_VYRN_CONFIG || {};
      if (!cfg.supabaseUrl || !cfg.supabaseKey) { renderAccount(); return; }
      try {
        await new Promise((ok, fail) => {
          const s = document.createElement("script");
          s.src = SUPABASE_JS;
          s.onload = ok;
          s.onerror = () => fail(new Error("could not load the sign-in library"));
          document.head.appendChild(s);
        });
        this.client = window.supabase.createClient(cfg.supabaseUrl, cfg.supabaseKey);
        this.ready = true;
        this.client.auth.onAuthStateChange((event, session) => {
          if (event === "PASSWORD_RECOVERY") authDialog("newpass");
          if (event === "SIGNED_IN" && session && (!this.user || this.user.id !== session.user.id)) this.signedIn(session.user);
        });
        const { data } = await this.client.auth.getSession();
        if (data.session) await this.signedIn(data.session.user);
      } catch (e) {
        console.warn("accounts unavailable:", e.message);
      }
      renderAccount();
    },
    async signedIn(user) {
      this.user = user;
      try {
        await this.pull();
        const { data } = await this.client.rpc("is_admin");
        this.admin = !!data;
        await this.client.from("profiles").update({ last_seen: new Date().toISOString() }).eq("id", user.id);
      } catch (e) {
        console.warn("sync failed:", e.message);
      }
      renderAccount();
      route();
    },
    // Brings the account and this browser together: lessons finished on
    // either count, and each lesson keeps the newest code.
    async pull() {
      const c = this.client, uid = this.user.id;
      const [prog, code, act, prof] = await Promise.all([
        c.from("progress").select("lesson_id").eq("user_id", uid),
        c.from("saved_code").select("lesson_id, code, updated_at").eq("user_id", uid),
        c.from("activity").select("day, lessons, checks, runs").eq("user_id", uid),
        c.from("profiles").select("daily_goal, name").eq("id", uid).single(),
      ]);
      const remoteDone = new Set((prog.data || []).map((r) => r.lesson_id));
      const upload = Object.keys(saved.done).filter((id) => !remoteDone.has(id)).map((lesson_id) => ({ user_id: uid, lesson_id }));
      if (upload.length) await c.from("progress").upsert(upload);
      for (const id of remoteDone) saved.done[id] = true;

      const remoteCode = new Map((code.data || []).map((r) => [r.lesson_id, r]));
      const codeUp = [];
      for (const [id, text] of Object.entries(saved.code)) {
        const r = remoteCode.get(id);
        if (!r || (saved.codeAt[id] || 0) > Date.parse(r.updated_at)) codeUp.push({ user_id: uid, lesson_id: id, code: text, updated_at: new Date(saved.codeAt[id] || Date.now()).toISOString() });
      }
      for (const [id, r] of remoteCode) {
        if (!saved.code[id] || (saved.codeAt[id] || 0) < Date.parse(r.updated_at)) { saved.code[id] = r.code; saved.codeAt[id] = Date.parse(r.updated_at); }
      }
      if (codeUp.length) await c.from("saved_code").upsert(codeUp);

      // Activity: the larger count wins for each day; what only this
      // browser has is added to the account.
      const remoteDays = new Map((act.data || []).map((r) => [r.day, r]));
      for (const [day, d] of Object.entries(saved.days)) {
        const r = remoteDays.get(day) || { lessons: 0, checks: 0, runs: 0 };
        const diff = { p_lessons: Math.max(0, d.lessons - r.lessons), p_checks: Math.max(0, (d.checks || 0) - r.checks), p_runs: Math.max(0, (d.runs || 0) - r.runs) };
        if (diff.p_lessons || diff.p_checks || diff.p_runs) await c.rpc("bump_activity", { p_day: day, ...diff });
      }
      for (const [day, r] of remoteDays) {
        const d = saved.days[day] || (saved.days[day] = { lessons: 0, checks: 0, runs: 0 });
        d.lessons = Math.max(d.lessons, r.lessons); d.checks = Math.max(d.checks || 0, r.checks); d.runs = Math.max(d.runs || 0, r.runs);
      }
      if (prof.data) {
        if (prof.data.daily_goal) saved.goal = prof.data.daily_goal;
        this.name = prof.data.name;
      }
      save();
    },
    markDone(id) {
      if (this.user) this.client.from("progress").upsert({ user_id: this.user.id, lesson_id: id }).then(() => {});
    },
    bump(day, kind, n) {
      if (!this.user) return;
      const args = { p_day: day, p_lessons: 0, p_checks: 0, p_runs: 0 };
      args["p_" + kind] = n;
      this.client.rpc("bump_activity", args).then(() => {});
    },
    attempt(id, passed) {
      if (this.user) this.client.from("attempts").insert({ user_id: this.user.id, lesson_id: id, passed }).then(() => {});
    },
    saveCode(id, code) {
      if (!this.user) return;
      clearTimeout(this.codeTimers[id]);
      this.codeTimers[id] = setTimeout(() => {
        this.client.from("saved_code").upsert({ user_id: this.user.id, lesson_id: id, code, updated_at: new Date().toISOString() }).then(() => {});
      }, 1500);
    },
    setGoal(n) {
      if (this.user) this.client.from("profiles").update({ daily_goal: n }).eq("id", this.user.id).then(() => {});
    },
    async signOut() {
      await this.client.auth.signOut();
      this.user = null;
      this.admin = false;
      // The progress is in the account; the next person on this computer
      // starts fresh.
      saved = fresh();
      save();
      renderAccount();
      location.hash = "#/";
      route();
    },
  };

  function renderAccount() {
    const el = $("account");
    if (!el) return;
    renderStreak();
    if (!account.ready) { el.innerHTML = ""; return; }
    if (!account.user) {
      el.innerHTML = '<button class="btn small" id="signin" type="button">Log in</button><button class="btn small primary" id="signup" type="button">Sign up</button>';
      $("signin").addEventListener("click", () => authDialog("signin"));
      $("signup").addEventListener("click", () => authDialog("signup"));
      return;
    }
    const who = account.name || account.user.email;
    el.innerHTML = `<details class="menu"><summary class="avatar" title="${esc(account.user.email)}" aria-label="Your account">${esc(who.trim()[0].toUpperCase())}</summary>
      <div class="menu-list">
        <div class="menu-who">${esc(account.user.email)}${account.admin ? ' <span class="badge">admin</span>' : ""}</div>
        <a href="#/me">Your progress</a>
        ${account.admin ? '<a href="#/admin">Admin dashboard</a>' : ""}
        <button type="button" id="signout">Log out</button>
      </div></details>`;
    $("signout").addEventListener("click", () => account.signOut());
    el.querySelectorAll(".menu-list a").forEach((a) => a.addEventListener("click", () => el.querySelector("details").removeAttribute("open")));
  }

  function authDialog(mode) {
    let dlg = $("auth");
    if (!dlg) {
      dlg = document.createElement("dialog");
      dlg.id = "auth";
      document.body.appendChild(dlg);
    }
    const titles = { signin: "Log in", signup: "Create your account", reset: "Reset your password", newpass: "Choose a new password" };
    dlg.innerHTML = `<form method="dialog" class="auth-form" novalidate>
      <button class="icon-btn close" value="cancel" aria-label="Close" formnovalidate>✕</button>
      <h2>${titles[mode]}</h2>
      ${mode === "signup" ? '<p class="muted">Free. Your progress, code and streak follow you to any device.</p>' : ""}
      ${mode === "signup" ? '<label>Name <input name="name" autocomplete="name" maxlength="80"></label>' : ""}
      ${mode !== "newpass" ? '<label>Email <input name="email" type="email" autocomplete="email" required></label>' : ""}
      ${mode !== "reset" ? `<label>Password <input name="password" type="password" minlength="8" required autocomplete="${mode === "signin" ? "current-password" : "new-password"}"></label>` : ""}
      ${mode === "signup" || mode === "newpass" ? '<p class="hint-small">At least 8 characters.</p>' : ""}
      <p class="auth-msg" role="status"></p>
      <button class="btn primary wide" type="submit">${{ signin: "Log in", signup: "Sign up", reset: "Send the reset link", newpass: "Save password" }[mode]}</button>
      <div class="auth-links">
        ${mode === "signin" ? '<a href="#" data-mode="reset">Forgot your password?</a><a href="#" data-mode="signup">New here? Sign up</a>' : ""}
        ${mode === "signup" ? '<a href="#" data-mode="signin">Already have an account? Log in</a>' : ""}
        ${mode === "reset" ? '<a href="#" data-mode="signin">Back to log in</a>' : ""}
      </div>
    </form>`;
    const form = dlg.querySelector("form"), msg = dlg.querySelector(".auth-msg");
    dlg.querySelectorAll("[data-mode]").forEach((a) => a.addEventListener("click", (e) => { e.preventDefault(); authDialog(a.dataset.mode); }));
    form.addEventListener("submit", async (e) => {
      if (e.submitter && e.submitter.value === "cancel") return;
      e.preventDefault();
      const f = Object.fromEntries(new FormData(form));
      if (f.email !== undefined && !/^\S+@\S+\.\S+$/.test(f.email)) { msg.textContent = "Please enter your email address."; return; }
      if (f.password !== undefined && f.password.length < 8) { msg.textContent = "The password needs at least 8 characters."; return; }
      const btn = form.querySelector('[type="submit"]');
      btn.disabled = true;
      msg.className = "auth-msg";
      msg.textContent = "One moment…";
      const back = location.origin + location.pathname;
      const auth = account.client.auth;
      let res;
      try {
        if (mode === "signin") res = await auth.signInWithPassword({ email: f.email, password: f.password });
        if (mode === "signup") res = await auth.signUp({ email: f.email, password: f.password, options: { data: { name: f.name }, emailRedirectTo: back } });
        if (mode === "reset") res = await auth.resetPasswordForEmail(f.email, { redirectTo: back });
        if (mode === "newpass") res = await auth.updateUser({ password: f.password });
      } catch (err) {
        res = { error: err };
      }
      btn.disabled = false;
      if (res.error) { msg.className = "auth-msg bad"; msg.textContent = res.error.message; return; }
      if (mode === "signup" && !res.data.session) {
        msg.className = "auth-msg good";
        msg.textContent = `Almost there: we sent a link to ${f.email}. Open it to confirm your email, then log in.`;
        return;
      }
      if (mode === "reset") { msg.className = "auth-msg good"; msg.textContent = "If that email has an account, a reset link is on its way."; return; }
      dlg.close();
      if (mode === "newpass") alert("Your password is changed.");
    });
    if (!dlg.open) dlg.showModal();
    const firstInput = dlg.querySelector("input");
    if (firstInput) firstInput.focus();
  }

  // ==== admin ==============================================================

  async function adminPage() {
    crumbs([{ text: "Admin" }]);
    if (!account.admin) {
      view.innerHTML = `<div class="home narrow"><h1 class="page-title">Admin</h1><p class="muted">${account.user ? "This account isn't an admin." : "Log in with an admin account to see this page."}</p></div>`;
      return;
    }
    view.innerHTML = '<div class="home narrow"><h1 class="page-title">Admin</h1><p class="muted">Loading…</p></div>';
    const c = account.client;
    const since = dayKey(addDays(new Date(), -89));
    const [profiles, progress, activity, attempts] = await Promise.all([
      c.from("profiles").select("id, email, name, created_at, last_seen").order("created_at", { ascending: false }).limit(5000),
      c.from("progress").select("user_id, lesson_id").limit(100000),
      c.from("activity").select("user_id, day, lessons, checks, runs").gte("day", since).limit(100000),
      c.from("attempts").select("user_id, lesson_id, passed").gte("at", parseDay(since).toISOString()).limit(100000),
    ]);
    const failed = [profiles, progress, activity, attempts].find((r) => r.error);
    if (failed) { view.innerHTML = `<div class="home narrow"><h1 class="page-title">Admin</h1><p class="auth-msg bad">${esc(failed.error.message)}</p></div>`; return; }
    const users = profiles.data, prog = progress.data, act = activity.data, att = attempts.data;
    const today = todayKey(), weekAgo = dayKey(addDays(new Date(), -6));
    const doneBy = new Map(), daysBy = new Map();
    for (const p of prog) doneBy.set(p.user_id, (doneBy.get(p.user_id) || 0) + 1);
    for (const a of act) {
      if (!daysBy.has(a.user_id)) daysBy.set(a.user_id, {});
      daysBy.get(a.user_id)[a.day] = a;
    }
    const activeOn = (k) => new Set(act.filter((a) => a.day === k).map((a) => a.user_id)).size;
    const activeWeek = new Set(act.filter((a) => a.day >= weekAgo).map((a) => a.user_id)).size;
    const newWeek = users.filter((u) => dayKey(new Date(u.created_at)) >= weekAgo).length;

    // The last 30 days, one bar per day.
    const last30 = Array.from({ length: 30 }, (_, i) => dayKey(addDays(new Date(), i - 29)));
    function bars(title, values, unit) {
      const max = Math.max(1, ...values);
      return `<div class="card"><div class="label">${title}</div>
        <div class="bars" role="img" aria-label="${title}, last 30 days">${values.map((v, i) => {
          const label = `${parseDay(last30[i]).toLocaleDateString(undefined, { month: "short", day: "numeric" })}: ${v} ${unit}`;
          return `<div class="bar-col" title="${label}"><i style="height:${(100 * v) / max}%"></i></div>`;
        }).join("")}</div>
        <div class="bars-axis"><span>${parseDay(last30[0]).toLocaleDateString(undefined, { month: "short", day: "numeric" })}</span><span>max ${max}</span><span>today</span></div></div>`;
    }
    const signups = last30.map((k) => users.filter((u) => dayKey(new Date(u.created_at)) === k).length);
    const actives = last30.map(activeOn);
    const finished = last30.map((k) => act.filter((a) => a.day === k).reduce((s, a) => s + a.lessons, 0));

    // Exercises people get stuck on: most attempts per learner who solved it.
    const byLesson = new Map();
    for (const a of att) {
      const s = byLesson.get(a.lesson_id) || { tries: 0, passes: 0, tried: new Set(), solved: new Set() };
      s.tries++;
      s.tried.add(a.user_id);
      if (a.passed) { s.passes++; s.solved.add(a.user_id); }
      byLesson.set(a.lesson_id, s);
    }
    const hard = [...byLesson.entries()]
      .filter(([, s]) => s.tried.size >= 1 && s.tries >= 3)
      .map(([id, s]) => ({ id, ...s, rate: s.passes / s.tries, stuck: s.tried.size - s.solved.size }))
      .sort((a, b) => a.rate - b.rate)
      .slice(0, 12);

    const rows = users.map((u) => {
      const days = daysBy.get(u.id) || {};
      const lastDay = Object.keys(days).sort().pop();
      return `<tr data-q="${esc((u.email + " " + (u.name || "")).toLowerCase())}">
        <td>${esc(u.name || "")}<div class="muted small">${esc(u.email)}</div></td>
        <td>${new Date(u.created_at).toLocaleDateString()}</td>
        <td>${lastDay ? parseDay(lastDay).toLocaleDateString() : "–"}</td>
        <td class="num">${doneBy.get(u.id) || 0}</td>
        <td class="num">${streaks(days).current}</td></tr>`;
    }).join("");

    view.innerHTML = `<div class="home narrow wide-ish">
      <h1 class="page-title">Admin</h1>
      <div class="stat-grid">
        <div class="stat"><b>${users.length}</b><span>learners</span></div>
        <div class="stat"><b>${newWeek}</b><span>joined this week</span></div>
        <div class="stat"><b>${activeOn(today)}</b><span>active today</span></div>
        <div class="stat"><b>${activeWeek}</b><span>active this week</span></div>
        <div class="stat"><b>${prog.length}</b><span>lessons finished</span></div>
        <div class="stat"><b>${att.length ? Math.round((100 * att.filter((a) => a.passed).length) / att.length) : 0}%</b><span>of checks pass (90 days)</span></div>
      </div>
      <div class="chart-grid">
        ${bars("Sign-ups", signups, "sign-ups")}
        ${bars("Active learners", actives, "learners")}
        ${bars("Lessons finished", finished, "lessons")}
      </div>
      <h2 class="section-title">Exercises people find hardest <span class="muted small">(last 90 days, 3+ checks)</span></h2>
      <div class="card table-wrap">${hard.length ? `<table class="data"><thead><tr><th>Lesson</th><th class="num">Checks</th><th class="num">Pass rate</th><th class="num">Tried</th><th class="num">Still stuck</th></tr></thead><tbody>
        ${hard.map((h) => `<tr><td><a href="#/lesson/${h.id}">${esc(h.id)} ${esc(byId.has(h.id) ? byId.get(h.id).title : "")}</a></td><td class="num">${h.tries}</td><td class="num">${Math.round(h.rate * 100)}%</td><td class="num">${h.tried.size}</td><td class="num">${h.stuck}</td></tr>`).join("")}
        </tbody></table>` : '<p class="muted">Not enough checks yet.</p>'}</div>
      <h2 class="section-title">Learners</h2>
      <input id="user-filter" type="search" class="filter" placeholder="Filter by name or email" aria-label="Filter learners">
      <div class="card table-wrap"><table class="data"><thead><tr><th>Learner</th><th>Joined</th><th>Last active</th><th class="num">Lessons</th><th class="num">Streak</th></tr></thead>
        <tbody id="user-rows">${rows || '<tr><td colspan="5" class="muted">Nobody yet.</td></tr>'}</tbody></table></div>
    </div>`;
    $("user-filter").addEventListener("input", (e) => {
      const q = e.target.value.toLowerCase();
      document.querySelectorAll("#user-rows tr[data-q]").forEach((tr) => { tr.hidden = !tr.dataset.q.includes(q); });
    });
  }

  // ==== routing ============================================================

  function route() {
    runner.stop();
    const hash = location.hash.replace(/^#\/?/, "");
    let m;
    if ((m = hash.match(/^lesson\/([\d.]+)$/))) lessonPage(m[1]);
    else if (hash === "me") mePage();
    else if (hash === "admin") adminPage();
    else if ((m = hash.match(/^day\/(\d+)$/))) {
      const day = course.days.find((d) => d.number === Number(m[1]));
      if (day) { location.replace(`#/lesson/${nextUnfinished(day.lessons).id}`); return; }
      home();
    } else home();
    renderOverall();
    view.focus({ preventScroll: true });
  }

  // ==== start ==============================================================

  async function loadCourse() {
    const embedded = $("course-data").textContent.trim();
    if (embedded) return JSON.parse(embedded);
    const r = await fetch("course.json");
    if (!r.ok) throw new Error("course.json was not found next to this page");
    return r.json();
  }
  async function wasmBytes() {
    const embedded = $("vyrn-wasm").textContent.trim();
    if (embedded) return Uint8Array.from(atob(embedded), (c) => c.charCodeAt(0));
    const r = await fetch("vyrn.wasm");
    if (!r.ok) throw new Error("vyrn.wasm was not found next to this page");
    return new Uint8Array(await r.arrayBuffer());
  }

  (async () => {
    try {
      course = await loadCourse();
    } catch (e) {
      view.innerHTML = `<div class="home"><h1>The course could not load</h1><p>${esc(e.message)}</p></div>`;
      return;
    }
    for (const d of course.days) for (const l of d.lessons) { lessons.push(l); byId.set(l.id, l); dayOf.set(l.id, d); }
    runner.ready = wasmBytes()
      .then((b) => WebAssembly.compile(b))
      .then((m) => { runner.module = m; runner.fresh(); })
      .catch((e) => {
        runner.run = async () => ({ ok: false, output: `error: Vyrn could not load in this browser (${e.message})\n`, ms: 0 });
      });
    window.addEventListener("hashchange", route);
    route();
    account.init();
  })();
})();
