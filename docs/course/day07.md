# Day 7: Project Day: Word Guess

Today there's nothing new to learn: you combine lists, loops, decisions and
functions into a complete game. The computer picks a secret word; the
player guesses one letter at a time; each wrong guess costs a life.

Big programs are built in small steps, each one working before the next
begins. Each lesson today is one step.

## 7.1 Step 1: pick a secret word

### Exercise

Pick a random word from the list (keep the seed) and print it, so you can
check the next steps. Real games hide it, of course.

```vyrn starter
use vyrn.random

const WORDS = ["planet", "rocket", "galaxy", "comet", "nebula"]

main:
    random.seed(4)
end
```

```text expect
Secret: rocket
```

> Hint: `secret = random.choice(WORDS)`

<details><summary>Solution</summary>

```vyrn solution
use vyrn.random

const WORDS = ["planet", "rocket", "galaxy", "comet", "nebula"]

main:
    random.seed(4)
    secret = random.choice(WORDS)
    print("Secret:", secret)
end
```

</details>

## 7.2 Step 2: show the blanks

The player sees one `_` for each letter not yet found. Write a function
`show(secret, found)` where `found` is the list of letters guessed so far.

### Exercise

```vyrn starter
create show(secret, found):
    shown = ""
    # Add each letter of `secret` if it was found, or "_" if not.
    return shown
end

main:
    print(show("rocket", []))
    print(show("rocket", ["o", "t"]))
    print(show("rocket", ["r", "o", "c", "k", "e", "t"]))
end
```

```text expect
______
_o___t
rocket
```

> Hint: `each letter in secret:`
> Hint: `shown += letter if letter in found else "_"`

<details><summary>Solution</summary>

```vyrn solution
create show(secret, found):
    shown = ""
    each letter in secret:
        shown += letter if letter in found else "_"
    end
    return shown
end

main:
    print(show("rocket", []))
    print(show("rocket", ["o", "t"]))
    print(show("rocket", ["r", "o", "c", "k", "e", "t"]))
end
```

</details>

## 7.3 Step 3: check one guess

### Exercise

Handle one guess: if the letter is in the word, add it to `found` and say
so; otherwise take a life. Then show the word and the lives.

```vyrn starter
create show(secret, found):
    shown = ""
    each letter in secret:
        shown += letter if letter in found else "_"
    end
    return shown
end

main:
    secret = "comet"
    found = []
    lives = 6
    guess = "m"
end
```

```text expect
Yes, m is in the word.
__m__  lives: 6
```

> Hint: `if guess in secret:` ... `else:` `lives -= 1` ... `end`
> Hint: `print("{show(secret, found)}  lives: {lives}")`

<details><summary>Solution</summary>

```vyrn solution
create show(secret, found):
    shown = ""
    each letter in secret:
        shown += letter if letter in found else "_"
    end
    return shown
end

main:
    secret = "comet"
    found = []
    lives = 6
    guess = "m"
    if guess in secret:
        found.append(guess)
        print("Yes, {guess} is in the word.")
    else:
        lives -= 1
        print("No {guess}.")
    end
    print("{show(secret, found)}  lives: {lives}")
end
```

</details>

## 7.4 Step 4: keep going until the game ends

The game ends when the word has no `_` left (a win) or lives reach 0 (a
loss). A `loop` with that condition repeats the guess step.

### Exercise

Put step 3 inside a loop that reads guesses with `input`. The checker
guesses `e`, `a`, `t`, `c`, `x`, `o`, `m`.

```vyrn starter
create show(secret, found):
    shown = ""
    each letter in secret:
        shown += letter if letter in found else "_"
    end
    return shown
end

main:
    secret = "comet"
    found = []
    lives = 6
end
```

```text input
e
a
t
c
x
o
m
```

```text expect
Guess a letter: ___e_  lives: 6
Guess a letter: ___e_  lives: 5
Guess a letter: ___et  lives: 5
Guess a letter: c__et  lives: 5
Guess a letter: c__et  lives: 4
Guess a letter: co_et  lives: 4
Guess a letter: comet  lives: 4
You win! The word was comet.
```

> Hint: `loop lives > 0 and "_" in show(secret, found):`
> Hint: After the loop, check which way it ended: `if lives > 0:` you won.

