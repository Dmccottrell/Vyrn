# Day 74: Design patterns

Programmers keep meeting the same problems: "the program must work with
several kinds of payment", "many parts must react when something
happens", "this object is complicated to build". The well-known solutions
have names, **design patterns**, so people can say "use an observer here"
and be understood. Today you meet five of the most useful, each solving a
real problem, in Vyrn's style: traits, closures and maps often make them
shorter than in Java, where most were first described. The project is a
small plugin system that uses several at once.

## 74.1 Strategy: swap the method

**Problem:** a shop works out shipping differently for standard, express
and collection. A chain of `if`s grows with every new option.
**Pattern:** make each method an object (or function) with the same shape,
and choose one:

```vyrn
trait Shipping:
    create cost(self, weight_kg: float) -> float
    create name(self) -> str
end

struct Standard:
end

struct Express:
    surcharge: float
end

struct Collect:
end

impl Shipping for Standard:
    create cost(self, weight_kg: float) -> float:
        return 3.0 + 0.5 * weight_kg
    end
    create name(self) -> str:
        return "standard"
    end
end

impl Shipping for Express:
    create cost(self, weight_kg: float) -> float:
        return 3.0 + 0.5 * weight_kg + self.surcharge
    end
    create name(self) -> str:
        return "express"
    end
end

impl Shipping for Collect:
    create cost(self, weight_kg: float) -> float:
        return 0.0
    end
    create name(self) -> str:
        return "collect in store"
    end
end

create checkout(total: float, weight: float, shipping: Shipping) -> str:
    return "£{total + shipping.cost(weight):.2f} with {shipping.name()}"
end

main:
    each s in [Standard(), Express(7.5), Collect()]:
        print(checkout(20.0, 4.0, s))
    end
end
```

```text
£25.00 with standard
£32.50 with express
£20.00 with collect in store
```

`checkout` never changes when a shipping method is added. When the
strategy is just one function, a function value is enough: `sorted(xs,
key = ...)` takes a strategy for comparing.

## 74.2 Factory: build the right thing

**Problem:** code receives `"express"` from a form and must make the right
object. **Pattern:** one function, the **factory**, knows how to turn a
name into an object, so the rest of the program never does:

```vyrn
struct Standard:
end

struct Express:
    surcharge: float
end

create shipping_from(name: str) -> any:
    let makers = {
        "standard": () => Standard(),
        "express": () => Express(7.5),
    }
    if name not in makers: fail ValueError("no shipping called {name}; choose from {sorted(makers.keys())}")
    return makers[name]()
end

main:
    print(shipping_from("express"))
    attempt:
        shipping_from("drone")
    catch ValueError as e:
        print(e.message)
    end
end
```

```text
Express(surcharge=7.5)
no shipping called drone; choose from ["express", "standard"]
```

A map from names to functions is the factory's whole table; adding a
method is one line, and the error message lists the choices for free.

## 74.3 Observer: tell everyone who's interested

**Problem:** when an order is placed, the stock must go down, an email must
be sent and a counter must go up, and you don't want the order code to
know about all of them. **Pattern:** the order code **publishes** an
event; anything interested **subscribes** a function to it:

```vyrn
object Events:
    handlers: map = {}

    create on(self, name: str, handler: any):
        self.handlers[name] = self.handlers.get(name, []) + [handler]
    end
    create emit(self, name: str, data: any):
        each h in self.handlers.get(name, []):
            h(data)
        end
    end
end

main:
    let events = Events()
    var stock = {"tea": 10}
    var emails = []
    create take_stock(o: map):
        stock[o["item"]] -= o["qty"]
    end
    events.on("order", take_stock)
    events.on("order", o => emails.append("Thanks for ordering {o["item"]}!"))
    events.emit("order", {"item": "tea", "qty": 3})
    events.emit("refund", {"item": "tea"})
    print(stock, emails)
end
```

```text
{"tea": 7} ["Thanks for ordering tea!"]
```

Nobody listens for "refund" yet, so emitting it does nothing, and that's
fine. (A lambda holds one expression, so the handler that changes `stock`
is a small named function.) Buttons in a GUI, webhooks, and the middleware you wrote on Day 49
are all observers.

### Exercise

Add `once(name, handler)` to `Events`: a handler that runs the first time
the event happens, and never again.

```vyrn starter
object Events:
    handlers: map = {}

    create on(self, name: str, handler: any):
        self.handlers[name] = self.handlers.get(name, []) + [handler]
    end
    create emit(self, name: str, data: any):
        each h in self.handlers.get(name, []):
            h(data)
        end
    end
end

main:
    let e = Events()
    e.on("tick", n => print("every", n))
    e.emit("tick", 1)
    e.emit("tick", 2)
end
```

```text expect
every 1
first 1
every 2
```

> Hint: in `once`, wrap the handler in a closure with `var done = false`
> that does nothing once `done` is true.
> Hint: in `main`, add `e.once("tick", n => print("first", n))` after the
> `on` line.

<details><summary>Solution</summary>

```vyrn solution
object Events:
    handlers: map = {}

    create on(self, name: str, handler: any):
        self.handlers[name] = self.handlers.get(name, []) + [handler]
    end
    create once(self, name: str, handler: any):
        var done = false
        create wrapper(data):
            if done: return
            done = true
            handler(data)
        end
        self.on(name, wrapper)
    end
    create emit(self, name: str, data: any):
        each h in self.handlers.get(name, []):
            h(data)
        end
    end
end

main:
    let e = Events()
    e.on("tick", n => print("every", n))
    e.once("tick", n => print("first", n))
    e.emit("tick", 1)
    e.emit("tick", 2)
end
```

