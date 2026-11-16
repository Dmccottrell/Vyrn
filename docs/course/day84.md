# Day 84: Your own project

The last day of the course, and the start of everything after it. Today's
brief is the one you write: a project of your own choosing, built,
tested, packaged and deployed like a professional's. Everything from the
last 83 days is available to you; the hard part now is choosing what to
build and seeing it through.

## 84.1 Choosing

The best project is one you'd use yourself, or one for someone you know:
a tool for a club, a game for a younger sibling, a dashboard for a family
business. Real users give real feedback, and a reason to finish. Write one
paragraph answering:

- Who is it for, and what problem does it solve for them?
- What is the smallest version that would already be useful?
- How will you know it works?

Then cut the smallest version down by half. That's your first milestone.

## 84.2 Done means

- [ ] A README: what it is, a screenshot, how to install and use it.
- [ ] Organised in modules, with types on the functions others call
      (Day 60).
- [ ] Tests for its promises, run by GitHub Actions on every push, with
      `vyrn check` and `vyrn fmt --check`.
- [ ] Errors handled: clear messages, no lost data, nothing secret in the
      code.
- [ ] Packaged for its users: `vyrn build`, a container, a package
      (`vyrn publish`), or a website.
- [ ] Used by at least one person other than you, and improved from what
      they said.

## 84.3 How to keep going

- **Small steps, always working.** Commit each time something works. Never
  be more than a day away from a working version.
- **When stuck,** shrink the problem: a tiny program that shows just the
  part you don't understand. Half of all bugs are found while writing
  that tiny program.
- **Read other people's code.** The Vyrn examples (`examples/`), the
  standard library's documentation (`docs/stdlib.md`), and packages from
  `vyrn search` all show how others solved problems you'll meet.

## 84.4 A last warm-up

Plan the project as milestones. Write `plan(tasks, days)`: given tasks
with an estimate in days, give the tasks that fit into the days available,
in order, and the ones that don't (for version 2).

```vyrn starter
create plan(tasks: list, days: int) -> (list, list):
    return (tasks, [])
end

main:
    print(plan([("login", 2), ("data model", 1), ("charts", 3), ("export", 2)], 5))
end
```

```text expect
(["login", "data model"], ["charts", "export"])
```

> Hint: walk the tasks in order, adding each estimate to a running total;
> once one doesn't fit, everything after goes to version 2.

<details><summary>Solution</summary>

```vyrn solution
create plan(tasks: list, days: int) -> (list, list):
    var now = []
    var later = []
    var used = 0
    each name, estimate in tasks:
        if not later and used + estimate <= days:
            now.append(name)
            used += estimate
        else:
            later.append(name)
        end
    end
    return (now, later)
end

main:
    print(plan([("login", 2), ("data model", 1), ("charts", 3), ("export", 2)], 5))
end
```

</details>

## 84.5 Where next

You can now write programs that are typed and tested, that talk to the
web, keep data in databases, analyse it, draw it, run tasks in parallel,
and run as games, tools, servers and reports. The same ideas carry over
to every other language: Python, JavaScript, Go, Rust, C#, Swift. Learning
the next one will take weeks, not months, because you already know what
programs are made of. Pick the one your next project needs, and keep
building.
