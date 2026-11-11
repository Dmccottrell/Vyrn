# Day 4: Chance and Lists

Games need luck, and most programs need to keep many values together.
Today you learn both: the random module, and **lists**, Vyrn's way of
keeping values in order. The day ends with Rock, Paper, Scissors against
the computer.

## 4.1 Modules: borrowing ready-made tools

Vyrn comes with a library of ready-made tools grouped into **modules**:
`vyrn.random` for chance, `vyrn.math` for maths, `vyrn.time` for dates, and
many more. `use` brings a module into your program; its tools are then
written `module.tool`:

```vyrn
use vyrn.math

main:
    print(math.sqrt(81), math.pi)
    print(math.floor(3.7), math.ceil(3.2))
end
```

```text
9.0 3.141592653589793
3 4
```

`use` lines go at the very top of the file, before `main:`.

### Exercise

A circle's area is pi times the radius squared. Print the area of a circle
of radius 3, with two decimals.

```vyrn starter
use vyrn.math

main:
    radius = 3
end
```

```text expect
Area: 28.27
```

> Hint: `math.pi * radius ** 2`

<details><summary>Solution</summary>

```vyrn solution
use vyrn.math

main:
    radius = 3
    area = math.pi * radius ** 2
    print("Area: {area:.2f}")
end
```

</details>

## 4.2 Random numbers

```vyrn
use vyrn.random

main:
    random.seed(42)
    print(random.int(1, 6))
    print(random.int(1, 6))
    print(random.float(0, 1))
end
```

```text
4
1
0.7570464904748936
```

- `random.int(1, 6)` gives a whole number from 1 to 6, both included, like
  a die.
- `random.float(0, 1)` gives a decimal number between 0 and 1.
- `random.seed(42)` makes the "random" numbers the same on every run.
  Without it, every run is different, which is what a game wants. With
  it, a program can be checked, which is what a lesson wants: you'll see
  `random.seed` in the exercises for that reason.

### Exercise

Flip a coin: use `random.int(0, 1)` and print `Heads` for 1 and `Tails`
for 0. Keep the seed so the checker gets the same flip.

```vyrn starter
use vyrn.random

main:
    random.seed(7)
end
```

```text expect
Heads
```

> Hint: `flip = random.int(0, 1)`, then an `if`.

<details><summary>Solution</summary>

```vyrn solution
use vyrn.random

main:
    random.seed(7)
    flip = random.int(0, 1)
    if flip == 1:
        print("Heads")
    else:
        print("Tails")
    end
end
```

</details>

## 4.3 Lists

A **list** holds values in order, between square brackets:

```vyrn
main:
    planets = ["Mercury", "Venus", "Earth", "Mars"]
    print(planets)
    print(len(planets))
    print(planets[0])
    print(planets[2])
    print(planets[-1])
end
```

```text
["Mercury", "Venus", "Earth", "Mars"]
4
Mercury
Earth
Mars
```

Each item has a position, its **index**, counted **from 0**: `planets[0]`
is the first item, `planets[1]` the second. Negative indexes count from the
end: `planets[-1]` is the last.

Asking for a position that doesn't exist is an error that says exactly
what went wrong:

```vyrn
main:
    planets = ["Mercury", "Venus"]
    print(planets[5])
end
```

```text
error: IndexError: index 5 is out of range for a list of length 2
```

### Exercise

Print the second and second-to-last items of this list, on one line.

```vyrn starter
main:
    colours = ["red", "orange", "yellow", "green", "blue", "violet"]
end
```

```text expect
orange blue
```

> Hint: The second item is at index 1.
> Hint: The second-to-last is at index -2.

<details><summary>Solution</summary>

```vyrn solution
main:
    colours = ["red", "orange", "yellow", "green", "blue", "violet"]
    print(colours[1], colours[-2])
end
```

</details>

## 4.4 Changing a list

Lists can grow, shrink and change:

```vyrn
main:
    queue = ["Ana", "Ben"]
    queue.append("Cai")
    queue.insert(0, "Dee")
    print(queue)
    queue[1] = "Ari"
    print(queue)
    first = queue.pop(0)
    last = queue.pop()
    print(first, last, queue)
    queue.remove("Ben")
    print(queue, "Ari" in queue, "Ben" in queue)
end
```

```text
["Dee", "Ana", "Ben", "Cai"]
["Dee", "Ari", "Ben", "Cai"]
Dee Cai ["Ari", "Ben"]
["Ari"] true false
```

| Tool | Does |
|---|---|
| `xs.append(x)` | adds `x` at the end |
| `xs.insert(i, x)` | puts `x` at position `i` |
| `xs[i] = x` | replaces the item at `i` |
| `xs.pop()`, `xs.pop(i)` | takes out the last item (or the one at `i`) and gives it back |
| `xs.remove(x)` | takes out the first item equal to `x` |
| `x in xs` | `true` if `x` is in the list |
| `xs.index(x)` | the position of `x` |
| `xs.sort()`, `sorted(xs)` | sorts the list in place, or gives a sorted copy |

Tools written after a dot, like `queue.append`, are called **methods**:
they belong to the value before the dot.

### Exercise

Start with `["milk", "eggs"]`. Add `bread` at the end, put `apples` first,
remove `eggs`, then print the list sorted.

```vyrn starter
main:
    shopping = ["milk", "eggs"]
end
```

```text expect
["apples", "bread", "milk"]
```

> Hint: `append`, then `insert(0, ...)`, then `remove`.
> Hint: `print(sorted(shopping))`

<details><summary>Solution</summary>

```vyrn solution
main:
    shopping = ["milk", "eggs"]
    shopping.append("bread")
    shopping.insert(0, "apples")
    shopping.remove("eggs")
    print(sorted(shopping))
end
```

