# Day 3: Making Decisions

Until now, every line of every program ran. Today programs start to
*choose*: do this if something is true, that otherwise. You will finish
with a text adventure where the player's choices decide the ending.

## 3.1 `if`: do something only sometimes

```vyrn
main:
    temperature = 31
    if temperature > 30:
        print("It's hot! Drink some water.")
    end
    print("Have a nice day.")
end
```

```text
It's hot! Drink some water.
Have a nice day.
```

The shape is always the same:

- `if`, a **condition** (something that is `true` or `false`), then `:`.
- The lines to run when it's true, indented.
- `end` to close the block.

If the condition is false, Vyrn skips straight to the line after `end`.
Change `31` to `20` and only `Have a nice day.` is printed.

When there is just one thing to do, it can go on the same line, with no
`end`:

```vyrn
main:
    lives = 0
    if lives == 0: print("Game over")
end
```

```text
Game over
```

### Exercise

The checker types a number of points. Print `Level up!` if it is 100 or
more, then always print `Points: <n>`.

```vyrn starter
main:
    points = int(input("Points: "))
end
```

```text input
120
```

```text expect
Points: Level up!
Points: 120
```

> Hint: "100 or more" is `points >= 100`.

<details><summary>Solution</summary>

```vyrn solution
main:
    points = int(input("Points: "))
    if points >= 100: print("Level up!")
    print("Points: {points}")
end
```

</details>

## 3.2 `else`: one thing or the other

`else` runs when the condition is false. The `if` and its `else` share one
`end`:

```vyrn
main:
    age = 15
    if age >= 18:
        print("You can vote.")
    else:
        print("You can vote in {18 - age} years.")
    end
end
```

```text
You can vote in 3 years.
```

### Exercise

A number is even when dividing by 2 leaves no remainder. The checker types
`7`: print `7 is odd` (or `is even` for even numbers).

```vyrn starter
main:
    n = int(input("Number: "))
end
```

```text input
7
```

```text expect
Number: 7 is odd
```

> Hint: `n % 2 == 0` is true for even numbers.

<details><summary>Solution</summary>

```vyrn solution
main:
    n = int(input("Number: "))
    if n % 2 == 0:
        print("{n} is even")
    else:
        print("{n} is odd")
    end
end
```

</details>

## 3.3 `else if`: more than two choices

```vyrn
main:
    score = 83
    if score >= 90:
        print("Grade A")
    else if score >= 80:
        print("Grade B")
    else if score >= 70:
        print("Grade C")
    else:
        print("Keep practising")
    end
end
```

```text
Grade B
```

Vyrn checks the conditions from the top and runs **only the first** that is
true. `83 >= 80` is true, so it prints `Grade B` and skips the rest, even
though `83 >= 70` is true too. Order matters: test the strictest condition
first.

### Exercise

Print the right greeting for an hour of the day (0 to 23): before 12
`Good morning`, before 18 `Good afternoon`, otherwise `Good evening`. The
checker types `14`.

```vyrn starter
main:
    hour = int(input("Hour: "))
end
```

```text input
14
```

```text expect
Hour: Good afternoon
```

> Hint: Three choices: `if`, `else if`, `else`.

<details><summary>Solution</summary>

```vyrn solution
main:
    hour = int(input("Hour: "))
    if hour < 12:
        print("Good morning")
    else if hour < 18:
        print("Good afternoon")
    else:
        print("Good evening")
    end
end
```

</details>

## 3.4 Comparing values

| Operator | Means |
|---|---|
| `==` | equal |
| `!=` | not equal |
| `<`, `<=` | less than, less than or equal |
| `>`, `>=` | greater than, greater than or equal |

Strings compare too: `==` checks they are exactly the same (capitals
count), and `<` compares in dictionary order. Comparisons can be chained
the way maths writes them:

```vyrn
main:
    name = "Ada"
    print(name == "Ada", name == "ada", "apple" < "banana")
    x = 7
    print(1 <= x <= 10)
    print(name.lower() == "ada")
end
```

```text
true false true
true
true
```

`name.lower()` gives the name in lowercase, so the comparison ignores
capitals. Day 8 is all about these string tools.

### Exercise

The checker types a password. Print `Strong enough` if it is at least 8
characters long, otherwise `Too short (<n> characters)`.

```vyrn starter
main:
    password = input("Password: ")
end
```

```text input
hunter2
```

```text expect
Password: Too short (7 characters)
```

> Hint: `len(password)` counts its characters.

<details><summary>Solution</summary>

```vyrn solution
main:
    password = input("Password: ")
    if len(password) >= 8:
        print("Strong enough")
    else:
        print("Too short ({len(password)} characters)")
    end
end
```

</details>

## 3.5 `and`, `or` and `not`

Combine conditions with plain words:

- `a and b`: true only when **both** are true.
- `a or b`: true when **at least one** is true.
- `not a`: flips true and false.

```vyrn
main:
    age = 25
    has_ticket = true
    if age >= 18 and has_ticket:
        print("Welcome to the concert")
    end
    day = "Sunday"
    if day == "Saturday" or day == "Sunday":
        print("It's the weekend")
    end
    raining = false
    if not raining: print("No umbrella needed")
end
```

```text
Welcome to the concert
It's the weekend
No umbrella needed
```

When mixing them, `not` goes first, then `and`, then `or`. Use parentheses
to make your meaning clear: `(a or b) and c`.

### Exercise