</details>

## 74.4 Decorator: add behaviour around something

**Problem:** some shipping costs need a weekend surcharge, some a
discount, some both, and making a type for every combination explodes.
**Pattern:** wrap an object in another with the same shape that adds one
thing; wrappers stack. (Vyrn's `@decorator` for functions, Day 64, is the
same idea.)

```vyrn
trait Price:
    create amount(self) -> float
    create label(self) -> str
end

struct Base:
    value: float
    what: str
end

struct Discount:
    inner: Price
    percent: float
end

struct Surcharge:
    inner: Price
    extra: float
end

impl Price for Base:
    create amount(self) -> float:
        return self.value
    end
    create label(self) -> str:
        return self.what
    end
end

impl Price for Discount:
    create amount(self) -> float:
        return self.inner.amount() * (1 - self.percent / 100)
    end
    create label(self) -> str:
        return "{self.inner.label()}, {self.percent}% off"
    end
end

impl Price for Surcharge:
    create amount(self) -> float:
        return self.inner.amount() + self.extra
    end
    create label(self) -> str:
        return "{self.inner.label()}, +£{self.extra:.2f}"
    end
end

main:
    let p = Surcharge(Discount(Base(40.0, "concert ticket"), 25), 2.5)
    print("{p.label()}: £{p.amount():.2f}")
end
```

```text
concert ticket, 25% off, +£2.50: £32.50
```

## 74.5 Singleton, and why to avoid it

**Problem:** there must be exactly one of something, such as the settings
or the database connection. The classic **singleton** pattern hides it in
a global that everything reaches for. Vyrn has no global variables, on
purpose: code that secretly reaches for shared state is hard to test
(you can't give it a fake) and hard to follow. Instead, make the one
object in `main` and **pass it in**, to functions as an argument or to
routes with `http.provide`. That is what every project in this course has
done, and why fakes could replace databases and email so easily. This
approach has a name too: **dependency injection**.

## 74.6 Project: a plugin system

A text tool whose features are plugins: each is registered by name
(factory), transforms the text (strategy), and announces what it did
(observer). New plugins need no change to the tool:

```vyrn
trait Plugin:
    create apply(self, text: str) -> str
end

struct Upper:
end

struct Replace:
    old: str
    new: str
end

struct Trim:
end

impl Plugin for Upper:
    create apply(self, text: str) -> str:
        return text.upper()
    end
end

impl Plugin for Replace:
    create apply(self, text: str) -> str:
        return text.replace(self.old, self.new)
    end
end

impl Plugin for Trim:
    create apply(self, text: str) -> str:
        return " ".join(text.split())
    end
end

object Tool:
    makers: map = {}
    listeners: list = []

    create register(self, name: str, maker: any):
        self.makers[name] = maker
    end
    create on_step(self, listener: any):
        self.listeners.append(listener)
    end
    create run(self, text: str, steps: list[str]) -> str:
        var result = text
        each step in steps:
            let parts = step.split(":")
            if parts[0] not in self.makers: fail ValueError("no plugin {parts[0]}")
            let before = result
            result = self.makers[parts[0]](parts[1:]).apply(result)
            each l in self.listeners: l(parts[0], before, result)
        end
        return result
    end
end

main:
    let tool = Tool()
    tool.register("upper", args => Upper())
    tool.register("trim", args => Trim())
    tool.register("replace", args => Replace(args[0], args[1]))
    var log = []
    tool.on_step((name, before, after) => log.append("{name}: {len(before)} -> {len(after)} chars"))
    print(tool.run("  hello   wide   world  ", ["trim", "replace:world:Vyrn", "upper"]))
    each line in log: print(" ", line)
end
```

```text
HELLO WIDE VYRN
  trim: 24 -> 16 chars
  replace: 16 -> 15 chars
  upper: 15 -> 15 chars
```

Adding a plugin is one `struct`, one `impl` and one `register` line.

### Exercise

Add a `reverse` plugin that reverses the text, register it, and run
`["reverse", "upper"]` on `"abc"`.

```vyrn starter
trait Plugin:
    create apply(self, text: str) -> str
end

main:
    print("nothing yet")
end
```

```text expect
CBA
```

> Hint: `struct Reverse:` with `impl Plugin for Reverse:` whose `apply`
> returns `text[::-1]`.
> Hint: no tool needed for the exercise: `Upper().apply(Reverse().apply("abc"))`
> works, given both plugins.

<details><summary>Solution</summary>

```vyrn solution
trait Plugin:
    create apply(self, text: str) -> str
end

struct Reverse:
end

struct Upper:
end

impl Plugin for Reverse:
    create apply(self, text: str) -> str:
        return text[::-1]
    end
end

impl Plugin for Upper:
    create apply(self, text: str) -> str:
        return text.upper()
    end
end

main:
    var text = "abc"
    each p in [Reverse(), Upper()]:
        text = p.apply(text)
    end
    print(text)
end
```

</details>

## Today you learned

- Strategy: interchangeable ways of doing one job, behind a trait or a
  function value.
- Factory: one place that turns a name into the right object.
- Observer: publish events; subscribers react without the publisher
  knowing them.
- Decorator: wrap an object to add behaviour, keeping its shape.
- Prefer passing shared objects in (dependency injection) to singletons.
