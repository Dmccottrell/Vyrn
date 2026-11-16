# Day 81: Portfolio project: a full-stack web app

Today's brief is a complete web application with accounts and a database,
the kind of project that shows you can build something real end to end.
Day 53's todo app is the pattern; this one should be your own idea.

## 81.1 The brief

Some ideas:

- A **reading tracker**: books you're reading, pages per day, a chart of
  your year.
- A **club organiser**: events, who's coming, a reminder email the day
  before.
- A **recipe box**: recipes with ingredients, scaling servings, a shopping
  list for the week.
- A **shared expenses** app: who paid what, and who owes whom.

## 81.2 Done means

- [ ] Sign up, log in, log out, with hashed passwords and sessions
      (Day 49).
- [ ] Each user sees and changes only their own data: every query includes
      the owner (Day 53).
- [ ] At least three kinds of record in the database, with at least one
      join in a real page.
- [ ] Pages from templates with a shared layout; works on a phone.
- [ ] Forms checked on the server, with errors shown next to the form;
      CSRF tokens on every POST (Day 51).
- [ ] A JSON API for at least one resource (Day 48).
- [ ] Tests through `http.test` for the main promises, including "another
      user can't see this".
- [ ] Settings from the environment, a `/health` route, a Dockerfile, and
      a GitHub Actions workflow running the tests (Day 52).
- [ ] Deployed somewhere people can visit, with HTTPS.

## 81.3 A suggested route

1. Sketch the pages on paper, and the tables they need.
2. Data layer first, with tests, using an in-memory database.
3. Routes and templates for one resource, end to end, before the next.
4. Logins last but one; deployment last, early enough to fix surprises.

## 81.4 Warm-up

Shared expenses need "who owes whom". Write `balances(payments)`: given
`(payer, amount)` pairs shared equally by everyone listed, give each
person's balance (positive: they're owed money).

```vyrn starter
create balances(payments: list) -> map:
    return {}
end

main:
    print(balances([("ada", 30.0), ("bob", 0.0), ("cy", 15.0)]))
end
```

```text expect
{"ada": 15.0, "bob": -15.0, "cy": 0.0}
```

> Hint: everyone's share is the total divided by the number of people;
> each balance is what they paid minus the share.

<details><summary>Solution</summary>

```vyrn solution
create balances(payments: list) -> map:
    let share = sum(a each _, a in payments) / len(payments)
    return {name: paid - share each name, paid in payments}
end

main:
    print(balances([("ada", 30.0), ("bob", 0.0), ("cy", 15.0)]))
end
```

</details>

## 81.5 Going further

- Live updates with server-sent events or WebSockets.
- Uploading pictures, kept safely outside the web folder (Day 51).
- An admin page, behind a role check, with usage charts.