A year is a **leap year** if it is divisible by 4, except years divisible
by 100, unless they are also divisible by 400. (So 2000 was a leap year,
but 1900 wasn't.) The checker types `1900`.

```vyrn starter
main:
    year = int(input("Year: "))
end
```

```text input
1900
```

```text expect
Year: 1900 is not a leap year
```

> Hint: "divisible by 4" is `year % 4 == 0`.
> Hint: The rule is `(divisible by 4 and not by 100) or divisible by 400`.

<details><summary>Solution</summary>

```vyrn solution
main:
    year = int(input("Year: "))
    leap = (year % 4 == 0 and year % 100 != 0) or year % 400 == 0
    if leap:
        print("{year} is a leap year")
    else:
        print("{year} is not a leap year")
    end
end
```

</details>

## 3.6 Decisions inside decisions

An `if` can go inside another. Each one has its own `end`:

```vyrn
main:
    height = 130
    age = 10
    if height >= 120:
        print("You can ride.")
        if age < 12:
            print("Child ticket: 5")
        else:
            print("Adult ticket: 12")
        end
    else:
        print("Sorry, you need to grow a bit.")
    end
end
```

```text
You can ride.
Child ticket: 5
```

Indentation shows which `end` closes which block. `vyrn fmt` lines it all
up for you.

### Exercise

Extend the ride: the price is 5 for under-12s, 7 for 12 to 17, 12 for
adults, and a photo adds 3. The checker types height `150`, age `14`,
photo `y`. Print `Please pay 10.`

```vyrn starter
main:
    height = int(input("Height: "))
    age = int(input("Age: "))
    photo = input("Photo (y/n)? ")
end
```

```text input
150
14
y
```

```text expect
Height: Age: Photo (y/n)? Please pay 10.
```

> Hint: Start with `price = 0` before the `if`s, so it exists afterwards.
> Hint: `if photo == "y": price += 3`

<details><summary>Solution</summary>

```vyrn solution
main:
    height = int(input("Height: "))
    age = int(input("Age: "))
    photo = input("Photo (y/n)? ")
    if height >= 120:
        price = 0
        if age < 12:
            price = 5
        else if age < 18:
            price = 7
        else:
            price = 12
        end
        if photo == "y": price += 3
        print("Please pay {price}.")
    else:
        print("Sorry, you need to grow a bit.")
    end
end
```

</details>

## 3.7 `when`: picking from many values

When you compare one value against many possibilities, `when` reads better
than a long chain of `else if`. Each `is` line is one case; `else` catches
the rest:

```vyrn
main:
    day = 6
    when day:
        is 1: print("Monday")
        is 6 or 7: print("Weekend!")
        is 2 to 5: print("A weekday")
        else: print("Not a day")
    end
    command = "go north"
    when command:
        is "go north", "go south":
            print("You walk.")
        is "quit":
            print("Bye!")
        else:
            print("I don't understand.")
    end
end
```

```text
Weekend!
You walk.
```

- `is 6 or 7` matches either value; so does `is "go north", "go south"`.
- `is 2 to 5` matches a range.
- Only the first matching case runs, like `else if`.

### Exercise

Turn a traffic light colour into an instruction: `red` gives `Stop`,
`amber` or `yellow` gives `Get ready`, `green` gives `Go`, anything else
gives `Broken light?`. The checker types `amber`.

```vyrn starter
main:
    light = input("Light: ")
end
```

```text input
amber
```

```text expect
Light: Get ready
```

> Hint: `is "amber" or "yellow": print("Get ready")`

<details><summary>Solution</summary>

```vyrn solution
main:
    light = input("Light: ")
    when light:
        is "red": print("Stop")
        is "amber" or "yellow": print("Get ready")
        is "green": print("Go")
        else: print("Broken light?")
    end
end
```

</details>

## 3.8 Project: Lost in the Caves

A tiny text adventure. The player wakes in a cave and makes three choices;
each wrong choice ends the game.

Plan it as a tree first:

```text
left or right?
├── right: you fall into an underground river. Game over.
└── left: swim or wait?
    ├── swim: the current is too strong. Game over.
    └── wait: which door: red, blue or gold?
        ├── gold: you find the way out. You win!
        └── anything else: Game over.
```

### Exercise

Write the adventure. Make the answers case-insensitive with `.lower()`.
The checker types `Left`, `wait`, `GOLD`.

```vyrn starter
main:
    print("You wake up in a dark cave.")
    first = input("Go left or right? ").lower()
end
```

```text input
Left
wait
GOLD
```

```text expect
You wake up in a dark cave.
Go left or right? You find a lake. Swim or wait? A boat arrives. Which door: red, blue or gold? Sunlight! You found the way out. You win!
```

> Hint: Each choice is an `if` inside the previous one's true branch.
> Hint: `input(...).lower()` turns `GOLD` into `gold`.

<details><summary>Solution</summary>

```vyrn solution
main:
    print("You wake up in a dark cave.")
    first = input("Go left or right? ").lower()
    if first == "left":
        second = input("You find a lake. Swim or wait? ").lower()
        if second == "wait":
            door = input("A boat arrives. Which door: red, blue or gold? ").lower()
            if door == "gold":
                print("Sunlight! You found the way out. You win!")
            else:
                print("The room is full of bats. Game over.")
            end
        else:
            print("The current is too strong. Game over.")
        end
    else:
        print("You fall into an underground river. Game over.")
    end
end
```

</details>

**Day 3 done.** Your programs can now choose. Tomorrow: chance, and keeping
many values together in lists.
