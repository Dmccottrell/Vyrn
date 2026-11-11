# Unit 2: Games, Errors and Objects (Days 11–20)

This unit builds games of growing size, and with them the tools that keep
bigger programs tidy: scope, error handling, objects and structs. It ends
with pictures, a window and the first half of a Snake game.

---

## Day 11: Project day: Pig, a dice game

**Goal:** a complete game from functions, lists and loops.

In Pig, each turn you roll a die as often as you dare and add up the
rolls, but a 1 wipes out the turn. First to 30 wins. Each player here has
a *strategy*: stop rolling once the turn reaches some total.

```vyrn
use vyrn.random

create play_turn(name, stop_at):
    turn = 0
    rolls = []
    loop turn < stop_at:
        roll = random.int(1, 6)
        rolls.append(roll)
        if roll == 1:
            print("  {name} rolls {rolls}: a 1! Nothing this turn.")
            return 0
        end
        turn += roll
    end
    print("  {name} rolls {rolls} and banks {turn}")
    return turn
end

main:
    random.seed(7)
    scores = {"Cautious": 0, "Bold": 0}
    strategies = {"Cautious": 8, "Bold": 20}
    round = 1
    winner = ""
    loop winner == "":
        print("Round {round}")
        each name in ["Cautious", "Bold"]:
            scores[name] += play_turn(name, strategies[name])
            if scores[name] >= 30:
                winner = name
                stop
            end
        end
        round += 1
    end
    print("{winner} wins with {scores[winner]}! Final: {scores}")
end
```

```text
Round 1
  Cautious rolls [2, 5, 3] and banks 10
  Bold rolls [6, 5, 5, 1]: a 1! Nothing this turn.
Round 2
  Cautious rolls [6, 5] and banks 11
  Bold rolls [6, 5, 4, 6] and banks 21
Round 3
  Cautious rolls [5, 1]: a 1! Nothing this turn.
  Bold rolls [3, 6, 1]: a 1! Nothing this turn.
Round 4
  Cautious rolls [2, 2, 6] and banks 10
Cautious wins with 31! Final: {"Cautious": 31, "Bold": 21}
```

`winner = ""` is a *flag*: the outer loop runs until a player sets it.

**Try it**

1. Add a third player with its own strategy.
2. Play 1000 silent games (no printing) and count how often each strategy
   wins. Which stopping total is best?

---

## Day 12: Scope, `let`, `var` and `const`, and a guessing game

**Goal:** know where a name lives, and fix the ones that should never
change.

A variable made inside a function or block belongs to it. Values meant to
be shared are passed in as parameters and handed back with `return`.

- `x = 1` makes a variable.
- `let x = 1` makes a name that can never be given a new value.
- `var x = 1` says clearly "this will change".
- `const X = 1` at the top of a file is a value fixed when the program is
  built, usable everywhere.

```vyrn
const MAX_TRIES = 5

create hint(guess, secret):
    if guess < secret: return "higher"
    if guess > secret: return "lower"
    return "correct"
end

main:
    let secret = 37
    var tries = 0
    each guess in [50, 25, 37]:
        tries += 1
        let answer = hint(guess, secret)
        print("{guess}? {answer}")
        if answer == "correct": stop
    end
    print("Found in {tries} of {MAX_TRIES} tries")
end
```

```text
50? lower
25? higher
37? correct
Found in 3 of 5 tries
```

Try writing `secret = 40` after the `let` line: `vyrn check` refuses,
because a `let` name cannot be given a new value. That is the point: the
name promises it won't change.

To read the guesses from the keyboard, use `input(prompt)`, which gives
back the line typed, as text:

```vyrn check
use vyrn.random

const MAX_TRIES = 7

main:
    let secret = random.int(1, 100)
    print("I'm thinking of a number from 1 to 100.")
    var tries = 0
    loop tries < MAX_TRIES:
        let guess = int(input("Your guess: "))
        tries += 1
        if guess < secret:
            print("Higher.")
        else if guess > secret:
            print("Lower.")
        else:
            print("Yes! {tries} tries.")
            return
        end
    end
    print("Out of tries: it was {secret}.")
end
```

