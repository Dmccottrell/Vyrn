# Day 69: Capstone: an expression evaluator

This unit's capstone builds a small piece of a programming language: a
calculator that understands `2 * (3 + 4) - x / 2`, with variables and
clear error messages. It's how `vyrn` itself works, in miniature: text
becomes **tokens**, tokens become a **tree**, and the tree is **evaluated**.
You'll use enums and patterns (Day 61), types (Day 60), traits (Day 62),
recursion (Day 68) and your own errors (Day 30).

## 69.1 Tokens

The first step reads the text and cuts it into **tokens**: numbers,
names, operators and brackets, skipping spaces. Each kind of token is an
enum variant:

```vyrn
enum Token:
    Num(value: float)
    Name(text: str)
    Op(symbol: str)
end

struct SyntaxError:
    message: str
end

create tokenize(text: str) -> list[Token]:
    var tokens = []
    var i = 0
    loop i < len(text):
        let c = text[i]
        if c == " ":
            i += 1
        else if c.isdigit() or c == ".":
            var j = i
            loop j < len(text) and (text[j].isdigit() or text[j] == "."): j += 1
            tokens.append(Token.Num(float(text[i:j])))
            i = j
        else if c.isalpha():
            var j = i
            loop j < len(text) and text[j].isalnum(): j += 1
            tokens.append(Token.Name(text[i:j]))
            i = j
        else if c in "+-*/^()":
            tokens.append(Token.Op(c))
            i += 1
        else:
            fail SyntaxError("I don't know the character '{c}' at position {i + 1}")
        end
    end
    return tokens
end

main:
    print(tokenize("2 * (x + 3.5)"))
    attempt:
        tokenize("2 # 3")
    catch SyntaxError as e:
        print(e.message)
    end
end
```

```text
[Token.Num(value=2.0), Token.Op(symbol="*"), Token.Op(symbol="("), Token.Name(text="x"), Token.Op(symbol="+"), Token.Num(value=3.5), Token.Op(symbol=")")]
I don't know the character '#' at position 3
```

## 69.2 The tree

`2 * (x + 3.5)` isn't really a flat list: the `+` happens first, and its
result is multiplied. A **syntax tree** shows that structure. Each node is
a number, a variable, or an operation with the nodes it combines:

```text
      *
     / \
    2   +
       / \
      x   3.5
```

As an enum, a node that holds other nodes is a variant with node fields:

```vyrn
enum Node:
    Num(value: float)
    Var(name: str)
    Neg(inner: Node)
    BinOp(op: str, left: Node, right: Node)
end

create show(n: Node) -> str:
    when n:
        is Node.Num(v): return str(v)
        is Node.Var(name): return name
        is Node.Neg(inner): return "-{show(inner)}"
        is Node.BinOp(op, a, b): return "({show(a)} {op} {show(b)})"
    end
end

main:
    let tree = Node.BinOp("*", Node.Num(2.0), Node.BinOp("+", Node.Var("x"), Node.Num(3.5)))
    print(show(tree))
end
```

```text
(2.0 * (x + 3.5))
```

`show` is recursive because the tree is: to show an operation, show its
two sides. Almost every function on a tree has this shape.

## 69.3 Parsing: from tokens to a tree

The **parser** builds the tree, and it must know that `*` binds tighter
than `+`. The classic method is **recursive descent**: one function per
level of priority, each calling the next level for its parts:

| Function | Handles | Example |
|---|---|---|
| `expression` | `+` and `-`, between terms | `a + b - c` |
| `term` | `*` and `/`, between factors | `a * b / c` |
| `power` | `^`, right to left | `2 ^ 3 ^ 2` |
| `factor` | a number, a name, `-x`, or `( expression )` | `3`, `x`, `(a + b)` |

