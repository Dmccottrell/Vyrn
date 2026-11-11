# Day 8: Working with Text, and Secret Codes

Text is everywhere: names, messages, files, web pages. Today you learn
Vyrn's tools for taking text apart and putting it back together, then use
them to build the Caesar cipher, a code Julius Caesar is said to have used
for his letters.

## 8.1 Characters and positions

A string is a sequence of **characters**: letters, digits, spaces,
punctuation. Like a list, each has an index from 0, and slices work the
same way:

```vyrn
main:
    word = "Vyrnlang"
    print(word[0], word[-1], len(word))
    print(word[0:4], word[4:])
    print(word[::-1])
    print(word.chars())
end
```

```text
V g 8
Vyrn lang
gnalnryV
["V", "y", "r", "n", "l", "a", "n", "g"]
```

Unlike a list, a string can't be changed in place: `word[0] = "B"` is an
error. Instead, you build a **new** string, as in `"B" + word[1:]`.

### Exercise

Print the initials of a full name, then the name backwards.

```vyrn starter
main:
    first = "Grace"
    last = "Hopper"
end
```

```text expect
Initials: G.H.
Backwards: reppoH ecarG
```

> Hint: `first[0]` is the first letter.
> Hint: The full name is `first + " " + last`; reverse it with `[::-1]`.

<details><summary>Solution</summary>

```vyrn solution
main:
    first = "Grace"
    last = "Hopper"
    print("Initials: {first[0]}.{last[0]}.")
    full = first + " " + last
    print("Backwards:", full[::-1])
end
```

</details>

## 8.2 String methods

Strings come with many methods. Each gives back a **new** string (or other
value) and leaves the original alone:

```vyrn
main:
    s = "  Hello, Vyrn World  "
    print("[" + s.strip() + "]")
    t = s.strip()
    print(t.upper(), t.lower())
    print(t.replace("World", "friends"))
    print(t.split(", "))
    print(t.startswith("Hello"), t.endswith("!"))
    print(t.find("Vyrn"), t.count("o"))
    print("-".join(["a", "b", "c"]))
    print("vyrn is fun".title(), "42".isdigit(), "abc".isalpha())
end
```

```text
[Hello, Vyrn World]
HELLO, VYRN WORLD hello, vyrn world
Hello, Vyrn friends
["Hello", "Vyrn World"]
true false
7 2
a-b-c
Vyrn Is Fun true true
```

| Method | Gives |
|---|---|
| `strip()`, `lstrip()`, `rstrip()` | the text without spaces at the ends |
| `upper()`, `lower()`, `title()`, `capitalize()` | changed capitals |
| `replace(old, new)` | every `old` replaced by `new` |
| `split(sep)` | a list of the parts between `sep`s (spaces if left out) |
| `sep.join(list)` | the list's strings joined with `sep` between them |
| `startswith(x)`, `endswith(x)` | `true` or `false` |
| `find(x)` | the index of `x`, or `-1` if it isn't there |
| `count(x)` | how many times `x` appears |
| `isdigit()`, `isalpha()`, `isspace()` | whether every character is a digit, a letter, a space |

### Exercise

Tidy up a messy name typed by a user: remove spaces at the ends, and make
it capitalised properly. The checker types `   aDA   lOVELACE  `.

```vyrn starter
main:
    raw = input("Name: ")
end
```

```text input
   aDA   lOVELACE  
```

```text expect
Name: Hello, Ada Lovelace!
```

> Hint: `.split()` with nothing in the brackets splits on any run of spaces, dropping the extra ones.
> Hint: `" ".join(raw.split())` collapses the spaces; then `.title()`.

<details><summary>Solution</summary>

```vyrn solution
main:
    raw = input("Name: ")
    name = " ".join(raw.split()).title()
    print("Hello, {name}!")
end
```

</details>

## 8.3 Counting words

`split` and loops together answer many questions about text:

```vyrn
main:
    text = "the cat sat on the mat with the hat"
    words = text.split()
    print(len(words), "words")
    longest = ""
    each w in words:
        if len(w) > len(longest): longest = w
    end
    print("first longest:", longest)
    print("'the' appears", words.count("the"), "times")
end
```

```text
9 words
first longest: with
'the' appears 3 times
```

### Exercise

Count the vowels and the consonants in a sentence (ignore spaces and
punctuation).

```vyrn starter
main:
    sentence = "Programming is a superpower!"
end
```

```text expect
vowels: 9, consonants: 15
```

> Hint: Loop over `sentence.lower()`.
> Hint: `if c in "aeiou":` it's a vowel; `else if c.isalpha():` it's a consonant.

<details><summary>Solution</summary>

```vyrn solution
main:
    sentence = "Programming is a superpower!"
    vowels = 0
    consonants = 0
    each c in sentence.lower():
        if c in "aeiou":
            vowels += 1
        else if c.isalpha():
            consonants += 1
        end
    end
    print("vowels: {vowels}, consonants: {consonants}")
end
```

</details>

## 8.4 Characters are numbers underneath

Every character has a number, its **code**. `ord` gives a character's code
and `chr` turns a code back into a character. Letters are in order, so
`ord("b")` is one more than `ord("a")`:

```vyrn
main:
    print(ord("a"), ord("b"), ord("z"), ord("A"))
    print(chr(97), chr(65), chr(9731))
    print(chr(ord("c") + 1))
end
```

```text
97 98 122 65
a A ☃
d
```

(Languages like C++ and Java have a separate `char` type for one
character. In Vyrn a character is simply a string of length 1.)

### Exercise

