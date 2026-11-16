# Day 71: Graphs, and bits

A **graph** is a set of things and the connections between them: towns and
roads, people and friendships, web pages and links, tasks and what each
depends on. Many problems that look different are the same graph problem
underneath: "can I get from here to there?", "what's the shortest way?",
"in what order must these be done?". Today you store graphs, search them
two ways, and find shortest routes. Then a short tour of **bits**, the
ones and zeros under every number. The project is a route planner.

## 71.1 Storing a graph

The things are **nodes**; the connections are **edges**. The handiest way
to store a graph is an **adjacency map**: each node, with the list of
nodes it connects to:

```vyrn
create add_edge(graph: map, a: str, b: str):
    graph[a] = graph.get(a, []) + [b]
    graph[b] = graph.get(b, []) + [a]
end

main:
    var g = {}
    each a, b in [("Home", "Shop"), ("Home", "Park"), ("Shop", "School"), ("Park", "School"), ("School", "Library")]:
        add_edge(g, a, b)
    end
    each node, near in g.items():
        print("{node:<8} -> {near}")
    end
end
```

```text
Home     -> ["Shop", "Park"]
Shop     -> ["Home", "School"]
Park     -> ["Home", "School"]
School   -> ["Shop", "Park", "Library"]
Library  -> ["School"]
```

Roads go both ways, so `add_edge` adds each edge in both directions. For
one-way connections (a link from page A to page B, task B needing task A),
add only one direction: that's a **directed** graph.

## 71.2 Breadth-first search

To find the way from one node to another with the fewest steps, explore in
**rings**: first every neighbour, then their neighbours, and so on. A
queue (Day 70) keeps the order: nodes are explored in the order they were
found. Remembering where each node was reached *from* lets you trace the
path back:

```vyrn
create add_edge(graph: map, a: str, b: str):
    graph[a] = graph.get(a, []) + [b]
    graph[b] = graph.get(b, []) + [a]
end

create shortest_path(graph: map, start: str, goal: str) -> list?:
    var came_from = {start: none}
    var queue = [start]
    loop queue:
        let node = queue.pop(0)
        if node == goal:
            var path = []
            var at = goal
            loop at != none:
                path.insert(0, at)
                at = came_from[at]
            end
            return path
        end
        each next in graph.get(node, []):
            if next not in came_from:
                came_from[next] = node
                queue.append(next)
            end
        end
    end
    return none
end

main:
    var g = {}
    each a, b in [("Home", "Shop"), ("Home", "Park"), ("Shop", "School"), ("Park", "School"), ("School", "Library"), ("Pond", "Woods")]:
        add_edge(g, a, b)
    end
    print(shortest_path(g, "Home", "Library"))
    print(shortest_path(g, "Library", "Park"))
    print(shortest_path(g, "Home", "Woods"))
end
```

```text
["Home", "Shop", "School", "Library"]
["Library", "School", "Park"]
none
```

`came_from` doubles as the record of what's been seen, so no node is
explored twice and the search can't go round in circles. "Pond" and
"Woods" aren't connected to the rest, so there's no path. This is
**breadth-first search** (BFS): it always finds a path with the fewest
edges.

### Exercise

How many steps away is every place from Home? Write `distances(graph,
start)` with the same BFS, recording each node's distance (its parent's
distance plus one).

```vyrn starter
create distances(graph: map, start: str) -> map:
    return {start: 0}
end

main:
    let g = {"Home": ["Shop", "Park"], "Shop": ["Home", "School"], "Park": ["Home", "School"], "School": ["Shop", "Park", "Library"], "Library": ["School"]}
    print(distances(g, "Home"))
end
```

```text expect
{"Home": 0, "Shop": 1, "Park": 1, "School": 2, "Library": 3}
```

> Hint: start with `var dist = {start: 0}` and `var queue = [start]`.
> Hint: for each unseen neighbour, `dist[next] = dist[node] + 1` and add
> it to the queue.

<details><summary>Solution</summary>

```vyrn solution
create distances(graph: map, start: str) -> map:
    var dist = {start: 0}
    var queue = [start]
    loop queue:
        let node = queue.pop(0)
        each next in graph.get(node, []):
            if next not in dist:
                dist[next] = dist[node] + 1
                queue.append(next)
            end
        end
    end
    return dist
end

main:
    let g = {"Home": ["Shop", "Park"], "Shop": ["Home", "School"], "Park": ["Home", "School"], "School": ["Shop", "Park", "Library"], "Library": ["School"]}
    print(distances(g, "Home"))
end
```

</details>

## 71.3 Depth-first search