```vyrn
enum Token:
    Num(value: float)
    Name(text: str)
    Op(symbol: str)
end

enum Node:
    Num(value: float)
    Var(name: str)
    Neg(inner: Node)
    BinOp(op: str, left: Node, right: Node)
end

struct SyntaxError:
    message: str
end

object Parser:
    tokens: list[Token]
    pos: int = 0

    create peek(self) -> str:
        if self.pos >= len(self.tokens): return "end"
        if let Token.Op(s) = self.tokens[self.pos]: return s
        return "value"
    end

    create expression(self) -> Node:
        var left = self.term()
        loop self.peek() in ["+", "-"]:
            let op = self.peek()
            self.pos += 1
            left = Node.BinOp(op, left, self.term())
        end
        return left
    end

    create term(self) -> Node:
        var left = self.power()
        loop self.peek() in ["*", "/"]:
            let op = self.peek()
            self.pos += 1
            left = Node.BinOp(op, left, self.power())
        end
        return left
    end

    create power(self) -> Node:
        let base = self.factor()
        if self.peek() == "^":
            self.pos += 1
            return Node.BinOp("^", base, self.power())
        end
        return base
    end

    create factor(self) -> Node:
        if self.pos >= len(self.tokens): fail SyntaxError("the expression ends too early")
        let t = self.tokens[self.pos]
        self.pos += 1
        when t:
            is Token.Num(v): return Node.Num(v)
            is Token.Name(n): return Node.Var(n)
            is Token.Op("-"): return Node.Neg(self.factor())
            is Token.Op("("):
                let inside = self.expression()
                if self.peek() != ")": fail SyntaxError("a '(' is never closed")
                self.pos += 1
                return inside
            is Token.Op(s): fail SyntaxError("'{s}' can't start a value")
        end
    end
end

create show(n: Node) -> str:
    when n:
        is Node.Num(v): return str(v)
        is Node.Var(name): return name
        is Node.Neg(inner): return "-{show(inner)}"
        is Node.BinOp(op, a, b): return "({show(a)} {op} {show(b)})"
    end
end

main:
    let tokens = [Token.Num(1.0), Token.Op("+"), Token.Num(2.0), Token.Op("*"), Token.Name("x"), Token.Op("^"), Token.Num(2.0)]
    print(show(Parser(tokens).expression()))
end
```

```text
(1.0 + (2.0 * (x ^ 2.0)))
```

The tree shows `^` bound tightest, then `*`, then `+`, without a single
priority table: the order of the function calls *is* the priority.

## 69.4 Evaluating

Evaluating is one more recursive `when`: a number is itself, a variable is
looked up, an operation evaluates both sides and combines them. Errors
get their own type so the user sees a clear message:

```vyrn
enum Node:
    Num(value: float)
    Var(name: str)
    Neg(inner: Node)
    BinOp(op: str, left: Node, right: Node)
end

struct MathError:
    message: str
end

create evaluate(n: Node, vars: map[str, float]) -> float:
    when n:
        is Node.Num(v): return v
        is Node.Var(name):
            if name not in vars: fail MathError("'{name}' has no value")
            return vars[name]
        is Node.Neg(inner): return -evaluate(inner, vars)
        is Node.BinOp(op, a, b):
            let x = evaluate(a, vars)
            let y = evaluate(b, vars)
            when op:
                is "+": return x + y
                is "-": return x - y
                is "*": return x * y
                is "/":
                    if y == 0: fail MathError("division by zero")
                    return x / y
                is "^": return x ** y
                else: fail MathError("unknown operator {op}")
            end
    end
end

main:
    let tree = Node.BinOp("*", Node.Num(2.0), Node.BinOp("+", Node.Var("x"), Node.Num(3.5)))
    print(evaluate(tree, {"x": 1.5}))
    attempt:
        evaluate(tree, {})
    catch MathError as e:
        print(e.message)
    end
end
```

```text
10.0
'x' has no value
```

### Exercise

Add a `%` (remainder) operator to `evaluate`: `7 % 4` is `3`, and a
remainder by zero is a `MathError`.

```vyrn starter
enum Node:
    Num(value: float)
    BinOp(op: str, left: Node, right: Node)
end

struct MathError:
    message: str
end

create evaluate(n: Node) -> float:
    when n:
        is Node.Num(v): return v
        is Node.BinOp(op, a, b):
            let x = evaluate(a)
            let y = evaluate(b)
            when op:
                is "+": return x + y
                else: fail MathError("unknown operator {op}")
            end
    end
end

main:
    print(evaluate(Node.BinOp("%", Node.Num(7.0), Node.Num(4.0))))
    attempt:
        evaluate(Node.BinOp("%", Node.Num(7.0), Node.Num(0.0)))
    catch MathError as e:
        print(e.message)
    end
end
```