`return` inside `main` ends the program.

**Try it**

1. Add an "easy" mode with 10 tries and a "hard" one with 5.
2. Tell the player "very close" when they are within 3.

---

## Day 13: When things go wrong

**Goal:** read error messages, and recover from failures.

When something fails and nothing handles it, Vyrn stops and says what,
where, and often how to fix it:

```vyrn
main:
    prices = {"tea": 2.5, "cake": 3.0}
    print(prices["tea"])
    print(prices["coffee"])
end
```

```text
2.5
error: KeyError: key "coffee" is not in the map
```

In a terminal the message also shows the line, with a hint:
`check with key in m first, or use m.get(key, default)`.

To handle a failure, wrap the risky code in `attempt:` and say what to do
in `catch`. `always:` runs whether or not anything failed. `catch Error`
catches every kind of failure.

```vyrn
create parse_age(text):
    attempt:
        return int(text)
    catch ValueError:
        return -1
    end
end

main:
    each entry in ["42", "forty", "7"]:
        print(entry, "->", parse_age(entry))
    end
    attempt:
        print(10 / 0)
    catch Error as e:
        print("Caught:", e)
    always:
        print("Done dividing.")
    end
end
```

```text
42 -> 42
forty -> -1
7 -> 7
Caught: DivisionByZero: division by zero
Done dividing.
```

Your own failures are structs (Day 17 covers them fully): declare one,
then `fail` it. The caller decides what to do.

```vyrn
struct TooCold:
    temp: float
end

create brew(temp):
    if temp < 90: fail TooCold(temp)
    return "Tea at {temp} degrees"
end

main:
    each t in [95, 70]:
        attempt:
            print(brew(t))
        catch TooCold as e:
            print("Water at {e.temp} is too cold for tea")
        end
    end
end
```

```text
Tea at 95 degrees
Water at 70 is too cold for tea
```

**Common mistakes, and what Vyrn says**

| You wrote | Vyrn says |
|---|---|
| `print(score)` before any `score = ...` | `` `score` is not defined `` |
| `if x > 1` (no colon) | `expected ':'` and points at the spot |
| a block without `end` | which block is still open, and where it began |
| `"3" + 4` | `cannot add str and int`; use `int("3") + 4` or `"3" + str(4)` |
| `xs[10]` on a short list | `index 10 is out of range for a list of length 3` |

**Try it:** take the calculator from Day 10 and make dividing by zero
print "Cannot divide by zero" using `attempt` and `catch DivisionByZero`.

---

## Day 14: Project day: Bigger or Smaller

**Goal:** a game driven by data.

The player sees two things and guesses which is bigger. The data is a list
of maps; each answer keeps the winner and brings in a new challenger.

```vyrn
use vyrn.random

const MOUNTAINS = [
    {"name": "Everest", "height": 8849},
    {"name": "K2", "height": 8611},
    {"name": "Kilimanjaro", "height": 5895},
    {"name": "Mont Blanc", "height": 4806},
    {"name": "Denali", "height": 6190},
    {"name": "Aconcagua", "height": 6961},
    {"name": "Fuji", "height": 3776},
]

create describe(m): return "{m["name"]}"

main:
    random.seed(3)
    pool = list(MOUNTAINS)
    random.shuffle(pool)
    a = pool.pop()
    score = 0
    # The player's answers, "a" or "b". Use input() to play for real.
    each answer in ["a", "b", "a", "a"]:
        b = pool.pop()
        print("A: {describe(a)}  vs  B: {describe(b)}  -> you say {answer}")
        bigger = "a" if a["height"] > b["height"] else "b"
        if answer != bigger:
            print("Wrong! {a["name"]} is {a["height"]} m, {b["name"]} is {b["height"]} m.")
            stop
        end
        score += 1
        print("Right! Score: {score}")
        if bigger == "b": a = b
    end
    print("Final score: {score}")
end
```