<details><summary>Solution</summary>

```vyrn solution
create show(secret, found):
    shown = ""
    each letter in secret:
        shown += letter if letter in found else "_"
    end
    return shown
end

main:
    secret = "comet"
    found = []
    lives = 6
    loop lives > 0 and "_" in show(secret, found):
        guess = input("Guess a letter: ")
        if guess in secret:
            found.append(guess)
        else:
            lives -= 1
        end
        print("{show(secret, found)}  lives: {lives}")
    end
    if lives > 0:
        print("You win! The word was {secret}.")
    else:
        print("Out of lives. The word was {secret}.")
    end
end
```

</details>

## 7.5 Step 5: polish

Good games forgive small mistakes:

- Guessing the same letter twice shouldn't cost a life.
- `E` should count the same as `e`.
- Typing more than one letter should get a gentle message.

### Exercise

Add all three. The checker guesses `T`, `t`, `ab`, `x`, `o`, `c`, `m`, `e`.

```vyrn starter
create show(secret, found):
    shown = ""
    each letter in secret:
        shown += letter if letter in found else "_"
    end
    return shown
end

main:
    secret = "comet"
    found = []
    tried = []
    lives = 6
end
```

```text input
T
t
ab
x
o
c
m
e
```

```text expect
Guess a letter: ____t  lives: 6
Guess a letter: You already tried t.
Guess a letter: One letter at a time, please.
Guess a letter: ____t  lives: 5
Guess a letter: _o__t  lives: 5
Guess a letter: co__t  lives: 5
Guess a letter: com_t  lives: 5
Guess a letter: comet  lives: 5
You win! The word was comet.
```

> Hint: `guess = input(...).lower()` handles capitals.
> Hint: Check `len(guess) != 1` first, then `guess in tried`, each with `skip`.
> Hint: Remember every guess with `tried.append(guess)`.

<details><summary>Solution</summary>

```vyrn solution
create show(secret, found):
    shown = ""
    each letter in secret:
        shown += letter if letter in found else "_"
    end
    return shown
end

main:
    secret = "comet"
    found = []
    tried = []
    lives = 6
    loop lives > 0 and "_" in show(secret, found):
        guess = input("Guess a letter: ").lower()
        if len(guess) != 1:
            print("One letter at a time, please.")
            skip
        end
        if guess in tried:
            print("You already tried {guess}.")
            skip
        end
        tried.append(guess)
        if guess in secret:
            found.append(guess)
        else:
            lives -= 1
        end
        print("{show(secret, found)}  lives: {lives}")
    end
    if lives > 0:
        print("You win! The word was {secret}.")
    else:
        print("Out of lives. The word was {secret}.")
    end
end
```

</details>

## 7.6 Step 6: the finished game

The last step brings back the random word and adds a picture that grows
with each lost life. Run it yourself and play: this one isn't checked,
because the word is different every time.

```vyrn check
use vyrn.random

const WORDS = ["planet", "rocket", "galaxy", "comet", "nebula", "orbit", "meteor"]
const PICTURES = ["  ", " o", " o/", "\\o/", "\\o/\n |", "\\o/\n |\n/", "\\o/\n |\n/ \\"]

create show(secret, found):
    shown = ""
    each letter in secret:
        shown += letter if letter in found else "_"
    end
    return shown
end

main:
    secret = random.choice(WORDS)
    found = []
    tried = []
    lives = 6
    print("Guess the space word!")
    loop lives > 0 and "_" in show(secret, found):
        print(show(secret, found))
        guess = input("Letter: ").lower()
        if len(guess) != 1 or guess in tried:
            print("Try a new single letter.")
            skip
        end
        tried.append(guess)
        if guess in secret:
            found.append(guess)
        else:
            lives -= 1
            print(PICTURES[6 - lives])
        end
    end
    if lives > 0:
        print("You win! The word was {secret}.")
    else:
        print("Out of lives. The word was {secret}.")
    end
end
```

**Try it:** add a hint command: typing `?` reveals one hidden letter but
costs a life.

**Day 7 done.** You built a whole game in six careful steps. Tomorrow:
text in depth, and secret codes.