```text expect
3.0
remainder by zero
```

> Hint: `is "%":` then check `y == 0` before `return x % y`.

<details><summary>Solution</summary>

```vyrn solution
enum Node:
    Num(value: float)
    BinOp(op: str, left: Node, right: Node)
end

struct MathError:
    message: str
end

create evaluate(n: Node) -> float:
    when n:
        is Node.Num(v): return v
        is Node.BinOp(op, a, b):
            let x = evaluate(a)
            let y = evaluate(b)
            when op:
                is "+": return x + y
                is "%":
                    if y == 0: fail MathError("remainder by zero")
                    return x % y
                else: fail MathError("unknown operator {op}")
            end
    end
end

main:
    print(evaluate(Node.BinOp("%", Node.Num(7.0), Node.Num(4.0))))
    attempt:
        evaluate(Node.BinOp("%", Node.Num(7.0), Node.Num(0.0)))
    catch MathError as e:
        print(e.message)
    end
end
```

</details>

## 69.5 The whole calculator

All three stages together, with variables set by `name = expression`
lines, so it can run a little script:

```vyrn
enum Token:
    Num(value: float)
    Name(text: str)
    Op(symbol: str)
end

enum Node:
    Num(value: float)
    Var(name: str)
    Neg(inner: Node)
    BinOp(op: str, left: Node, right: Node)
end

struct SyntaxError:
    message: str
end

struct MathError:
    message: str
end

create tokenize(text: str) -> list[Token]:
    var tokens = []
    var i = 0
    loop i < len(text):
        let c = text[i]
        if c == " ":
            i += 1
        else if c.isdigit() or c == ".":
            var j = i
            loop j < len(text) and (text[j].isdigit() or text[j] == "."): j += 1
            tokens.append(Token.Num(float(text[i:j])))
            i = j
        else if c.isalpha():
            var j = i
            loop j < len(text) and text[j].isalnum(): j += 1
            tokens.append(Token.Name(text[i:j]))
            i = j
        else if c in "+-*/^()":
            tokens.append(Token.Op(c))
            i += 1
        else:
            fail SyntaxError("I don't know the character '{c}'")
        end
    end
    return tokens
end

object Parser:
    tokens: list[Token]
    pos: int = 0

    create peek(self) -> str:
        if self.pos >= len(self.tokens): return "end"
        if let Token.Op(s) = self.tokens[self.pos]: return s
        return "value"
    end

    create parse(self) -> Node:
        let tree = self.expression()
        if self.pos < len(self.tokens): fail SyntaxError("unexpected '{self.peek()}' after the end")
        return tree
    end

    create expression(self) -> Node:
        var left = self.term()
        loop self.peek() in ["+", "-"]:
            let op = self.peek()
            self.pos += 1
            left = Node.BinOp(op, left, self.term())
        end
        return left
    end

    create term(self) -> Node:
        var left = self.power()
        loop self.peek() in ["*", "/"]:
            let op = self.peek()
            self.pos += 1
            left = Node.BinOp(op, left, self.power())
        end
        return left
    end

    create power(self) -> Node:
        let base = self.factor()
        if self.peek() == "^":
            self.pos += 1
            return Node.BinOp("^", base, self.power())
        end
        return base
    end

    create factor(self) -> Node:
        if self.pos >= len(self.tokens): fail SyntaxError("the expression ends too early")
        let t = self.tokens[self.pos]
        self.pos += 1
        when t:
            is Token.Num(v): return Node.Num(v)
            is Token.Name(n): return Node.Var(n)
            is Token.Op("-"): return Node.Neg(self.factor())
            is Token.Op("("):
                let inside = self.expression()
                if self.peek() != ")": fail SyntaxError("a '(' is never closed")
                self.pos += 1
                return inside
            is Token.Op(s): fail SyntaxError("'{s}' can't start a value")
        end
    end
end

create evaluate(n: Node, vars: map[str, float]) -> float:
    when n:
        is Node.Num(v): return v
        is Node.Var(name):
            if name not in vars: fail MathError("'{name}' has no value")
            return vars[name]
        is Node.Neg(inner): return -evaluate(inner, vars)
        is Node.BinOp(op, a, b):
            let x = evaluate(a, vars)
            let y = evaluate(b, vars)
            when op:
                is "+": return x + y
                is "-": return x - y
                is "*": return x * y
                is "/":
                    if y == 0: fail MathError("division by zero")
                    return x / y
                else: return x ** y
            end
    end
end

create run_line(line: str, vars: map[str, float]) -> str:
    attempt:
        var name = none
        var text = line
        if "=" in line:
            let parts = line.split("=", 1)
            name = parts[0].strip()
            text = parts[1]
        end
        let value = evaluate(Parser(tokenize(text)).parse(), vars)
        if name != none:
            vars[name] = value
            return "{name} = {value}"
        end
        return str(value)
    catch SyntaxError as e:
        return "syntax error: {e.message}"
    catch MathError as e:
        return "math error: {e.message}"
    end
end

main:
    var vars = {"pi": 3.14159}
    each line in ["r = 2", "area = pi * r ^ 2", "-(area - 12) / 2", "2 ^ 3 ^ 2", "(1 + 2", "1 / (r - 2)", "speed * 3", "4 $ 5"]:
        print("> {line:<20} {run_line(line, vars)}")
    end
end
```

