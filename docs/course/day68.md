# Day 68: Algorithms

An **algorithm** is a recipe for solving a problem. There's usually more
than one, and some are vastly faster than others: searching a million
names one by one takes up to a million steps, but the right method needs
twenty. Today you learn two classic searches and three sorts, how to
measure an algorithm by **counting its steps**, and the notation
programmers use to compare them. Recursion, from Day 10, does a lot of the
work. The project races the algorithms against each other.

## 68.1 Counting steps

Timing a program with a clock depends on the computer, and on what else it
is doing. A fairer measure is to count the basic steps an algorithm takes
for an input of size *n*, and see how that count **grows** as *n* grows:

```vyrn
create linear_search(xs: list[int], target: int) -> (int, int):
    var steps = 0
    each i, x in enumerate(xs):
        steps += 1
        if x == target: return (i, steps)
    end
    return (-1, steps)
end

main:
    each n in [10, 1000, 100000]:
        let xs = [i * 2 each i in 0 until n]
        let found, steps = linear_search(xs, -1)
        print("n = {n:,}: {steps:,} steps")
    end
end
```

```text
n = 10: 10 steps
n = 1,000: 1,000 steps
n = 100,000: 100,000 steps
```

Looking for something that isn't there checks every item: the steps grow
in step with *n*. We say linear search is **O(n)** ("order n"): double the
data, double the work.

## 68.2 Binary search

If the list is **sorted**, there's a much better way: look at the middle.
If the target is smaller, it can only be in the left half; if bigger, the
right half. Each step throws away half of what's left, like looking up a
word in a dictionary:

```vyrn
create binary_search(xs: list[int], target: int) -> (int, int):
    var low = 0
    var high = len(xs) - 1
    var steps = 0
    loop low <= high:
        steps += 1
        let mid = (low + high) // 2
        if xs[mid] == target: return (mid, steps)
        if xs[mid] < target:
            low = mid + 1
        else:
            high = mid - 1
        end
    end
    return (-1, steps)
end

main:
    each n in [10, 1000, 100000, 10000000]:
        let xs = 0 until n
        let found, steps = binary_search(list(xs), n - 1)
        print("n = {n:,}: found at {found:,} in {steps} steps")
    end
end
```

```text
n = 10: found at 9 in 4 steps
n = 1,000: found at 999 in 10 steps
n = 100,000: found at 99,999 in 17 steps
n = 10,000,000: found at 9,999,999 in 24 steps
```

Ten million items, 24 steps. Halving again and again is a **logarithm**,
so binary search is **O(log n)**: multiply the data by a thousand and it
takes only about ten more steps.

### Exercise

Write binary search recursively: `find(xs, target, low, high)` checks the
middle, then calls itself on the half that could hold the target. Give the
position, or -1.

```vyrn starter
create find(xs: list[int], target: int, low: int, high: int) -> int:
    return -1
end

main:
    let xs = [2, 3, 5, 7, 11, 13, 17]
    print(find(xs, 11, 0, len(xs) - 1), find(xs, 2, 0, len(xs) - 1), find(xs, 4, 0, len(xs) - 1))
end
```

```text expect
4 0 -1
```

> Hint: if `low > high` there's nothing left: return -1.
> Hint: otherwise compare `xs[mid]`, and `return find(xs, target, mid + 1, high)`
> or `find(xs, target, low, mid - 1)`.

<details><summary>Solution</summary>

```vyrn solution
create find(xs: list[int], target: int, low: int, high: int) -> int:
    if low > high: return -1
    let mid = (low + high) // 2
    if xs[mid] == target: return mid
    if xs[mid] < target: return find(xs, target, mid + 1, high)
    return find(xs, target, low, mid - 1)
end

main:
    let xs = [2, 3, 5, 7, 11, 13, 17]
    print(find(xs, 11, 0, len(xs) - 1), find(xs, 2, 0, len(xs) - 1), find(xs, 4, 0, len(xs) - 1))
end
```

</details>

## 68.3 Big-O

**Big-O notation** describes how the work grows, ignoring small details:

| Big-O | Name | n = 1,000 means about | Example |
|---|---|---|---|
| O(1) | constant | 1 step | `xs[5]`, `key in some_map` |
| O(log n) | logarithmic | 10 steps | binary search |
| O(n) | linear | 1,000 steps | linear search, `sum(xs)`, `x in some_list` |
| O(n log n) | "n log n" | 10,000 steps | good sorting: merge sort, `sorted` |
| O(n²) | quadratic | 1,000,000 steps | comparing every pair, bubble sort |
| O(2ⁿ) | exponential | more than atoms in the universe | trying every subset; naive Fibonacci |

The table explains a piece of advice you've seen before: `x in some_map`
and `x in some_set` are O(1) because they find the item by its key, while
`x in some_list` is O(n) because it checks each item. In a loop over
another big list, that's the difference between fast and unusable.

## 68.4 Sorting

`sorted` is built in and fast, but how sorting works is a classic way to
learn algorithm design. **Bubble sort** repeatedly swaps neighbours that
are in the wrong order. It's simple and O(n²):

```vyrn
create bubble_sort(xs: list[int]) -> (list[int], int):
    var a = xs.copy()
    var comparisons = 0
    each end_at in reversed(1 until len(a)):
        each i in 0 until end_at:
            comparisons += 1
            if a[i] > a[i + 1]: a[i], a[i + 1] = a[i + 1], a[i]
        end
    end
    return (a, comparisons)
end

main:
    print(bubble_sort([5, 2, 9, 1, 7]))
end
```

```text
([1, 2, 5, 7, 9], 10)
```

**Merge sort** uses recursion. A list of one item is already sorted. For a
longer list: sort each half (recursively), then **merge** the two sorted
halves by repeatedly taking the smaller front item:

```vyrn
create merge(a: list[int], b: list[int]) -> list[int]:
    var out = []
    var i = 0
    var j = 0
    loop i < len(a) and j < len(b):
        if a[i] <= b[j]:
            out.append(a[i])
            i += 1
        else:
            out.append(b[j])
            j += 1
        end
    end
    return out + a[i:] + b[j:]
end

create merge_sort(xs: list[int]) -> list[int]:
    if len(xs) <= 1: return xs
    let mid = len(xs) // 2
    return merge(merge_sort(xs[:mid]), merge_sort(xs[mid:]))
end

main:
    print(merge([1, 4, 9], [2, 3, 10]))
    print(merge_sort([38, 27, 43, 3, 9, 82, 10]))
end
```

```text
[1, 2, 3, 4, 9, 10]
[3, 9, 10, 27, 38, 43, 82]
```

The list is halved about log n times, and each level of halves takes n
steps to merge: O(n log n). This **divide and conquer** pattern (split,
solve the parts recursively, combine) solves many problems.

### Exercise

**Quicksort** is another divide-and-conquer sort: pick the first item as
the **pivot**, split the rest into those smaller and those not smaller,
sort both parts recursively, and put them together around the pivot.

```vyrn starter
create quicksort(xs: list[int]) -> list[int]:
    return xs
end

main:
    print(quicksort([3, 6, 1, 8, 2, 9, 2]))
end
```

```text expect
[1, 2, 2, 3, 6, 8, 9]
```

> Hint: if there are fewer than two items, return them.
> Hint: `smaller = [x each x in xs[1:] if x < pivot]`, `rest = [x each x in xs[1:] if x >= pivot]`,
> then `quicksort(smaller) + [pivot] + quicksort(rest)`.

<details><summary>Solution</summary>

```vyrn solution
create quicksort(xs: list[int]) -> list[int]:
    if len(xs) < 2: return xs
    let pivot = xs[0]
    let smaller = [x each x in xs[1:] if x < pivot]
    let rest = [x each x in xs[1:] if x >= pivot]
    return quicksort(smaller) + [pivot] + quicksort(rest)
end

main:
    print(quicksort([3, 6, 1, 8, 2, 9, 2]))
end
```

</details>

## 68.5 Remembering answers: dynamic programming

Some problems break into smaller versions of themselves that overlap. How
many ways can you make 50p from 1p, 2p, 5p, 10p and 20p coins? A
recursive answer: either use the first coin (and make the rest of the
amount) or never use it (and make the amount from the other coins).
The same sub-questions come up again and again, so remember them, as
`@memo` did on Day 64:

```vyrn
create ways(amount: int, coins: list[int]) -> int:
    var saved = {}
    create count(left: int, first: int) -> int:
        if left == 0: return 1
        if left < 0 or first == len(coins): return 0
        let key = (left, first)
        if key not in saved:
            saved[key] = count(left - coins[first], first) + count(left, first + 1)
        end
        return saved[key]
    end
    return count(amount, 0)
end

main:
    print(ways(5, [1, 2, 5]), ways(50, [1, 2, 5, 10, 20]), ways(500, [1, 2, 5, 10, 20, 50, 100, 200]))
end
```

```text
4 450 6295434
```

Six million ways, worked out instantly, because each `(left, first)` pair
is solved once. Without `saved`, the last one would take hours. Solving
overlapping sub-problems once and storing the answers is called **dynamic
programming**.

## 68.6 Project: the race

Count comparisons, not seconds, so the race is fair and repeatable.
Bubble sort against merge sort on growing lists:

```vyrn
use vyrn.random

create bubble_count(xs: list[int]) -> int:
    var a = xs.copy()
    var count = 0
    each end_at in reversed(1 until len(a)):
        each i in 0 until end_at:
            count += 1
            if a[i] > a[i + 1]: a[i], a[i + 1] = a[i + 1], a[i]
        end
    end
    return count
end

create merge_count(xs: list[int]) -> (list[int], int):
    if len(xs) <= 1: return (xs, 0)
    let mid = len(xs) // 2
    let left, c1 = merge_count(xs[:mid])
    let right, c2 = merge_count(xs[mid:])
    var out = []
    var i = 0
    var j = 0
    var count = c1 + c2
    loop i < len(left) and j < len(right):
        count += 1
        if left[i] <= right[j]:
            out.append(left[i])
            i += 1
        else:
            out.append(right[j])
            j += 1
        end
    end
    return (out + left[i:] + right[j:], count)
end

main:
    random.seed(1)
    print("     n    bubble     merge")
    each n in [10, 100, 1000, 2000]:
        let xs = [random.int(0, 1000) each _ in 0 until n]
        let _, m = merge_count(xs)
        print("{n:>6} {bubble_count(xs):>9,} {m:>9,}")
    end
end
```

```text
     n    bubble     merge
    10        45        23
   100     4,950       537
  1000   499,500     8,701
  2000 1,999,000    19,421
```

Doubling *n* from 1,000 to 2,000 quadruples bubble sort's work (n²) but
only a little more than doubles merge sort's (n log n). At a million items,
bubble sort would need half a million million comparisons; merge sort,
about twenty million.

### Exercise

Two loops over the same list are O(n) each; a loop *inside* a loop is
O(n²). This function finds whether any two numbers add up to a target,
with nested loops. Rewrite it to be O(n) with a set: for each number, check
whether the number it needs has already been seen.

```vyrn starter
create has_pair(xs: list[int], target: int) -> bool:
    each i in 0 until len(xs):
        each j in i + 1 until len(xs):
            if xs[i] + xs[j] == target: return true
        end
    end
    return false
end

main:
    print(has_pair([8, 3, 5, 11], 16), has_pair([8, 3, 5, 11], 9), has_pair([], 1))
end
```

```text expect
true false false
```

> Hint: `var seen = set()`; for each `x`, if `target - x in seen` you've
> found a pair; then `seen.add(x)`.

<details><summary>Solution</summary>

```vyrn solution
create has_pair(xs: list[int], target: int) -> bool:
    var seen = set()
    each x in xs:
        if target - x in seen: return true
        seen.add(x)
    end
    return false
end

main:
    print(has_pair([8, 3, 5, 11], 16), has_pair([8, 3, 5, 11], 9), has_pair([], 1))
end
```

</details>

## Today you learned

- Measure algorithms by how their step count grows with the input size.
- Linear search is O(n); binary search on sorted data is O(log n).
- Big-O: O(1), O(log n), O(n), O(n log n), O(n²), O(2ⁿ). Map and set
  lookups are O(1); `in` on a list is O(n).
- Bubble sort is O(n²); merge sort and quicksort divide and conquer in
  O(n log n).
- Dynamic programming solves overlapping sub-problems once and remembers
  them.