```text
A: Mont Blanc  vs  B: Fuji  -> you say a
Right! Score: 1
A: Mont Blanc  vs  B: Everest  -> you say b
Right! Score: 2
A: Everest  vs  B: Aconcagua  -> you say a
Right! Score: 3
A: Everest  vs  B: Denali  -> you say a
Right! Score: 4
Final score: 4
```

A `const` at the top of a file, like `MOUNTAINS`, is a fixed value every
function can read. It can be a whole list of maps.

**Try it:** play it from the keyboard with `input()`, and keep going until
the pool runs out.

---

## Day 15: Project day: the Smoothie Bar

**Goal:** a program with stock, money and a menu: the shape of many real
programs.

```vyrn
const MENU = {
    "berry": {"cost": 3.5, "needs": {"berries": 150, "yogurt": 100}},
    "mango": {"cost": 4.0, "needs": {"mango": 200, "yogurt": 50}},
    "green": {"cost": 4.5, "needs": {"spinach": 80, "mango": 100}},
}

create can_make(stock, needs):
    each item, amount in needs.items():
        if stock.get(item, 0) < amount:
            print("  Sorry, not enough {item}.")
            return false
        end
    end
    return true
end

create make(stock, needs):
    each item, amount in needs.items(): stock[item] -= amount
end

main:
    stock = {"berries": 300, "yogurt": 200, "mango": 250, "spinach": 100}
    till = 0.0
    orders = [["berry", 5.0], ["mango", 3.0], ["mango", 5.0], ["berry", 4.0], ["green", 5.0], ["report", 0]]
    each order in orders:
        let name = order[0]
        let paid = order[1]
        if name == "report":
            print("Stock: {stock}")
            print("Takings: {till:.2f}")
            skip
        end
        let drink = MENU[name]
        print("{name} smoothie, paid {paid:.2f}")
        if paid < drink["cost"]:
            print("  Not enough money: {drink["cost"]:.2f} needed. Refunded.")
            skip
        end
        if not can_make(stock, drink["needs"]): skip
        make(stock, drink["needs"])
        till += drink["cost"]
        print("  Here you go! Change: {paid - drink["cost"]:.2f}")
    end
end
```

```text
berry smoothie, paid 5.00
  Here you go! Change: 1.50
mango smoothie, paid 3.00
  Not enough money: 4.00 needed. Refunded.
mango smoothie, paid 5.00
  Here you go! Change: 1.00
berry smoothie, paid 4.00
  Sorry, not enough yogurt.
green smoothie, paid 5.00
  Sorry, not enough mango.
Stock: {"berries": 150, "yogurt": 50, "mango": 50, "spinach": 100}
Takings: 7.50
```

A map passed to a function is the same map, not a copy, so `make` changes
the stock that `main` sees.

**Try it:** add a "refill" order that tops every stock item back up, and
an "off" order that ends the program.

---

## Day 16: Objects

**Goal:** bundle data with the functions that work on it.

An `object` declares a new kind of thing (Python calls this a class). Its
fields are listed with their types; fields with a default can be left out
when making one. Functions inside it are *methods*, and `self` is the
object they were called on.

```vyrn
object BankAccount:
    owner: str
    balance: float = 0.0
    history: list = []

    create deposit(self, amount: float):
        self.balance += amount
        self.history.append("+{amount}")
    end

    create withdraw(self, amount: float) -> bool:
        if amount > self.balance: return false
        self.balance -= amount
        self.history.append("-{amount}")
        return true
    end
end

main:
    acct = BankAccount("Ada")
    acct.deposit(100)
    acct.withdraw(30)
    print(acct.withdraw(500))
    print(acct.owner, acct.balance, acct.history)
    print(acct)
    same = acct
    same.deposit(5)
    print(acct.balance)
end
```

```text
false
Ada 70.0 ["+100", "-30"]
BankAccount(owner="Ada", balance=70.0, history=["+100", "-30"])
75.0
```

The last lines show a key fact: an object is *shared*. `same = acct`
doesn't copy anything; both names are the same account.

Objects can build on others with `object Savings(BankAccount):`, adding
fields and methods or replacing them.

**Try it:** write an `object Pet` with a name, a hunger level and methods
`feed()` and `play()`, where playing makes it hungrier.