**Depth-first search** (DFS) goes as far as it can down one path before
backing up and trying the next, like exploring a maze by keeping one hand
on the wall. It's naturally recursive. It doesn't find shortest paths, but
it's the tool for "what can I reach?" and for ordering tasks that depend
on each other:

```vyrn
## Tasks in an order where each comes after everything it needs.
create task_order(needs: map) -> list[str]:
    var done = []
    var visiting = set()
    create visit(task: str):
        if task in done: return
        if task in visiting: fail ValueError("tasks depend on each other in a circle at {task}")
        visiting.add(task)
        each before in needs.get(task, []): visit(before)
        visiting.discard(task)
        done.append(task)
    end
    each task in needs.keys(): visit(task)
    return done
end

main:
    let recipe = {
        "serve": ["bake", "make icing"],
        "bake": ["mix", "heat oven"],
        "mix": ["buy eggs"],
        "make icing": ["buy sugar"],
    }
    print(task_order(recipe))
    attempt:
        task_order({"a": ["b"], "b": ["a"]})
    catch ValueError as e:
        print(e.message)
    end
end
```

```text
["buy eggs", "mix", "heat oven", "bake", "buy sugar", "make icing", "serve"]
tasks depend on each other in a circle at a
```

This ordering is a **topological sort**. Build tools use it to compile
files in the right order, and package managers (like `vyrn add`) to
install dependencies before what needs them. The `visiting` set catches
circles, where no order is possible.

## 71.4 Shortest routes with distances

Real roads have lengths. The path with the fewest roads isn't always the
shortest drive. **Dijkstra's algorithm** handles this: always continue
from the closest place not yet settled. For a small graph, picking the
closest from a map is fine:

```vyrn
create dijkstra(roads: map, start: str) -> (map, map):
    var dist = {start: 0}
    var came_from = {start: none}
    var settled = set()
    loop len(settled) < len(dist):
        let node = min([n each n in dist.keys() if n not in settled], key = n => dist[n])
        settled.add(node)
        each next, km in roads.get(node, {}).items():
            let through = dist[node] + km
            if next not in dist or through < dist[next]:
                dist[next] = through
                came_from[next] = node
            end
        end
    end
    return (dist, came_from)
end

create route(came_from: map, goal: str) -> list[str]:
    var path = []
    var at = goal
    loop at != none:
        path.insert(0, at)
        at = came_from[at]
    end
    return path
end

main:
    let roads = {
        "A": {"B": 7, "C": 2},
        "B": {"A": 7, "D": 1},
        "C": {"A": 2, "B": 3, "D": 8},
        "D": {"B": 1, "C": 8},
    }
    let dist, came_from = dijkstra(roads, "A")
    print(dist)
    print(route(came_from, "D"), dist["D"], "km")
end
```

```text
{"A": 0, "B": 5, "C": 2, "D": 6}
["A", "C", "B", "D"] 6 km
```

Going A → C → B → D is three roads but 6 km; A → C → D is two roads but
10 km. Maps apps, network routers and games all run versions of this.

## 71.5 Bits

Under the hood every number is **binary**: ones and zeros, called
**bits**. `bin` and `hex` show them; `int(text, 2)` reads them back:

```vyrn
main:
    print(bin(13), hex(255), int("1101", 2), int("ff", 16))
    print(13 & 6, 13 | 6, 13 ^ 6, 1 << 4, 200 >> 3)
end
```

```text
0b1101 0xff 13 255
4 15 11 16 25
```

| Operator | Name | Each bit of the answer is 1 when |
|---|---|---|
| `a & b` | and | both bits are 1 |
| `a \| b` | or | either bit is 1 |
| `a ^ b` | xor | the bits differ |
| `a << n` | shift left | (moves bits up: multiplies by 2ⁿ) |
| `a >> n` | shift right | (moves bits down: divides by 2ⁿ) |

Their everyday use is **flags**: several yes/no settings packed into one
number, one bit each. Files' permissions, game controller buttons and
network packets all work this way:

```vyrn
const READ = 1 << 0
const WRITE = 1 << 1
const SHARE = 1 << 2

create describe(perms: int) -> str:
    var names = []
    each flag, name in [(READ, "read"), (WRITE, "write"), (SHARE, "share")]:
        if perms & flag != 0: names.append(name)
    end
    return ", ".join(names) or "nothing"
end

main:
    var p = READ | WRITE
    print(p, bin(p), describe(p))
    p = p | SHARE
    p = p & ~WRITE
    print(p, bin(p), describe(p))
end
```

```text
3 0b11 read, write
5 0b101 read, share
```

`| flag` turns a bit on, `& ~flag` turns it off, `& flag != 0` tests it.

### Exercise

