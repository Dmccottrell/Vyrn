-- Accounts for the Learn Vyrn website (Supabase).
--
-- Each learner's progress, saved code, daily activity and exercise
-- attempts, readable and writable only by that learner. Admins (emails in
-- `admin_emails`, once confirmed) can read everyone's, for the admin page.
-- Apply once to a new Supabase project (SQL editor, or `supabase db push`).

-- ---- who is an admin ---------------------------------------------------------

create table public.admin_emails (email text primary key);
alter table public.admin_emails enable row level security; -- no policies: only SQL can read it

-- Add each admin once, in the SQL editor (not kept here, so no address is
-- published with the code):
--   insert into public.admin_emails (email) values ('you@example.com');

-- True for a signed-in user whose confirmed email is an admin's. The
-- confirmation matters: nobody can claim an admin's email without its inbox.
create function public.is_admin() returns boolean
language sql stable security definer set search_path = '' as $$
  select exists (
    select 1 from auth.users u
    join public.admin_emails a on lower(a.email) = lower(u.email)
    where u.id = auth.uid() and u.email_confirmed_at is not null
  );
$$;

-- ---- learners ----------------------------------------------------------------

create table public.profiles (
  id uuid primary key references auth.users (id) on delete cascade,
  email text not null,
  name text check (char_length(name) <= 80),
  daily_goal int not null default 3 check (daily_goal between 1 and 20),
  created_at timestamptz not null default now(),
  last_seen timestamptz
);

-- A profile for every new account.
create function public.handle_new_user() returns trigger
language plpgsql security definer set search_path = '' as $$
begin
  insert into public.profiles (id, email, name)
  values (new.id, new.email, nullif(trim(new.raw_user_meta_data ->> 'name'), ''));
  return new;
end;
$$;

create trigger on_auth_user_created
  after insert on auth.users
  for each row execute function public.handle_new_user();

-- Finished lessons.
create table public.progress (
  user_id uuid not null default auth.uid() references auth.users (id) on delete cascade,
  lesson_id text not null check (char_length(lesson_id) <= 10),
  done_at timestamptz not null default now(),
  primary key (user_id, lesson_id)
);

-- The code in each lesson's editor.
create table public.saved_code (
  user_id uuid not null default auth.uid() references auth.users (id) on delete cascade,
  lesson_id text not null check (char_length(lesson_id) <= 10),
  code text not null check (char_length(code) <= 100000),
  updated_at timestamptz not null default now(),
  primary key (user_id, lesson_id)
);

-- What a learner did each day (their own calendar day): the daily goal,
-- streaks and the activity calendar come from this.
create table public.activity (
  user_id uuid not null default auth.uid() references auth.users (id) on delete cascade,
  day date not null,
  lessons int not null default 0 check (lessons >= 0),
  checks int not null default 0 check (checks >= 0),
  runs int not null default 0 check (runs >= 0),
  primary key (user_id, day)
);

-- Every "Check answer": shows the admin which exercises people get stuck on.
create table public.attempts (
  id bigint generated always as identity primary key,
  user_id uuid not null default auth.uid() references auth.users (id) on delete cascade,
  lesson_id text not null check (char_length(lesson_id) <= 10),
  passed boolean not null,
  at timestamptz not null default now()
);
create index attempts_user_id_idx on public.attempts (user_id);
create index attempts_at_idx on public.attempts (at);

-- Adds to today's counts (several devices can add at once).
create function public.bump_activity(p_day date, p_lessons int default 0, p_checks int default 0, p_runs int default 0)
returns void language sql security invoker set search_path = '' as $$
  insert into public.activity (user_id, day, lessons, checks, runs)
  values (auth.uid(), p_day, greatest(p_lessons, 0), greatest(p_checks, 0), greatest(p_runs, 0))
  on conflict (user_id, day) do update set
    lessons = public.activity.lessons + excluded.lessons,
    checks = public.activity.checks + excluded.checks,
    runs = public.activity.runs + excluded.runs;
$$;

-- ---- who may see what --------------------------------------------------------

alter table public.profiles enable row level security;
alter table public.progress enable row level security;
alter table public.saved_code enable row level security;
alter table public.activity enable row level security;
alter table public.attempts enable row level security;