---

## Day 17: Structs and objects; a quiz app

**Goal:** choose between a struct and an object, and build a quiz with
both.

A `struct` is a *value*, like a number: assigning or passing it makes a
copy. Use structs for plain data (a point, a quiz question, a colour).
Use objects for things with an identity that changes over time (a player,
a bank account, a game).

```vyrn
struct Point:
    x: float
    y: float

    create moved(self, dx: float, dy: float) -> Point:
        return Point(self.x + dx, self.y + dy)
    end
end

main:
    a = Point(1.0, 2.0)
    b = a
    b.x = 9.0
    print(a, b)
    print(a.moved(1.0, 1.0))
end
```

```text
Point(x=1.0, y=2.0) Point(x=9.0, y=2.0)
Point(x=2.0, y=3.0)
```

Changing `b` left `a` alone: `b` was a copy.

**Project: the quiz.** Questions are structs; the quiz is an object that
keeps the score as it goes.

<details><summary>A solution</summary>

```vyrn
struct Question:
    text: str
    answer: bool
end

object Quiz:
    questions: list
    asked: int = 0
    score: int = 0

    create has_more(self) -> bool:
        return self.asked < len(self.questions)
    end

    create ask(self, reply: bool):
        let q = self.questions[self.asked]
        self.asked += 1
        let verdict = "right" if reply == q.answer else "wrong"
        if reply == q.answer: self.score += 1
        print("Q{self.asked}. {q.text} You said {reply}: {verdict}.")
    end
end

main:
    let quiz = Quiz([
        Question("Octopuses have three hearts.", true),
        Question("Venus is the closest planet to the Sun.", false),
        Question("Honey can last for thousands of years.", true),
    ])
    let replies = [true, true, true]
    var i = 0
    loop quiz.has_more():
        quiz.ask(replies[i])
        i += 1
    end
    print("Score: {quiz.score}/{len(quiz.questions)}")
end
```

```text
Q1. Octopuses have three hearts. You said true: right.
Q2. Venus is the closest planet to the Sun. You said true: wrong.
Q3. Honey can last for thousands of years. You said true: right.
Score: 2/3
```

</details>

**Try it:** load the questions from a list of maps (as if from a file)
and build the `Question`s with a comprehension.

---

## Day 18: Drawing pictures

**Goal:** draw shapes and save them as images.

`use vyrn.draw` brings in graphics. A *canvas* is a picture in memory.
Draw on it with `rect`, `circle`, `line` and `text`, then `save` it as a
PNG file and open it in any image viewer. Positions count from the top-left
corner, in pixels.

A *turtle* draws by moving: it starts in the middle, facing right, and
leaves a line wherever it goes.

```vyrn
use vyrn.draw

main:
    pic = draw.canvas(300, 300)
    pic.clear("white")
    pic.rect(10, 10, 80, 50, "orange")
    pic.circle(250, 50, 30, "skyblue")
    pic.line(0, 299, 299, 299, "black", width = 3)
    pic.text("Hello", 120, 30, "black")

    t = draw.turtle(pic)
    t.color("purple")
    each side in 1 to 5:
        t.forward(80)
        t.right(144)
    end
    print("Turtle ends at", round(t.x), round(t.y), "facing", round(t.heading))
    print("Pixel at (20, 20) is", pic.get_pixel(20, 20))
    pic.save("star.png")
    print("Saved star.png")
end
```

```text
Turtle ends at 0 0 facing 0
Pixel at (20, 20) is #fb8c00
Saved star.png
```

Colours can be names (`"red"`), hex codes (`"#ff8800"`) or lists of red,
green and blue from 0 to 255 (`[255, 136, 0]`).

**Project: spot painting.** Fill a canvas with a 10 by 10 grid of dots in
random colours from a palette, and save it.

<details><summary>A solution</summary>

