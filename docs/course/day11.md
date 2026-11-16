# Day 11: Capstone: Twenty-One

Today is a capstone: a complete card game using everything from Unit 1.
In **Twenty-One**, you and the dealer are each dealt cards; you try to get
closer to 21 than the dealer without going over. Number cards count their
number, picture cards count 10, and an ace counts 11, or 1 if 11 would
take you over 21.

As on Day 7, the game is built one step at a time, and each step is
checked before the next.

## 11.1 Step 1: the deck and dealing

Represent each card by its value: `[11, 2, 3, 4, 5, 6, 7, 8, 9, 10, 10,
10, 10]` (ace, 2 to 10, then three picture cards). Dealing means picking a
random card.

### Exercise

Write `deal()` that returns a random card, and deal two to the player.

```vyrn starter
use vyrn.random

const CARDS = [11, 2, 3, 4, 5, 6, 7, 8, 9, 10, 10, 10, 10]

create deal():
    return 0
end

main:
    random.seed(21)
    hand = [deal(), deal()]
    print("Your cards:", hand)
end
```

```text expect
Your cards: [10, 4]
```

> Hint: `return random.choice(CARDS)`

<details><summary>Solution</summary>

```vyrn solution
use vyrn.random

const CARDS = [11, 2, 3, 4, 5, 6, 7, 8, 9, 10, 10, 10, 10]

create deal():
    return random.choice(CARDS)
end

main:
    random.seed(21)
    hand = [deal(), deal()]
    print("Your cards:", hand)
end
```

</details>

## 11.2 Step 2: scoring a hand

The score is the sum of the cards, with one twist: while the score is over
21 and the hand holds an ace counted as 11, count that ace as 1 instead
(take 10 off).

### Exercise

Write `score(hand)`.

```vyrn starter
create score(hand):
    return sum(hand)
end

main:
    print(score([10, 7]))
    print(score([11, 10]))
    print(score([11, 11]))
    print(score([11, 9, 5]))
    print(score([10, 9, 5]))
end
```

```text expect
17
21
12
15
24
```

> Hint: Count the aces: `aces = hand.count(11)`.
> Hint: `loop total > 21 and aces > 0:` take 10 off the total and one off `aces`.

<details><summary>Solution</summary>

```vyrn solution
create score(hand):
    total = sum(hand)
    aces = hand.count(11)
    loop total > 21 and aces > 0:
        total -= 10
        aces -= 1
    end
    return total
end

main:
    print(score([10, 7]))
    print(score([11, 10]))
    print(score([11, 11]))
    print(score([11, 9, 5]))
    print(score([10, 9, 5]))
end
```

</details>

## 11.3 Step 3: who wins?

### Exercise

Write `result(player, dealer)` taking two scores and returning one of:
`You went over. You lose.`, `Dealer went over. You win!`, `Draw.`,
`You win!`, `You lose.`. The order of the checks matters: going over is
checked first.

```vyrn starter
create result(player, dealer):
    return ""
end

main:
    print(result(23, 18))
    print(result(20, 25))
    print(result(19, 19))
    print(result(20, 18))
    print(result(17, 19))
end
```

```text expect
You went over. You lose.
Dealer went over. You win!
Draw.
You win!
You lose.
```

> Hint: `if player > 21: return ...` first, then `if dealer > 21: ...`.

<details><summary>Solution</summary>

```vyrn solution
create result(player, dealer):
    if player > 21: return "You went over. You lose."
    if dealer > 21: return "Dealer went over. You win!"
    if player == dealer: return "Draw."
    if player > dealer: return "You win!"
    return "You lose."
end

main:
    print(result(23, 18))
    print(result(20, 25))
    print(result(19, 19))
    print(result(20, 18))
    print(result(17, 19))
end
```

</details>

## 11.4 Step 4: the player's turn

The player sees their cards and one of the dealer's, then types `y` to
take another card or `n` to stop. Going over 21 ends the turn at once.

### Exercise

Write the player's turn. The checker types `y`, then `n`.

