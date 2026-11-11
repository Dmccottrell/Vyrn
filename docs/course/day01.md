# Day 1: Hello, Vyrn

Today you write your first programs. No experience is needed: every word is
explained the first time it appears. By the end of the day you will have
built a name generator that asks you questions and makes up a name for your
band, your robot or your spaceship.

## 1.1 What is a program?

A **program** is a list of instructions for a computer, written in a
language both you and the computer can read. Vyrn is one such language.
The computer follows the instructions one at a time, from top to bottom.

Here is a whole Vyrn program:

```vyrn
main:
    print("Hello, world!")
end
```

```text
Hello, world!
```

Line by line:

- `main:` means "the program starts here". Every program has exactly one
  `main:`.
- `print("Hello, world!")` shows the words `Hello, world!` on the screen.
- `end` closes the `main:` block. The lines between `main:` and `end` are
  *inside* it, and are pushed to the right by four spaces so you can see
  that at a glance. That push is called **indentation**.

**How to run it.** Save the program in a file called `hello.vyrn`, then in
a terminal type:

```bash
vyrn hello.vyrn
```

With `vyrn learn`, you don't even need to make the file: each lesson's
exercise file is made for you.

### Exercise

Change the program so it prints `Hello, Vyrn!` instead.

```vyrn starter
main:
    print("Hello, world!")
end
```

```text expect
Hello, Vyrn!
```

> Hint: Only the words between the quotes need to change.

<details><summary>Solution</summary>

```vyrn solution
main:
    print("Hello, Vyrn!")
end
```

</details>

## 1.2 Printing more than one line

Each `print` shows one line. Several `print`s run one after another, top
to bottom:

```vyrn
main:
    print("Day 1")
    print("of")
    print("100")
end
```

```text
Day 1
of
100
```

You can give `print` several things at once, separated by commas. It puts
a space between them:

```vyrn
main:
    print("Vyrn", "is", "fun")
    print("Lesson", 2)
end
```

```text
Vyrn is fun
Lesson 2
```

Notice `2` has no quotes: it is a number, not text. Tomorrow is all about
the difference.

### Exercise

Print this little poem, one line per `print`:

```text
Roses are red,
Code can be too,
I wrote a program,
And so can you.
```

```vyrn starter
main:
    print("Roses are red,")
end
```

```text expect
Roses are red,
Code can be too,
I wrote a program,
And so can you.
```

> Hint: You need four `print` lines, each inside `main:`.
> Hint: Every line inside `main:` starts with four spaces.

<details><summary>Solution</summary>

```vyrn solution
main:
    print("Roses are red,")
    print("Code can be too,")
    print("I wrote a program,")
    print("And so can you.")
end
```

</details>

## 1.3 Text, quotes and comments

Text in a program is called a **string** (a string of letters). A string
goes between double quotes `"like this"` or single quotes `'like this'`.
To put a quote inside, use the other kind outside:

```vyrn
main:
    print('She said "hi"')
    print("It's sunny")
end
```

```text
She said "hi"
It's sunny
```

A line starting with `#` is a **comment**: a note for people reading the
code. Vyrn ignores it completely.

```vyrn
# This program greets the reader.
main:
    print("Welcome!")  # comments can also go at the end of a line
end
```

```text
Welcome!
```

Two special pieces inside strings: `\n` starts a new line, and `\t` jumps
to the next tab stop.

```vyrn
main:
    print("one\ntwo")
    print("name:\tAda")
end
```

```text
one
two
name:	Ada
```

### Exercise

Print exactly this, using **one** `print`:

```text
Ready
Set
Go!
```

```vyrn starter
main:
    # One print, with \n where a new line should start.
end
```

```text expect
Ready
Set
Go!
```

> Hint: `\n` inside a string starts a new line.

<details><summary>Solution</summary>

```vyrn solution
main:
    print("Ready\nSet\nGo!")
end
```

</details>

## 1.4 When something goes wrong

Everyone who writes code makes mistakes. Vyrn's job is to tell you what
went wrong, where, and how to fix it. Suppose you forget the closing quote:

```vyrn check
main:
    print("Hello")
end
```

If the line were `print("Hello)`, Vyrn would answer:

```text
error: this string is never closed
 --> hello.vyrn:2:11
  |
2 |     print("Hello)
  |           ^
  = help: add the closing quote; use `"""` for text that spans several lines
```

Read an error from the top:

1. **What** went wrong: the string is never closed.
2. **Where**: file `hello.vyrn`, line 2, column 11. The `^` points at the
   spot.
3. **How to fix it**: the `help` line.

Other common first mistakes:

| You wrote | Vyrn says |
|---|---|
| `Print("hi")` | `` `Print` is not defined `` and "did you mean `print`?" (capital letters matter) |
| `print("hi"` | this `(` is never closed |
| `main` without the `:` | expected `:` to end the `main` line |
| no `end` after the block | this `main` block is never closed |

The four spaces of indentation are for people: Vyrn knows where a block
ends from its `end`, and `vyrn fmt` puts the spaces in for you.

`vyrn check hello.vyrn` looks for mistakes without running the program.

### Exercise

This program has three mistakes. Run it, read the first error, fix that
one, and repeat until it prints the two lines below.

```vyrn starter broken
main
    Print("Bugs are normal.")
    print("Fixing them is a skill."
end
```

```text expect
Bugs are normal.
Fixing them is a skill.
```

> Hint: The first line is missing something every block header needs.
> Hint: Names are case-sensitive: is it `Print` or `print`?
> Hint: Count the `(` and `)` on the last print line.

<details><summary>Solution</summary>

```vyrn solution
main:
    print("Bugs are normal.")
    print("Fixing them is a skill.")
end
```

</details>

## 1.5 Asking questions with `input`

`input` shows a question, waits for the person to type an answer and press
Enter, and gives back what they typed:

```vyrn check
main:
    name = input("What is your name? ")
    print("Nice to meet you,", name)
end
```

If you type `Ada`, the screen shows:

```text
What is your name? Ada
Nice to meet you, Ada
```

`name = input(...)` *stores* the answer under the name `name`, so we can
use it later. A stored value with a name is called a **variable**: the next
lesson is all about them.

### Exercise

Ask "What is your favourite colour? " and answer with
`<colour> is a great colour!`. The checker will type `teal`.

```vyrn starter
main:
    colour = input("What is your favourite colour? ")
end
```

```text input
teal
```

```text expect
What is your favourite colour? teal is a great colour!
```

> Hint: `print(colour, "is a great colour!")`
> Hint: The question and the answer appear on one line because the person
> pressing Enter is not part of the program's output.

<details><summary>Solution</summary>

```vyrn solution
main:
    colour = input("What is your favourite colour? ")
    print(colour, "is a great colour!")
end
```

</details>

## 1.6 Variables

A **variable** is a name for a value. You make one with `=`:

```vyrn
main:
    city = "Lisbon"
    print(city)
    city = "Kyoto"
    print(city)
end
```

```text
Lisbon
Kyoto
```

Read `city = "Lisbon"` as "let `city` be `"Lisbon"`". Giving it a new
value replaces the old one.

Variables make programs flexible: change the value in one place and every
use follows.

```vyrn
main:
    hero = "Mira"
    print(hero, "woke up.")
    print(hero, "found a map.")
    print(hero, "set off.")
end
```

```text
Mira woke up.
Mira found a map.
Mira set off.
```

**Naming rules.** Names are made of letters, digits and `_`, and can't
start with a digit. Vyrn style is lowercase words joined by `_`:
`first_name`, `high_score`. Names are case-sensitive: `score` and `Score`
are different.

### Exercise

Swap the drinks: after your code, `a` should hold `"coffee"` and `b`
`"tea"`. Don't type the words `"tea"` or `"coffee"` again: use a third
variable.

```vyrn starter
main:
    a = "tea"
    b = "coffee"
    # Swap them here.
    print("a:", a)
    print("b:", b)
end
```

```text expect
a: coffee
b: tea
```

> Hint: Keep a copy first: `spare = a`.
> Hint: Then `a = b`, then `b = spare`.

<details><summary>Solution</summary>

```vyrn solution
main:
    a = "tea"
    b = "coffee"
    spare = a
    a = b
    b = spare
    print("a:", a)
    print("b:", b)
end
```

</details>

## 1.7 Joining text

`+` joins strings together, and `*` repeats one:

```vyrn
main:
    first = "Ada"
    last = "Lovelace"
    print(first + " " + last)
    print("=" * 12)
end
```

```text
Ada Lovelace
============
```

An easier way to build text: put a variable's name in `{braces}` inside a
string, and Vyrn fills in its value. This is called **interpolation**.

```vyrn
main:
    pet = "Biscuit"
    sound = "woof"
    print("{pet} says {sound}!")
end
```

```text
Biscuit says woof!
```

`len` counts the characters in a string:

```vyrn
main:
    word = "rhythm"
    print("{word} has {len(word)} letters")
end
```

```text
rhythm has 6 letters
```

### Exercise

Ask for a first name and a city (the checker types `Sam`, then `Oslo`) and
print `Sam from Oslo` underlined with `-` exactly as long as the line.

```vyrn starter
main:
    first = input("First name: ")
    city = input("City: ")
end
```

```text input
Sam
Oslo
```

```text expect
First name: City: Sam from Oslo
-------------
```

> Hint: Build the line first: `line = "{first} from {city}"`.
> Hint: Then `print("-" * len(line))`.

<details><summary>Solution</summary>

```vyrn solution
main:
    first = input("First name: ")
    city = input("City: ")
    line = "{first} from {city}"
    print(line)
    print("-" * len(line))
end
```

</details>

## 1.8 Project: the Name Generator

Time to put the day together. The program asks two questions and makes up
a name from the answers.

```vyrn check
main:
    print("Welcome to the Name Generator!")
    town = input("Which town did you grow up in?\n")
    pet = input("What's the name of a pet you know?\n")
    print("Your band could be called: {town} {pet}")
end
```

Plan before you type: what does the program need to *ask*, *remember*,
and *show*? Here: ask two things, remember both in variables, show one line.
That three-step plan works for most small programs.

### Exercise

Make a spaceship name generator. Ask `Pick a colour: ` and
`Pick an animal: `, then print `Launching the <Colour> <Animal>!`. The
checker types `Silver`, then `Falcon`.

```vyrn starter
main:
    print("Spaceship Name Generator")
end
```

```text input
Silver
Falcon
```

```text expect
Spaceship Name Generator
Pick a colour: Pick an animal: Launching the Silver Falcon!
```

> Hint: Two `input` lines, each stored in its own variable.
> Hint: `print("Launching the {colour} {animal}!")`

<details><summary>Solution</summary>

```vyrn solution
main:
    print("Spaceship Name Generator")
    colour = input("Pick a colour: ")
    animal = input("Pick an animal: ")
    print("Launching the {colour} {animal}!")
end
```

</details>

**Day 1 done.** You can print text, read answers, keep values in variables
and build text from them. Tomorrow: numbers, and the different kinds of
data.