A colour is often stored as one number, `0xRRGGBB`. Write `channels(c)`
giving the red, green and blue parts by shifting and masking with `0xff`.

```vyrn starter
create channels(c: int) -> (int, int, int):
    return (0, 0, 0)
end

main:
    print(channels(0x1e88e5), channels(0xff0000))
end
```

```text expect
(30, 136, 229) (255, 0, 0)
```

> Hint: red is `(c >> 16) & 0xff`, green `(c >> 8) & 0xff`, blue `c & 0xff`.

<details><summary>Solution</summary>

```vyrn solution
create channels(c: int) -> (int, int, int):
    return ((c >> 16) & 0xff, (c >> 8) & 0xff, c & 0xff)
end

main:
    print(channels(0x1e88e5), channels(0xff0000))
end
```

</details>

## 71.6 Project: a route planner

A small rail network with travel times in minutes. The planner gives the
fastest route between any two stations, with the time of each leg:

```vyrn
const LINES = [
    ("Kings Cross", "Euston", 4), ("Euston", "Warren Street", 2), ("Warren Street", "Oxford Circus", 2),
    ("Kings Cross", "Angel", 3), ("Angel", "Old Street", 3), ("Old Street", "Bank", 4),
    ("Oxford Circus", "Bond Street", 2), ("Oxford Circus", "Tottenham Court Road", 2),
    ("Tottenham Court Road", "Bank", 9), ("Kings Cross", "Farringdon", 4), ("Farringdon", "Bank", 6),
]

create network() -> map:
    var roads = {}
    each a, b, minutes in LINES:
        if a not in roads: roads[a] = {}
        if b not in roads: roads[b] = {}
        roads[a][b] = minutes
        roads[b][a] = minutes
    end
    return roads
end

create plan(roads: map, start: str, goal: str) -> list?:
    var dist = {start: 0}
    var came_from = {start: none}
    var settled = set()
    loop len(settled) < len(dist):
        let node = min([n each n in dist.keys() if n not in settled], key = n => dist[n])
        if node == goal: stop
        settled.add(node)
        each next, minutes in roads[node].items():
            if next not in dist or dist[node] + minutes < dist[next]:
                dist[next] = dist[node] + minutes
                came_from[next] = node
            end
        end
    end
    if goal not in dist: return none
    var legs = []
    var at = goal
    loop came_from[at] != none:
        let before = came_from[at]
        legs.insert(0, (before, at, roads[before][at]))
        at = before
    end
    return legs
end

main:
    let roads = network()
    each start, goal in [("Kings Cross", "Bank"), ("Bond Street", "Angel")]:
        let legs = plan(roads, start, goal)
        print("{start} to {goal}: {sum(m each _, _, m in legs)} minutes")
        each a, b, m in legs:
            print("  {a} -> {b} ({m} min)")
        end
    end
end
```

```text
Kings Cross to Bank: 10 minutes
  Kings Cross -> Farringdon (4 min)
  Farringdon -> Bank (6 min)
Bond Street to Angel: 13 minutes
  Bond Street -> Oxford Circus (2 min)
  Oxford Circus -> Warren Street (2 min)
  Warren Street -> Euston (2 min)
  Euston -> Kings Cross (4 min)
  Kings Cross -> Angel (3 min)
```

The planner stops as soon as the goal is settled: its distance can't get
any shorter after that, so there's no need to explore the rest.

### Exercise

Write `legs_text(legs)`
that joins the stations of a route with arrows, like
`"Kings Cross → Angel → Old Street → Bank"`.

```vyrn starter
create legs_text(legs: list) -> str:
    return ""
end

main:
    print(legs_text([("Kings Cross", "Angel", 3), ("Angel", "Old Street", 3), ("Old Street", "Bank", 4)]))
end
```

```text expect
Kings Cross → Angel → Old Street → Bank
```

> Hint: the stations are the first leg's start, then every leg's end.

<details><summary>Solution</summary>

```vyrn solution
create legs_text(legs: list) -> str:
    let stations = [legs[0][0]] + [b each _, b, _ in legs]
    return " → ".join(stations)
end

main:
    print(legs_text([("Kings Cross", "Angel", 3), ("Angel", "Old Street", 3), ("Old Street", "Bank", 4)]))
end
```

</details>

## Today you learned

- Graphs are nodes and edges; store them as a map from each node to its
  neighbours.
- Breadth-first search finds the fewest steps; depth-first search explores
  and orders tasks (topological sort), catching circles.
- Dijkstra's algorithm finds the shortest route when edges have lengths.
- Numbers are bits: `&`, `|`, `^`, `~`, `<<`, `>>`, and flags packed into
  one number.
