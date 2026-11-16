# Learn Vyrn: the course website

The 84-day course (`docs/course`) as a website: a lesson on the left, an
editor and console on the right, like an online coding school.

- Every example has **Run** and **Edit**; the exercise starts in the
  editor, and **Check answer** runs it and compares what it prints with
  the goal, using the same rule as `vyrn learn check` (trailing spaces
  and blank lines don't count). A wrong answer shows both outputs with the
  first different line marked.
- **Hint** gives one hint at a time; **Solution** shows an answer;
  **Reset** brings back the starting code.
- Code and progress are saved in the browser (`localStorage`), so a
  visitor can close the tab and carry on later. The home page shows
  progress by unit and day, and *Continue* returns to the last lesson.
- Light and dark themes; one column on phones.

Vyrn itself runs in the page: `vyrn.wasm`, the whole toolchain compiled
to WebAssembly (`runtime/wasm`), in a Web Worker, so a loop that never
ends is stopped (after 10 seconds, or with **Stop**) without freezing the
page. Files, environment variables and SQLite databases live in memory
for one run; time comes from the browser, and `sleep` moves a virtual
clock forward at once. Lessons that open windows (`vyrn.draw`), save PNG
charts or use several cores are marked "runs on your computer" and give
the `vyrn learn` commands instead.

## Files

| File | Is |
|---|---|
| `index.html` | the page |
| `style.css` | the look |
| `app.js` | everything else: the course, a small Markdown renderer, the editor, running and checking |
| `build.mjs` | builds the site (Node 18 or later) |

## Building

```bash
cargo build --release --target wasm32-unknown-unknown --manifest-path runtime/wasm/Cargo.toml
```

```bash
vyrn learn export build/course
```

```bash
node site/build.mjs build/course/course.json runtime/wasm/target/wasm32-unknown-unknown/release/vyrn_wasm.wasm build/site
```

`vyrn learn export` writes the course as JSON. `build.mjs` then runs every
example and every exercise's solution through the WebAssembly Vyrn,
marks the ones that need a computer, and **fails** if a solution that runs
in the browser prints the wrong thing (so the site never tells a learner
a right answer is wrong). CI does this on every push.

Serve `build/site` with any static web server (the page fetches
`course.json` and `vyrn.wasm`), for example:

```bash
python3 -m http.server -d build/site
```

`--single learn-vyrn.html` also writes the whole site as one
self-contained page (about 6 MB), which works even opened from disk.

## Publishing

`.github/workflows/site.yml` builds the site and publishes it to GitHub
Pages on every push to `main` that changes the course, the site or Vyrn.
In the repository's Settings, under Pages, set the source to "GitHub
Actions" once.