</details>

## 4.5 Slices: parts of a list

`xs[start:stop]` gives the items from `start` up to, **not including**,
`stop`. Leave a side out to go to the edge:

```vyrn
main:
    letters = ["a", "b", "c", "d", "e", "f"]
    print(letters[1:4])
    print(letters[:2])
    print(letters[3:])
    print(letters[-2:])
    print(letters[::2])
    print(letters[::-1])
end
```

```text
["b", "c", "d"]
["a", "b"]
["d", "e", "f"]
["e", "f"]
["a", "c", "e"]
["f", "e", "d", "c", "b", "a"]
```

The third number is the **step**: `::2` takes every second item, and
`::-1` walks backwards, reversing the list. Slices work on strings too:
`"Vyrn"[1:3]` is `"yr"`.

### Exercise

From the race results, print the top 3, then everyone else.

```vyrn starter
main:
    results = ["Kai", "Lea", "Max", "Noa", "Oli", "Pia"]
end
```

```text expect
Podium: ["Kai", "Lea", "Max"]
Others: ["Noa", "Oli", "Pia"]
```

> Hint: `results[:3]` and `results[3:]`.

<details><summary>Solution</summary>

```vyrn solution
main:
    results = ["Kai", "Lea", "Max", "Noa", "Oli", "Pia"]
    print("Podium:", results[:3])
    print("Others:", results[3:])
end
```

</details>

## 4.6 Lists inside lists: grids

A list can hold other lists. A list of rows makes a **grid** (C++ and Java
call it a 2D array): `grid[row][column]`.

```vyrn
main:
    board = [
        ["X", "O", "X"],
        [".", "X", "O"],
        ["O", ".", "X"],
    ]
    print(board[0])
    print(board[1][2])
    board[1][0] = "O"
    print(board[1])
    print(len(board), "rows of", len(board[0]))
end
```

```text
["X", "O", "X"]
O
["O", "X", "O"]
3 rows of 3
```

`board[1][2]` reads as: row 1, then column 2 of that row.

### Exercise

Put a treasure `T` at row 2, column 1 of the map, then print each row.

```vyrn starter
main:
    row0 = ["~", "~", "~"]
    row1 = ["~", "~", "~"]
    row2 = ["~", "~", "~"]
    map = [row0, row1, row2]
end
```

```text expect
["~", "~", "~"]
["~", "~", "~"]
["~", "T", "~"]
```

> Hint: `map[2][1] = "T"`

<details><summary>Solution</summary>

```vyrn solution
main:
    row0 = ["~", "~", "~"]
    row1 = ["~", "~", "~"]
    row2 = ["~", "~", "~"]
    map = [row0, row1, row2]
    map[2][1] = "T"
    print(map[0])
    print(map[1])
    print(map[2])
end
```

</details>

## 4.7 Random choices from a list

```vyrn
use vyrn.random

main:
    random.seed(4)
    friends = ["Ana", "Ben", "Cai", "Dee"]
    print(random.choice(friends), "pays for lunch")
    random.shuffle(friends)
    print("Speaking order:", friends)
    print("Team:", random.sample(friends, 2))
end
```

```text
Cai pays for lunch
Speaking order: ["Dee", "Ben", "Ana", "Cai"]
Team: ["Dee", "Cai"]
```

- `random.choice(xs)` picks one item.
- `random.shuffle(xs)` mixes the list up, in place.
- `random.sample(xs, k)` picks `k` different items.

### Exercise

Pick the winner of a raffle with `random.choice`, keeping the seed.

```vyrn starter
use vyrn.random

main:
    random.seed(11)
    tickets = ["Ivy", "Jon", "Kim", "Lou", "Mo"]
end
```

```text expect
The winner is Lou
```

> Hint: `print("The winner is", random.choice(tickets))`

<details><summary>Solution</summary>

```vyrn solution
use vyrn.random

main:
    random.seed(11)
    tickets = ["Ivy", "Jon", "Kim", "Lou", "Mo"]
    print("The winner is", random.choice(tickets))
end
```

</details>

## 4.8 Project: Rock, Paper, Scissors

The player types 0 for rock, 1 for paper or 2 for scissors; the computer
picks at random; the program says who wins.

A neat trick: put the hands in a list, so the number *is* the index:
`hands[0]` is `"rock"`. And the winner can be worked out with `%`: the
player wins when `(player - computer) % 3 == 1`.

### Exercise

The checker types `0` (rock). Keep the seed.

```vyrn starter
use vyrn.random

main:
    random.seed(5)
    hands = ["rock", "paper", "scissors"]
    player = int(input("0 rock, 1 paper, 2 scissors: "))
end
```

```text input
0
```

```text expect
0 rock, 1 paper, 2 scissors: You chose rock, the computer chose scissors.
You win!
```

> Hint: `computer = random.int(0, 2)`, then print both hands from the list.
> Hint: Same number: draw. `(player - computer) % 3 == 1`: you win. Otherwise you lose.

<details><summary>Solution</summary>

```vyrn solution
use vyrn.random

main:
    random.seed(5)
    hands = ["rock", "paper", "scissors"]
    player = int(input("0 rock, 1 paper, 2 scissors: "))
    computer = random.int(0, 2)
    print("You chose {hands[player]}, the computer chose {hands[computer]}.")
    if player == computer:
        print("It's a draw.")
    else if (player - computer) % 3 == 1:
        print("You win!")
    else:
        print("You lose.")
    end
end
```

</details>

**Day 4 done.** You can use modules, roll dice, and keep values in lists and
grids. Tomorrow: loops, to do things again and again.