Print the alphabet in capitals, without typing it, using `chr` and a range.

```vyrn starter
main:
end
```

```text expect
ABCDEFGHIJKLMNOPQRSTUVWXYZ
```

> Hint: `ord("A")` is 65 and `ord("Z")` is 90.
> Hint: Build it: `letters = ""` then `each code in 65 to 90: letters += chr(code)`.

<details><summary>Solution</summary>

```vyrn solution
main:
    letters = ""
    each code in ord("A") to ord("Z"):
        letters += chr(code)
    end
    print(letters)
end
```

</details>

## 8.5 Wrapping around with `%`

The Caesar cipher shifts each letter forward in the alphabet: with a shift
of 3, `a` becomes `d`, `b` becomes `e`... and `x` becomes `a`, wrapping
around the end. `%` does the wrapping: positions go 0 to 25, and
`(position + shift) % 26` always lands back in that range.

```vyrn
main:
    alphabet = "abcdefghijklmnopqrstuvwxyz"
    each letter in "axyz":
        i = alphabet.find(letter)
        print(letter, "->", alphabet[(i + 3) % 26])
    end
    print(-1 % 26)
end
```

```text
a -> d
x -> a
y -> b
z -> c
25
```

`-1 % 26` is `25`: in Vyrn, `%` with a positive right side never gives a
negative answer, so shifting backwards wraps around too.

### Exercise

A clock shows hours 0 to 23. Print what time it is 50 hours after 18:00.

```vyrn starter
main:
    now = 18
    later = 50
end
```

```text expect
It will be 20:00
```

> Hint: `(now + later) % 24`

<details><summary>Solution</summary>

```vyrn solution
main:
    now = 18
    later = 50
    print("It will be {(now + later) % 24}:00")
end
```

</details>

## 8.6 The Caesar cipher

### Exercise

Write `encrypt(text, shift)`: shift each letter by `shift`, keep anything
that isn't a lowercase letter (spaces, punctuation) as it is.

```vyrn starter
const ALPHABET = "abcdefghijklmnopqrstuvwxyz"

main:
    print(encrypt("meet me at noon", 3))
    print(encrypt("zebra!", 1))
end
```

```text expect
phhw ph dw qrrq
afcsb!
```

> Hint: Start with `result = ""` and loop over the text.
> Hint: `i = ALPHABET.find(c)`; if `i == -1` keep `c`, otherwise add `ALPHABET[(i + shift) % 26]`.

<details><summary>Solution</summary>

```vyrn solution
const ALPHABET = "abcdefghijklmnopqrstuvwxyz"

create encrypt(text, shift):
    result = ""
    each c in text:
        i = ALPHABET.find(c)
        if i == -1:
            result += c
        else:
            result += ALPHABET[(i + shift) % 26]
        end
    end
    return result
end

main:
    print(encrypt("meet me at noon", 3))
    print(encrypt("zebra!", 1))
end
```

</details>

## 8.7 Decoding: reuse, don't rewrite

To decode, shift *backwards*. There's no need for a second function:
`encrypt(secret, -3)` undoes `encrypt(text, 3)`, because `%` wraps
negative positions too. Spotting that one function can do both jobs is a
big part of programming well.

### Exercise

Write `caesar(text, shift, direction)` where `direction` is `"encode"` or
`"decode"`, using one shift calculation. The checker types `decode`,
`wkh fdnh lv d olh`, `3`.

```vyrn starter
const ALPHABET = "abcdefghijklmnopqrstuvwxyz"

main:
    direction = input("encode or decode? ")
    text = input("Message: ")
    shift = int(input("Shift: "))
end
```

```text input
decode
wkh fdnh lv d olh
3
```

```text expect
encode or decode? Message: Shift: Result: the cake is a lie
```

> Hint: `if direction == "decode": shift = -shift`, then encode as usual.

<details><summary>Solution</summary>

```vyrn solution
const ALPHABET = "abcdefghijklmnopqrstuvwxyz"

create caesar(text, shift, direction):
    if direction == "decode": shift = -shift
    result = ""
    each c in text:
        i = ALPHABET.find(c)
        result += c if i == -1 else ALPHABET[(i + shift) % 26]
    end
    return result
end

main:
    direction = input("encode or decode? ")
    text = input("Message: ")
    shift = int(input("Shift: "))
    print("Result:", caesar(text, shift, direction))
end
```

</details>

## 8.8 Long text and special strings

Text over several lines goes between triple quotes. Every string fills in
`{...}`; when you want real braces, or backslashes, as they are, put `r`
before the quote for a **raw** string:

```vyrn
main:
    name = "Ada"
    letter = """Dear {name},
Thank you for your code.
Best wishes"""
    print(letter)
    print(r"C:\new\folder has {no} interpolation")
end
```

```text
Dear Ada,
Thank you for your code.
Best wishes
C:\new\folder has {no} interpolation
```

### Exercise

Print a small framed menu using one triple-quoted string with
`{dish}` and `{price:.2f}` filled in.

```vyrn starter
main:
    dish = "Soup"
    price = 4.5
end
```

```text expect
+-----------+
| Soup 4.50 |
+-----------+
```

> Hint: Put the three lines between `"""` and `"""`.

<details><summary>Solution</summary>

```vyrn solution
main:
    dish = "Soup"
    price = 4.5
    print("""+-----------+
| {dish} {price:.2f} |
+-----------+""")
end
```

</details>

**Day 8 done.** You can slice, search, change and build text, and you've
written your first cipher. Tomorrow: maps, for looking things up by name.