create policy "own profile, or admin" on public.profiles for select to authenticated
  using ((select auth.uid()) = id or (select public.is_admin()));
create policy "update own profile" on public.profiles for update to authenticated
  using ((select auth.uid()) = id) with check ((select auth.uid()) = id);

create policy "own progress, or admin" on public.progress for select to authenticated
  using ((select auth.uid()) = user_id or (select public.is_admin()));
create policy "add own progress" on public.progress for insert to authenticated
  with check ((select auth.uid()) = user_id);
create policy "change own progress" on public.progress for update to authenticated
  using ((select auth.uid()) = user_id) with check ((select auth.uid()) = user_id);

create policy "own code" on public.saved_code for select to authenticated
  using ((select auth.uid()) = user_id);
create policy "add own code" on public.saved_code for insert to authenticated
  with check ((select auth.uid()) = user_id);
create policy "change own code" on public.saved_code for update to authenticated
  using ((select auth.uid()) = user_id) with check ((select auth.uid()) = user_id);

create policy "own activity, or admin" on public.activity for select to authenticated
  using ((select auth.uid()) = user_id or (select public.is_admin()));
create policy "add own activity" on public.activity for insert to authenticated
  with check ((select auth.uid()) = user_id);
create policy "change own activity" on public.activity for update to authenticated
  using ((select auth.uid()) = user_id) with check ((select auth.uid()) = user_id);

create policy "own attempts, or admin" on public.attempts for select to authenticated
  using ((select auth.uid()) = user_id or (select public.is_admin()));
create policy "add own attempts" on public.attempts for insert to authenticated
  with check ((select auth.uid()) = user_id);

-- Signed-out visitors get nothing; signed-in ones only what the policies allow.
revoke all on public.profiles, public.progress, public.saved_code, public.activity, public.attempts from anon;
revoke all on function public.bump_activity(date, int, int, int) from anon, public;
grant execute on function public.bump_activity(date, int, int, int) to authenticated;
revoke all on function public.is_admin() from anon, public;
grant execute on function public.is_admin() to authenticated;
revoke all on function public.handle_new_user() from anon, authenticated, public;

-- ---- questions under each lesson ---------------------------------------------

create table public.questions (
  id bigint generated always as identity primary key,
  lesson_id text not null check (char_length(lesson_id) <= 10),
  user_id uuid not null default auth.uid() references auth.users (id) on delete cascade,
  author text not null check (char_length(author) between 1 and 80),
  body text not null check (char_length(body) between 1 and 4000),
  created_at timestamptz not null default now()
);
create index questions_lesson_idx on public.questions (lesson_id, created_at);
create index questions_user_idx on public.questions (user_id);

create table public.answers (
  id bigint generated always as identity primary key,
  question_id bigint not null references public.questions (id) on delete cascade,
  user_id uuid not null default auth.uid() references auth.users (id) on delete cascade,
  author text not null check (char_length(author) between 1 and 80),
  body text not null check (char_length(body) between 1 and 4000),
  created_at timestamptz not null default now()
);
create index answers_question_idx on public.answers (question_id);
create index answers_user_idx on public.answers (user_id);

alter table public.questions enable row level security;
alter table public.answers enable row level security;

-- Everyone, signed in or not, can read the questions and answers.
create policy "anyone reads questions" on public.questions for select to anon, authenticated using (true);
create policy "anyone reads answers" on public.answers for select to anon, authenticated using (true);
-- Signed-in learners post as themselves.
create policy "post own question" on public.questions for insert to authenticated
  with check ((select auth.uid()) = user_id);
create policy "post own answer" on public.answers for insert to authenticated
  with check ((select auth.uid()) = user_id);
-- Authors remove their own; admins remove anything.
create policy "delete own question, or admin" on public.questions for delete to authenticated
  using ((select auth.uid()) = user_id or (select public.is_admin()));
create policy "delete own answer, or admin" on public.answers for delete to authenticated
  using ((select auth.uid()) = user_id or (select public.is_admin()));

grant select on public.questions, public.answers to anon;
revoke insert, update, delete on public.questions, public.answers from anon;
