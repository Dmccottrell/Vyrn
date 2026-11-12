# Day 31: Capstone: flash cards

Today is a **capstone**: no new features, just a complete program built from
what you know. Flash cards help you learn words in another language: the
card shows a word, you try to remember its meaning, then flip it. Cards you
know are put away; cards you don't come back more often. You will use CSV
files (Day 25), maps and comprehensions (Day 26), random choice, saving
progress (Day 29) and careful errors (Day 30).

## 31.1 The cards

The words come from a CSV file. Loading them is a comprehension:

```vyrn
use vyrn.csv

create load_cards(path: str) -> list:
    return [(row["french"], row["english"]) each row in csv.read(path)]
end

main:
    write_text("french.csv", "french,english\nchat,cat\nchien,dog\nmaison,house\npomme,apple\nlivre,book\n")
    cards = load_cards("french.csv")
    print(len(cards), "cards; the first is", cards[0])
end
```

```text
5 cards; the first is ("chat", "cat")
```

Each card is a **tuple**: two values that belong together, front and back.

### Exercise

Load the cards the other way round, English first, so you can practise in
both directions.

```vyrn starter
use vyrn.csv

main:
    write_text("french.csv", "french,english\nchat,cat\nchien,dog\n")
end
```

```text expect
[("cat", "chat"), ("dog", "chien")]
```

> Hint: `[(row["english"], row["french"]) each row in csv.read("french.csv")]`

<details><summary>Solution</summary>

```vyrn solution
use vyrn.csv

main:
    write_text("french.csv", "french,english\nchat,cat\nchien,dog\n")
    print([(row["english"], row["french"]) each row in csv.read("french.csv")])
end
```

</details>

## 31.2 Boxes: learning what you don't know

A smart way to study is the **Leitner system**: every card sits in a box
numbered 1 to 3. New cards start in box 1. Get one right and it moves up a
box; get it wrong and it goes back to box 1. Box 1 cards come up most
often, so you spend your time on the words you don't know yet.

```vyrn
## Moves a card after an answer: up a box when right, back to 1 when wrong.
create after_answer(box: int, right: bool) -> int:
    if right: return min(box + 1, 3)
    return 1
end

main:
    var box = 1
    each right in [true, true, true, false, true]:
        box = after_answer(box, right)
        print("right" if right else "wrong", "-> box", box)
    end
end
```

```text
right -> box 2
right -> box 3
right -> box 3
wrong -> box 1
right -> box 2
```

Which card next? Pick at random, but count box 1 cards three times, box 2
twice and box 3 once, so the hard ones come up more:

```vyrn
use vyrn.random

create pick(boxes: map) -> str:
    var pool = []
    each word, box in boxes.items():
        each _ in 0 until 4 - box: pool.append(word)
    end
    return random.choice(pool)
end

main:
    random.seed(1)
    boxes = {"chat": 1, "chien": 3, "maison": 2}
    counts = {"chat": 0, "chien": 0, "maison": 0}
    each _ in 0 until 600:
        counts[pick(boxes)] += 1
    end
    print(counts["chat"] > counts["maison"], counts["maison"] > counts["chien"])
end
```

```text
true true
```

Over 600 picks the box 1 card comes up most, as planned.

### Exercise

Write `mastered(boxes)`, which gives the words in box 3, in alphabetical
order.

```vyrn starter
create mastered(boxes: map) -> list:
    return []
end

main:
    print(mastered({"pomme": 3, "chat": 1, "livre": 3}))
end
```

```text expect
["livre", "pomme"]
```

> Hint: `sorted([w each w, box in boxes.items() if box == 3])`

<details><summary>Solution</summary>

```vyrn solution
create mastered(boxes: map) -> list:
    return sorted([w each w, box in boxes.items() if box == 3])
end

main:
    print(mastered({"pomme": 3, "chat": 1, "livre": 3}))
end
```

</details>

## 31.3 Checking answers kindly

People type answers in many ways: `" Cat"`, `"the cat"`, `"cats"`. Being
strict is annoying, so the checker cleans both sides first and accepts a
leading "the" or "a":

```vyrn
create clean(answer: str) -> str:
    var a = answer.strip().lower()
    each article in ["the ", "a ", "an "]:
        if a.startswith(article): a = a[len(article):]
    end
    return a
end

create is_right(answer: str, expected: str) -> bool:
    return clean(answer) == clean(expected)
end

main:
    each answer in [" Cat", "the cat", "a cat", "dog", "cats"]:
        print(repr(answer), is_right(answer, "cat"))
    end
end
```

