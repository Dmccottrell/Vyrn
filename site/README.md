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
- **Daily tracking:** a daily goal (lessons a day) with a progress ring,
  a streak (days in a row with a lesson finished), and an activity
  calendar. The 🔥 chip at the top shows both on every page; *Your
  progress* (`#/me`) has lessons, exercises, best streak, a year of
  activity, and progress by unit.
- **Review:** a solved exercise comes back to be solved again after 1,
  3, 7, 16, 35 and 80 days while it keeps being right (`#/review`).
- **Certificates** for each finished unit and the whole course, to print
  or save as PDF.
- **Playground and sharing:** `#/play` is a free editor; *Share* gives a
  link with the program inside it (nothing is stored on a server).
- **Pictures and files:** canvases a program draws, and files it writes
  (PNG charts, CSV, JSON), appear under its output.
- **Questions** under every lesson, answered by other learners and the
  admin.
- **AI tutor:** after an error or a wrong answer, *Explain this* says what
  went wrong without giving the answer away (needs an API key, below).
- **Docs and packages:** the reference documentation (`#/docs`) and the
  package registry (`#/packages`).
- **Search:** type a topic ("loops", "SQL join") to jump to a lesson;
  `/` focuses the search box.
- **Accounts** (optional): sign up, log in, reset a password. Signed in,
  progress, code, streaks and the daily goal are saved to the account and
  follow the learner to any device; what was done before signing up is
  kept. Without accounts configured, everything stays in the browser.
- **Admin dashboard** (`#/admin`, admins only): learners, sign-ups and
  active learners over 30 days, lessons finished, the exercises people get
  stuck on most, and every learner with their last activity and streak.
- Light and dark themes; one column on phones.

Vyrn itself runs in the page: `vyrn.wasm`, the whole toolchain compiled
to WebAssembly (`runtime/wasm`), in a Web Worker, so a loop that never
ends is stopped (after 10 seconds, or with **Stop**) without freezing the
page. Files, environment variables and SQLite databases live in memory
for one run; time comes from the browser, and `sleep` moves a virtual
clock forward at once. Lessons that open windows (`vyrn.draw`), save PNG
charts or use several cores are marked "runs on your computer" and give
the `vyrn learn` commands instead.

## Accounts

Accounts live in a Supabase project. `site/supabase/schema.sql` creates
its tables and rules: each learner can only read and write their own
rows; admins (the emails in `admin_emails`, once confirmed) can read
everyone's progress and activity for the dashboard, but not their code.
`config.js` holds the project's URL and publishable key, which are meant
to be public. Admins are added in the SQL editor (see the schema).

In the Supabase dashboard, under Authentication → URL Configuration, the
Site URL must be the website's address, so the links in confirmation and
password emails come back to it. Supabase's built-in email service only
sends to the project's own team; for other learners to get confirmation
emails, set up an SMTP provider under Authentication → Emails.

## Files

| File | Is |
|---|---|
| `index.html` | the page |
| `style.css` | the look |
| `app.js` | everything else: the course, a small Markdown renderer, the editor, running and checking, daily tracking, accounts, the admin dashboard |
| `config.js` | where accounts are kept (Supabase URL and publishable key) |
| `supabase/schema.sql` | the accounts database |
| `api/explain.js` | the AI tutor (Vercel function) |
| `api/remind.js` | streak reminder emails (Vercel Cron) |
| `public/` | the built site, as Vercel serves it (made by `build.mjs`) |
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

The site is on Vercel (`learn-vyrn` project), serving `site/public`, a
built copy committed to the repository. `.github/workflows/site.yml`
rebuilds it on every push to `main` that changes the course, the site or
Vyrn, and commits the result; Vercel publishes each commit.

One-time setup, in the Vercel dashboard for `learn-vyrn`:

1. **Settings → Git**: connect `Dmccottrell/Vyrn`; production branch
   `main`.
2. **Settings → Build and Deployment**: root directory `site/public`,
   framework "Other", no build command.

### The AI tutor ("Explain this")

`api/explain.js` asks Claude why a learner's program failed. It needs an
Anthropic API key: create one at console.anthropic.com, then in Vercel,
**Settings → Environment Variables**, add `ANTHROPIC_API_KEY`. Optional:
`AI_DAILY_LIMIT` (explanations per learner per day, default 20). Without
the key the button says the tutor isn't set up.

### Emails: sign-up confirmations and streak reminders

Supabase's built-in email only reaches the project's own team. For
everyone's sign-up and password emails, add an SMTP provider in Supabase
(**Authentication → Emails → SMTP settings**); Resend works well. In
**Authentication → URL Configuration**, set the Site URL to the website's
address.

Streak reminders (`api/remind.js`, run each evening by Vercel Cron) send
through Resend too. In Vercel's environment variables add:

| Name | Value |
|---|---|
| `RESEND_API_KEY` | from resend.com |
| `REMINDER_FROM` | the sender, such as `Learn Vyrn <hello@your-domain.com>` (a domain verified in Resend) |
| `SUPABASE_SERVICE_ROLE_KEY` | Supabase: **Project Settings → API keys** (secret; never in the code) |
| `CRON_SECRET` | any long random text |

Learners turn reminders on under *Your progress*.

### A custom domain

Buy a domain (any registrar, or Vercel's **Domains** page), then in the
project's **Settings → Domains** add it and set the DNS records Vercel
shows. Then use the new address as Supabase's Site URL, and as
`SITE_URL` in Vercel's environment variables (for the reminder emails'
links).