```vyrn
use vyrn.draw
use vyrn.random

main:
    random.seed(5)
    let palette = ["#e63946", "#f1c453", "#2a9d8f", "#264653", "#e76f51"]
    let pic = draw.canvas(500, 500)
    pic.clear("#f4f1de")
    each row in 0 until 10:
        each col in 0 until 10:
            pic.circle(25 + col * 50, 25 + row * 50, 15, random.choice(palette))
        end
    end
    pic.save("spots.png")
    print("100 spots painted; the first is", pic.get_pixel(25, 25))
end
```

```text
100 spots painted; the first is #e76f51
```

</details>

---

## Day 19: Windows and the keyboard

**Goal:** an interactive program that responds to keys.

`draw.window` opens a window with the same drawing methods as a canvas.
The program runs a *game loop*: check the keys, update, draw, `show`.
`win.show()` waits so the loop runs 60 times a second.

```vyrn check
use vyrn.draw

main:
    let win = draw.window(600, 400, "Sketch")
    let pen = draw.turtle(win)
    win.clear("white")
    pen.color("black")
    loop win.open():
        if win.key_down("up"): pen.forward(3)
        if win.key_down("down"): pen.back(3)
        if win.key_down("left"): pen.left(4)
        if win.key_down("right"): pen.right(4)
        if win.key_pressed("c"): win.clear("white")
        win.show()
    end
end
```

`key_down` is true for as long as a key is held; `key_pressed` only on the
frame it goes down. Without a screen (on a server, say) a window fails
with a clear error, so drawing programs that only save pictures use a
canvas instead.

**Project: a turtle race.** Five turtles move forward a random distance
each round; the first past the finish line wins. Here it runs on a canvas
so you can see the result in `race.png`.

<details><summary>A solution</summary>

```vyrn
use vyrn.draw
use vyrn.random

main:
    random.seed(12)
    let colors = ["red", "orange", "green", "blue", "purple"]
    let pic = draw.canvas(500, 300)
    pic.clear("white")
    pic.line(450, 0, 450, 300, "black")  # turtle x 200 is the finish
    var racers = []
    each i, c in enumerate(colors):
        let t = draw.turtle(pic)
        t.color(c)
        t.pen_up()
        t.goto(-230, -100 + i * 50)
        t.pen_down()
        racers.append(t)
    end
    var winner = ""
    loop winner == "":
        each i, t in enumerate(racers):
            t.forward(random.int(0, 10))
            if t.x >= 200 and winner == "": winner = colors[i]
        end
    end
    pic.save("race.png")
    print("The {winner} turtle wins!")
end
```

```text
The purple turtle wins!
```

</details>

---

## Day 20: Snake, part 1: a snake that moves

**Goal:** design a game's data before drawing it.

A game is easiest to build when its *state* is plain data you can test
without a window. The snake is a list of cells, head first. Each step adds
a new head in the direction of travel and drops the tail, unless it has
just eaten.

```vyrn
const COLS = 10
const ROWS = 8

struct Cell:
    x: int
    y: int
end

create step(snake, dx, dy, food):
    let head = Cell((snake[0].x + dx) % COLS, (snake[0].y + dy) % ROWS)
    snake.insert(0, head)
    if head == food: return true
    snake.pop()
    return false
end

create picture(snake, food):
    var rows = []
    each y in 0 until ROWS:
        var line = ""
        each x in 0 until COLS:
            let c = Cell(x, y)
            if c == snake[0]:
                line += "@"
            else if c in snake:
                line += "o"
            else if c == food:
                line += "*"
            else:
                line += "."
            end
        end
        rows.append(line)
    end
    return "\n".join(rows)
end

main:
    var snake = [Cell(3, 3), Cell(2, 3), Cell(1, 3)]
    let food = Cell(6, 3)
    each _ in 1 to 3:
        if step(snake, 1, 0, food): print("Yum!")
    end
    print(picture(snake, food))
    print("Length: {len(snake)}")
end
```

```text
Yum!
..........
..........
..........
...ooo@...
..........
..........
..........
..........
Length: 4
```

Two structs with the same fields are equal (`head == food`), and `in`
works on a list of structs.

Running off one edge brings the snake back on the other side: `%` wraps
the position around. Tomorrow the same `step` drives a real window.

**Try it:** add a `crashed(snake)` function that tells whether the head
has hit the body.