```text
> r = 2                r = 2.0
> area = pi * r ^ 2    area = 12.56636
> -(area - 12) / 2     -0.28317999999999977
> 2 ^ 3 ^ 2            512.0
> (1 + 2               syntax error: a '(' is never closed
> 1 / (r - 2)          math error: division by zero
> speed * 3            math error: 'speed' has no value
> 4 $ 5                syntax error: I don't know the character '$'
```

`2 ^ 3 ^ 2` is 512 (2 to the 9th), not 64, because `power` calls itself
for its right side, so powers group from the right as in maths. The long
`-0.28317999...` is a float being honest about rounding (Day 63): print
it with `{value:.4f}` for people.

### Exercise

Add functions of one argument: `sqrt(x)` and `abs(x)`. A name followed by
`(` is a call. Write just the new `factor` case and `evaluate` case in this
cut-down version, where the tree has a `Call(name, arg)` variant.

```vyrn starter
enum Node:
    Num(value: float)
    Call(name: str, arg: Node)
end

struct MathError:
    message: str
end

create evaluate(n: Node) -> float:
    when n:
        is Node.Num(v): return v
        is Node.Call(name, arg): fail MathError("no function {name}")
    end
end

main:
    print(evaluate(Node.Call("sqrt", Node.Num(16.0))), evaluate(Node.Call("abs", Node.Num(-3.0))))
    attempt:
        evaluate(Node.Call("cube", Node.Num(2.0)))
    catch MathError as e:
        print(e.message)
    end
end
```

```text expect
4.0 3.0
no function cube
```

> Hint: evaluate the argument first, then `when name:` with `is "sqrt": return x ** 0.5`.

<details><summary>Solution</summary>

```vyrn solution
enum Node:
    Num(value: float)
    Call(name: str, arg: Node)
end

struct MathError:
    message: str
end

create evaluate(n: Node) -> float:
    when n:
        is Node.Num(v): return v
        is Node.Call(name, arg):
            let x = evaluate(arg)
            when name:
                is "sqrt": return x ** 0.5
                is "abs": return abs(x)
                else: fail MathError("no function {name}")
            end
    end
end

main:
    print(evaluate(Node.Call("sqrt", Node.Num(16.0))), evaluate(Node.Call("abs", Node.Num(-3.0))))
    attempt:
        evaluate(Node.Call("cube", Node.Num(2.0)))
    catch MathError as e:
        print(e.message)
    end
end
```

</details>

## Today you built

A working interpreter for arithmetic: a tokenizer, a recursive-descent
parser that gets priorities right, a tree as a recursive enum, an
evaluator, and clear errors at every stage. Every programming language,
Vyrn included, is built from these same three stages. Ideas to take it
further:

- Comparisons (`<`, `==`) and `if ... else ...` expressions.
- Functions with several arguments, or functions the user defines:
  `f(x) = x ^ 2 + 1`.
- Show where an error is with a `^` under the position in the line, as
  `vyrn` does.
