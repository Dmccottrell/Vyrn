# Day 34: An API in a window

Yesterday's programs printed what an API said. Today the answers go into a
window. The project is a trivia quiz: questions come from the Open Trivia
Database, the player clicks **True** or **False**, and the window keeps the
score. On the way you meet text that arrives *encoded* for web pages, and a
clean way to split a program into its data, its logic and its window.

## 34.1 The questions

The Open Trivia Database gives questions as JSON. Ask for ten true-or-false
questions with:

```text
https://opentdb.com/api.php?amount=10&type=boolean
```

The answer looks like this (shortened to two questions):

```vyrn
use vyrn.json

const ANSWER = r'{"response_code": 0, "results": [{"category": "Science: Computers", "type": "boolean", "difficulty": "easy", "question": "The logo for Snapchat is a Bell.", "correct_answer": "False", "incorrect_answers": ["True"]}, {"category": "Geography", "type": "boolean", "difficulty": "easy", "question": "Greenland is covered with more than 50% of ice.", "correct_answer": "True", "incorrect_answers": ["False"]}]}'

main:
    data = json.parse(ANSWER)
    print("code", data["response_code"], "with", len(data["results"]), "questions")
    each q in data["results"]:
        print("-", q["question"], "->", q["correct_answer"])
    end
end
```

```text
code 0 with 2 questions
- The logo for Snapchat is a Bell. -> False
- Greenland is covered with more than 50% of ice. -> True
```

The API has its own `response_code` besides the HTTP status: `0` means
success, `1` means it doesn't have enough questions for what you asked.

## 34.2 Text made for web pages

Ask the real API and some questions come back looking odd:

```text
The word &quot;astronaut&quot; comes from Greek words meaning &quot;star sailor&quot;.
```

`&quot;` is an **HTML entity**: how a web page writes `"` without confusing
it with the page's own quotes. `&amp;` is `&`, `&#039;` is `'`, `&lt;` is
`<`. Turn them back with `html.unescape`:

```vyrn
use vyrn.html

main:
    raw = "The word &quot;astronaut&quot; comes from Greek words meaning &quot;star sailor&quot;."
    print(html.unescape(raw))
    print(html.unescape("Tom &amp; Jerry&#039;s 5 &lt; 6"))
end
```

```text
The word "astronaut" comes from Greek words meaning "star sailor".
Tom & Jerry's 5 < 6
```

`html.escape` goes the other way, for when you put text into a page
(Day 41).

### Exercise

Clean every question in the list, and print them numbered from 1.

```vyrn starter
use vyrn.html

main:
    questions = ["Pac-Man was first called &quot;Puck Man&quot;.", "R&amp;B stands for Rhythm and Blues."]
end
```

```text expect
1. Pac-Man was first called "Puck Man".
2. R&B stands for Rhythm and Blues.
```

> Hint: `each i, q in enumerate(questions):` and print `i + 1`.

<details><summary>Solution</summary>

```vyrn solution
use vyrn.html

main:
    questions = ["Pac-Man was first called &quot;Puck Man&quot;.", "R&amp;B stands for Rhythm and Blues."]
    each i, q in enumerate(questions):
        print("{i + 1}. {html.unescape(q)}")
    end
end
```

</details>

## 34.3 The quiz logic, without a window

Keep the rules of the game in an object that knows nothing about windows or
the internet. Then it can be tested with plain lists, and the same object
works with a window, a terminal, or a phone app later:

```vyrn
use vyrn.html

object Question:
    text: str
    answer: str
end

object Quiz:
    questions: list
    number: int = 0
    score: int = 0

    create has_more(self) -> bool:
        return self.number < len(self.questions)
    end

    create current(self) -> str:
        return "Q{self.number + 1}: {self.questions[self.number].text}"
    end

    ## Checks the answer, moves to the next question, and says how it went.
    create answer(mut self, guess: str) -> bool:
        let right = guess == self.questions[self.number].answer
        if right: self.score += 1
        self.number += 1
        return right
    end
end

create from_api(results: list) -> list:
    return [Question(html.unescape(r["question"]), r["correct_answer"]) each r in results]
end

main:
    results = [{"question": "The sun is a star.", "correct_answer": "True"}, {"question": "Spiders have six legs.", "correct_answer": "False"}, {"question": "Gold&#039;s symbol is Ag.", "correct_answer": "False"}]
    quiz = Quiz(from_api(results))
    each guess in ["True", "True", "False"]:
        print(quiz.current())
        print("  you said {guess}:", "right" if quiz.answer(guess) else "wrong")
    end
    print("score {quiz.score}/{len(quiz.questions)}; more? {quiz.has_more()}")
end
```

```text
Q1: The sun is a star.
  you said True: right
Q2: Spiders have six legs.
  you said True: wrong
Q3: Gold's symbol is Ag.
  you said False: right
score 2/3; more? false
```

### Exercise

Add a method `percent(self)` that gives the score as a whole-number
percentage of the questions answered so far (0 when none have been
answered).

```vyrn starter
object Quiz:
    answered: int = 0
    score: int = 0

    create percent(self) -> int:
        return 0
    end
end

main:
    print(Quiz().percent(), Quiz(answered = 3, score = 2).percent(), Quiz(answered = 4, score = 4).percent())
end
```

```text expect
0 67 100
```

> Hint: guard against dividing by zero first.
> Hint: `round(100 * self.score / self.answered)`

<details><summary>Solution</summary>

```vyrn solution
object Quiz:
    answered: int = 0
    score: int = 0

    create percent(self) -> int:
        if self.answered == 0: return 0
        return round(100 * self.score / self.answered)
    end