```text
" Cat" true
"the cat" true
"a cat" true
"dog" false
"cats" false
```

### Exercise

Accept small typos too: an answer is right if it differs from the expected
word by at most one letter in the same position (and has the same length).

```vyrn starter
create close_enough(answer: str, expected: str) -> bool:
    return answer == expected
end

main:
    print(close_enough("hosue", "house"), close_enough("hause", "house"), close_enough("hut", "house"))
end
```

```text expect
false true false
```

> Hint: if the lengths differ, it is wrong.
> Hint: count positions where they differ:
> `len([i each i in 0 until len(answer) if answer[i] != expected[i]])`.

<details><summary>Solution</summary>

```vyrn solution
create close_enough(answer: str, expected: str) -> bool:
    if len(answer) != len(expected): return false
    let wrong = len([i each i in 0 until len(answer) if answer[i] != expected[i]])
    return wrong <= 1
end

main:
    print(close_enough("hosue", "house"), close_enough("hause", "house"), close_enough("hut", "house"))
end
```

</details>

## 31.4 The whole program

Here everything comes together: load the cards and the saved progress,
study a round, save the progress. The answers are a list so you can see a
whole session; in the real program each comes from `input()`.

```vyrn
use vyrn.csv
use vyrn.json
use vyrn.fs
use vyrn.random

const PROGRESS = "progress.json"

create load_cards(path: str) -> map:
    return {row["french"]: row["english"] each row in csv.read(path)}
end

create load_boxes(cards: map) -> map:
    var boxes = {word: 1 each word in cards.keys()}
    if fs.exists(PROGRESS):
        attempt:
            boxes = boxes | json.load(PROGRESS)
        catch ValueError:
            print("progress file damaged; starting again")
        end
    end
    return boxes
end

create pick(boxes: map) -> str:
    var pool = []
    each word, box in boxes.items():
        each _ in 0 until 4 - box: pool.append(word)
    end
    return random.choice(pool)
end

main:
    write_text("french.csv", "french,english\nchat,cat\nchien,dog\nmaison,house\npomme,apple\n")
    if fs.exists(PROGRESS): fs.remove(PROGRESS)
    random.seed(4)
    cards = load_cards("french.csv")
    boxes = load_boxes(cards)
    var right = 0
    answers = ["dog", "cat", "house", "pear", "cat", "apple"]
    each answer in answers:
        word = pick(boxes)
        correct = answer.strip().lower() == cards[word]
        if correct:
            right += 1
            boxes[word] = min(boxes[word] + 1, 3)
        else:
            boxes[word] = 1
        end
        print("{word}? {answer} -> {"yes" if correct else "no, it's " + cards[word]}")
    end
    json.save(PROGRESS, boxes)
    print("{right} of {len(answers)} right; boxes now {json.load(PROGRESS)}")
end
```

```text
chat? dog -> no, it's cat
maison? cat -> no, it's house
maison? house -> yes
pomme? pear -> no, it's apple
chat? cat -> yes
chat? apple -> no, it's cat
2 of 6 right; boxes now {"chat": 1, "chien": 1, "maison": 2, "pomme": 1}
```

Look at `chat`: right once (up to box 2), then wrong (back to box 1).
`maison` was answered correctly and moved up to box 2, so it will come up
less often next time. Because the progress is saved, the next run continues
where this one left off.

### Exercise

After a session, print a short report: how many cards are in each box.

```vyrn starter
create report(boxes: map):
    print("box 1: 0")
end

main:
    report({"chat": 1, "chien": 3, "maison": 2, "pomme": 1})
end
```

```text expect
box 1: 2
box 2: 1
box 3: 1
```

> Hint: `each b in 1 to 3:` and count the words whose box is `b`.

<details><summary>Solution</summary>

```vyrn solution
create report(boxes: map):
    each b in 1 to 3:
        print("box {b}: {len([w each w, box in boxes.items() if box == b])}")
    end
end

main:
    report({"chat": 1, "chien": 3, "maison": 2, "pomme": 1})
end
```

</details>

## Today you built

A study program with saved progress, smart repetition and forgiving
answers, out of pieces you already knew: CSV, maps, comprehensions, random
choice, JSON and error handling. Ideas to take it further:

- Read each answer with `input()` and stop when the player types `quit`.
- Show the cards in a window with buttons for "knew it" and "didn't"
  (Day 27).
- Load several CSV files and let the player choose a language.
