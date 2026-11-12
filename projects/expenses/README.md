# Expense Tracker

A command-line tool that remembers what you spend, written in [Vyrn](https://github.com/dmccottrell/vyrn).

## Run it

Install Vyrn first (see its README), then in this folder:

```powershell
vyrn run add 12.50 food "lunch with Sam"
vyrn run add 900 rent
vyrn run list
vyrn run summary
vyrn run delete 2
vyrn test
```

## Files

| File | What it does |
|---|---|
| `main.vyrn` | the commands: add, list, summary, delete, help |
| `money.vyrn` | reading and showing amounts, kept as whole cents |
| `store.vyrn` | the SQLite database, behind a small object |
| `tests/` | tests for the money functions |

## What it shows

Modules, a SQLite database, command-line arguments, money done properly in cents, error messages and exit codes. Every file is commented: read them top to bottom.