end

main:
    print(Quiz().percent(), Quiz(answered = 3, score = 2).percent(), Quiz(answered = 4, score = 4).percent())
end
```

</details>

## 34.4 Fetching the questions

Fetching is one small function. It fails with a clear error when the API
doesn't deliver, so the window can show a message instead of crashing:

```vyrn check
use vyrn.http

create fetch_questions(amount: int = 10) -> list:
    let r = http.get("https://opentdb.com/api.php?" + http.query({"amount": amount, "type": "boolean"}))
    if not r.ok: fail NetworkError("the trivia server said {r.status}")
    if r.json["response_code"] != 0: fail ValueError("not enough questions")
    return r.json["results"]
end

main:
    each q in fetch_questions(3):
        print(q["question"])
    end
end
```

## 34.5 Long text in a window

A window label is one line; a long question runs off the edge. **Wrap** it:
add words to a line until the next one wouldn't fit, then start a new line:

```vyrn
create wrap(text: str, width: int) -> list:
    var lines = [""]
    each word in text.split():
        if lines[-1] and len(lines[-1]) + 1 + len(word) > width:
            lines.append(word)
        else:
            lines[-1] = (lines[-1] + " " + word).strip()
        end
    end
    return lines
end

main:
    each line in wrap("The Great Wall of China is visible from the Moon with the naked eye.", 24):
        print("|" + line.ljust(24) + "|")
    end
end
```

```text
|The Great Wall of China |
|is visible from the Moon|
|with the naked eye.     |
```

With the window's text at its normal size, each letter is 16 pixels wide,
so a 520-pixel window fits about 28 letters a line after its margins.

## 34.6 Project: the trivia window

The window draws whatever the quiz says now, and calls the quiz's methods
when a button is clicked. The whole program, run with `vyrn run` (it needs
the internet):

```vyrn check
use vyrn.draw
use vyrn.html
use vyrn.http

object Question:
    text: str
    answer: str
end

object Quiz:
    questions: list
    number: int = 0
    score: int = 0

    create has_more(self) -> bool:
        return self.number < len(self.questions)
    end

    create current(self) -> str:
        return self.questions[self.number].text
    end

    create answer(mut self, guess: str) -> bool:
        let right = guess == self.questions[self.number].answer
        if right: self.score += 1
        self.number += 1
        return right
    end
end

create wrap(text: str, width: int) -> list:
    var lines = [""]
    each word in text.split():
        if lines[-1] and len(lines[-1]) + 1 + len(word) > width:
            lines.append(word)
        else:
            lines[-1] = (lines[-1] + " " + word).strip()
        end
    end
    return lines
end

create fetch_questions() -> list:
    let r = http.get("https://opentdb.com/api.php?amount=10&type=boolean")
    if not r.ok: fail NetworkError("the trivia server said {r.status}")
    return [Question(html.unescape(q["question"]), q["correct_answer"]) each q in r.json["results"]]
end

main:
    let win = draw.window(520, 300, "Trivia")
    var quiz = Quiz([])
    var message = "Loading..."
    attempt:
        quiz = Quiz(fetch_questions())
        message = ""
    catch Error as e:
        message = "Could not load questions: {e.message}"
    end
    var flash = "#263238"
    loop win.open():
        win.clear(flash)
        win.layout(24, 24, gap = 16)
        win.label("Score: {quiz.score}/{quiz.number}", color = "#90caf9")
        if quiz.has_more():
            each line in wrap(quiz.current(), 28):
                win.label(line)
            end
            win.row()
            each choice in ["True", "False"]:
                if win.button(choice, w = 100):
                    flash = "#2e7d32" if quiz.answer(choice) else "#c62828"
                end
            end
            win.end_row()
        else:
            win.label(message or "Finished! {quiz.score} of {len(quiz.questions)}.")
        end
        win.show()
    end
end
```

The background flashes green or red after each answer. The window code has
almost no logic of its own: it asks the quiz what to show, and tells the
quiz what was clicked. That split, **logic** in one place and **display**
in another, is what lets you test the quiz on 34.3's plain lists.

### Exercise

The flash stays forever: once it turns green it never goes back. Make it
fade back after half a second. Write `background(since)`, which is given the
seconds since the last answer and the colour it flashed, and gives the
colour to clear the window with now.

```vyrn starter
create background(since: float, flash: str) -> str:
    return flash
end

main:
    print(background(0.1, "#2e7d32"), background(0.6, "#2e7d32"), background(0.2, "#c62828"))
end
```

```text expect
#2e7d32 #263238 #c62828
```

> Hint: under 0.5 seconds, keep the flash; after, give the normal
> `"#263238"`.
> Hint: in the window, keep `answered_at = time.now()` when a button is
> clicked, and call `background(time.now() - answered_at, flash)`.

<details><summary>Solution</summary>

```vyrn solution
create background(since: float, flash: str) -> str:
    if since < 0.5: return flash
    return "#263238"
end

main:
    print(background(0.1, "#2e7d32"), background(0.6, "#2e7d32"), background(0.2, "#c62828"))
end
```

</details>

## Today you learned

- APIs can have their own success codes inside the JSON, besides the HTTP
  status.
- `html.unescape` turns `&quot;`, `&amp;` and friends back into text.
- Keep the rules in an object of their own, tested without a window or the
  internet; the window only shows what the object says and passes on clicks.