```vyrn starter
use vyrn.random

const CARDS = [11, 2, 3, 4, 5, 6, 7, 8, 9, 10, 10, 10, 10]

create deal(): return random.choice(CARDS)

create score(hand):
    total = sum(hand)
    aces = hand.count(11)
    loop total > 21 and aces > 0:
        total -= 10
        aces -= 1
    end
    return total
end

main:
    random.seed(3)
    player = [deal(), deal()]
    dealer = [deal(), deal()]
end
```

```text input
y
n
```

```text expect
Your cards: [7, 10], score 17. Dealer shows 10.
Another card? (y/n) You end with [7, 10, 8], score 25.
```

> Hint: `loop score(player) < 21:` show the hand, ask, and `stop` on anything but `y`.
> Hint: Show the dealer's first card with `dealer[0]`.

<details><summary>Solution</summary>

```vyrn solution
use vyrn.random

const CARDS = [11, 2, 3, 4, 5, 6, 7, 8, 9, 10, 10, 10, 10]

create deal(): return random.choice(CARDS)

create score(hand):
    total = sum(hand)
    aces = hand.count(11)
    loop total > 21 and aces > 0:
        total -= 10
        aces -= 1
    end
    return total
end

main:
    random.seed(3)
    player = [deal(), deal()]
    dealer = [deal(), deal()]
    loop score(player) < 21:
        print("Your cards: {player}, score {score(player)}. Dealer shows {dealer[0]}.")
        if input("Another card? (y/n) ") != "y": stop
        player.append(deal())
    end
    print("You end with {player}, score {score(player)}.")
end
```

</details>

## 11.5 Step 5: the dealer's turn, and the whole game

The dealer follows a fixed rule: keep taking cards while the score is under
17. Then the results are compared.

### Exercise

Put everything together. The checker types `n` (stand on the first two
cards).

```vyrn starter
use vyrn.random

const CARDS = [11, 2, 3, 4, 5, 6, 7, 8, 9, 10, 10, 10, 10]

create deal(): return random.choice(CARDS)

create score(hand):
    total = sum(hand)
    aces = hand.count(11)
    loop total > 21 and aces > 0:
        total -= 10
        aces -= 1
    end
    return total
end

create result(player, dealer):
    if player > 21: return "You went over. You lose."
    if dealer > 21: return "Dealer went over. You win!"
    if player == dealer: return "Draw."
    if player > dealer: return "You win!"
    return "You lose."
end

main:
    random.seed(8)
end
```

```text input
n
```

```text expect
Your cards: [4, 10], score 14. Dealer shows 10.
Another card? (y/n) You: [4, 10] = 14
Dealer: [10, 9] = 19
You lose.
```

> Hint: After the player's loop, `if score(player) <= 21:` let the dealer draw with `loop score(dealer) < 17:`.
> Hint: Finish with both hands and `print(result(score(player), score(dealer)))`.

<details><summary>Solution</summary>

```vyrn solution
use vyrn.random

const CARDS = [11, 2, 3, 4, 5, 6, 7, 8, 9, 10, 10, 10, 10]

create deal(): return random.choice(CARDS)

create score(hand):
    total = sum(hand)
    aces = hand.count(11)
    loop total > 21 and aces > 0:
        total -= 10
        aces -= 1
    end
    return total
end

create result(player, dealer):
    if player > 21: return "You went over. You lose."
    if dealer > 21: return "Dealer went over. You win!"
    if player == dealer: return "Draw."
    if player > dealer: return "You win!"
    return "You lose."
end

main:
    random.seed(8)
    player = [deal(), deal()]
    dealer = [deal(), deal()]
    loop score(player) < 21:
        print("Your cards: {player}, score {score(player)}. Dealer shows {dealer[0]}.")
        if input("Another card? (y/n) ") != "y": stop
        player.append(deal())
    end
    if score(player) <= 21:
        loop score(dealer) < 17: dealer.append(deal())
    end
    print("You: {player} = {score(player)}")
    print("Dealer: {dealer} = {score(dealer)}")
    print(result(score(player), score(dealer)))
end
```

</details>

**Try it:** remove the seed and play a few rounds. Then add a "play again?"
question around the whole game, and keep a running count of wins.

**Day 11 done.** You designed and built a full card game from small,
tested pieces: exactly how professional programs are made.
