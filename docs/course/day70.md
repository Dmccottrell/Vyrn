# Day 70: Stacks, queues, lists and trees

Lists and maps cover most needs, but some problems are much simpler (or
much faster) with a structure built for them. Today you build four
classic **data structures**: stacks, queues, linked lists and binary
search trees. Each is an object with a few methods, and with Day 63's
`len`, `contains` and `iter` they feel built in. The project adds undo to
a text editor and builds a word index.

## 70.1 Stacks: last in, first out

A **stack** is like a pile of plates: you add to the top and take from the
top. The last thing in is the first out. A list does it, with `append`
and `pop`:

```vyrn
object Stack[T]:
    items: list[T] = []

    create push(self, item: T):
        self.items.append(item)
    end
    create pop(self) -> T:
        if not self.items: fail IndexError("the stack is empty")
        return self.items.pop()
    end
    create peek(self) -> T?:
        return self.items[-1] if self.items else none
    end
    create len(self) -> int:
        return len(self.items)
    end
end

main:
    let s = Stack()
    each x in ["a", "b", "c"]: s.push(x)
    print(s.pop(), s.pop(), s.peek(), len(s))
end
```

```text
c b a 1
```

Stacks are everywhere: the "back" button, undo, and the calls your program
is in the middle of (the trace in an error message is the call stack).
A classic use, checking that brackets match:

```vyrn
create balanced(text: str) -> bool:
    let pairs = {")": "(", "]": "[", "}": "\{"}
    var stack = []
    each c in text:
        if c in "([\{":
            stack.append(c)
        else if c in pairs:
            if not stack or stack.pop() != pairs[c]: return false
        end
    end
    return not stack
end

main:
    each t in ["(a[b]c)", "(a[b)c]", "((", "f(x) = [1, 2]"]:
        print(t, balanced(t))
    end
end
```

```text
(a[b]c) true
(a[b)c] false
(( false
f(x) = [1, 2] true
```

## 70.2 Queues: first in, first out

A **queue** is a line at a shop: join at the back, leave from the front.
Taking from the front of a list (`pop(0)`) moves every other item along,
which is slow for long lists. A queue that keeps a moving start index
avoids that:

```vyrn
object Queue[T]:
    items: list[T] = []
    start: int = 0

    create add(self, item: T):
        self.items.append(item)
    end
    create take(self) -> T:
        if self.start == len(self.items): fail IndexError("the queue is empty")
        let item = self.items[self.start]
        self.start += 1
        if self.start > 64 and self.start * 2 > len(self.items):
            self.items = self.items[self.start:]
            self.start = 0
        end
        return item
    end
    create len(self) -> int:
        return len(self.items) - self.start
    end
end

main:
    let q = Queue()
    each name in ["Ada", "Alan", "Grace"]: q.add(name)
    print(q.take(), q.take(), len(q))
    q.add("Mo")
    print(q.take(), q.take(), len(q))
end
```

```text
Ada Alan 1
Grace Mo 0
```

Every so often it drops the items already taken, so it doesn't grow
forever. Queues hold work waiting to be done: print jobs, messages,
requests, and (Day 71) places still to explore.

### Exercise

A printer queue: jobs arrive as `(name, pages)`; the printer prints 10
pages a minute. Using a queue, print when each job finishes.

```vyrn starter
main:
    let jobs = [("report", 25), ("photo", 2), ("thesis", 120)]
end
```

```text expect
report done after 2.5 minutes
photo done after 2.7 minutes
thesis done after 14.7 minutes
```

> Hint: a list used as a queue is fine for three jobs: `jobs.pop(0)` takes
> the first.
> Hint: keep a running total of minutes: `pages / 10` each.

<details><summary>Solution</summary>

```vyrn solution
main:
    var jobs = [("report", 25), ("photo", 2), ("thesis", 120)]
    var minutes = 0.0
    loop jobs:
        let name, pages = jobs.pop(0)
        minutes += pages / 10
        print("{name} done after {minutes:.1f} minutes")
    end
end
```

</details>

## 70.3 Linked lists

A **linked list** stores each item in a node that points to the next.
Adding at the front is instant, whatever the length, because nothing
moves; finding the 500th item means walking 500 steps. It's the simplest
structure built from objects pointing at objects:

```vyrn
object Node:
    value: any
    next: Node? = none
end

object LinkedList:
    head: Node? = none
    size: int = 0

    create push_front(self, value):
        self.head = Node(value, self.head)
        self.size += 1
    end
    create iter(self):
        var node = self.head
        loop node != none:
            yield node.value
            node = node.next
        end
    end
    create len(self) -> int:
        return self.size
    end
    create reverse(self):
        var previous = none
        var node = self.head
        loop node != none:
            let next = node.next
            node.next = previous
            previous = node
            node = next
        end
        self.head = previous
    end
end

main:
    let l = LinkedList()
    each x in [3, 2, 1]: l.push_front(x)
    print([x each x in l], len(l))
    l.reverse()
    print([x each x in l])
end
```

```text
[1, 2, 3] 3
[3, 2, 1]
```

Reversing a linked list in place, by turning each arrow around, is a
classic interview question: draw the nodes on paper and follow the three
variables through the loop.

## 70.4 Binary search trees

A **binary search tree** keeps items sorted as they're added. Each node
has a value, a left branch with everything smaller, and a right branch
with everything bigger. Finding a value means going left or right at each
node, like binary search (Day 68), so a balanced tree of a million items
needs about 20 steps:

```text
          8
        /   \
       3     10
      / \      \
     1   6      14
        / \    /
       4   7  13
```

```vyrn
object TreeNode:
    value: int
    left: TreeNode? = none
    right: TreeNode? = none
end

object Tree:
    root: TreeNode? = none
    size: int = 0

    create add(self, value: int):
        if self.root == none:
            self.root = TreeNode(value)
        else:
            insert(self.root, value)
        end
        self.size += 1
    end
    create contains(self, value: int) -> bool:
        var node = self.root
        loop node != none:
            if value == node.value: return true
            node = node.left if value < node.value else node.right
        end
        return false
    end
    create iter(self):
        each v in in_order(self.root): yield v
    end
    create len(self) -> int:
        return self.size
    end
end

create insert(node: TreeNode, value: int):
    if value < node.value:
        if node.left == none:
            node.left = TreeNode(value)
        else:
            insert(node.left, value)
        end
    else:
        if node.right == none:
            node.right = TreeNode(value)
        else:
            insert(node.right, value)
        end
    end
end

## Left branch, then the node, then the right branch: sorted order.
create in_order(node: TreeNode?):
    if node == none: return
    each v in in_order(node.left): yield v
    yield node.value
    each v in in_order(node.right): yield v
end

create height(node: TreeNode?) -> int:
    if node == none: return 0
    return 1 + max(height(node.left), height(node.right))
end

main:
    let t = Tree()
    each x in [8, 3, 10, 1, 6, 14, 4, 7, 13]: t.add(x)
    print([x each x in t], len(t))
    print(6 in t, 5 in t, height(t.root))
    let sorted_in = Tree()
    each x in 1 to 9: sorted_in.add(x)
    print(height(sorted_in.root))
end
```

```text
[1, 3, 4, 6, 7, 8, 10, 13, 14] 9
true false 4
9
```

The last line shows the catch: adding items that are already sorted gives
a "tree" that is just a long line, height 9 for 9 items, as slow as a
list. Real libraries use **self-balancing** trees that reorganise
themselves to stay short. In Vyrn, a map is usually the right tool for
"find by key"; a tree when you also need the items in order.

### Exercise

Add a method `smallest(self)` giving the smallest value in the tree (keep
going left), or `none` for an empty tree.

```vyrn starter
object TreeNode:
    value: int
    left: TreeNode? = none
    right: TreeNode? = none
end

object Tree:
    root: TreeNode? = none

    create smallest(self) -> int?:
        return none
    end
end

main:
    let t = Tree(TreeNode(8, TreeNode(3, TreeNode(1)), TreeNode(10)))
    print(t.smallest(), Tree().smallest())
end
```

```text expect
1 none
```

> Hint: `if self.root == none: return none`, then follow `.left` while it
> isn't `none`.

<details><summary>Solution</summary>

```vyrn solution
object TreeNode:
    value: int
    left: TreeNode? = none
    right: TreeNode? = none
end

object Tree:
    root: TreeNode? = none

    create smallest(self) -> int?:
        if self.root == none: return none
        var node = self.root
        loop node.left != none:
            node = node.left
        end
        return node.value
    end
end

main:
    let t = Tree(TreeNode(8, TreeNode(3, TreeNode(1)), TreeNode(10)))
    print(t.smallest(), Tree().smallest())
end
```

</details>

## 70.5 Project: undo, and a word index

**Undo** is two stacks: every change goes on the undo stack; undoing moves
it to the redo stack; a new change clears redo:

```vyrn
object Editor:
    text: str = ""
    undo_stack: list[str] = []
    redo_stack: list[str] = []

    create type_text(self, more: str):
        self.undo_stack.append(self.text)
        self.redo_stack = []
        self.text += more
    end
    create undo(self):
        if not self.undo_stack: return
        self.redo_stack.append(self.text)
        self.text = self.undo_stack.pop()
    end
    create redo(self):
        if not self.redo_stack: return
        self.undo_stack.append(self.text)
        self.text = self.redo_stack.pop()
    end
end

main:
    let e = Editor()
    e.type_text("Hello")
    e.type_text(", world")
    e.type_text("!!!")
    e.undo()
    print(repr(e.text))
    e.undo()
    print(repr(e.text))
    e.redo()
    print(repr(e.text))
    e.type_text("?")
    e.redo()
    print(repr(e.text))
end
```

```text
"Hello, world"
"Hello"
"Hello, world"
"Hello, world?"
```

A **word index** (the back of a book: each word with the pages it's on)
is a map from word to a set of pages, printed in sorted order:

```vyrn
create index(pages: list[str]) -> map:
    var found = {}
    each number, text in enumerate(pages):
        each word in text.lower().split():
            let w = word.strip(".,!?;:")
            if len(w) < 4: skip
            if w not in found: found[w] = set()
            found[w].add(number + 1)
        end
    end
    return found
end

main:
    let book = ["The engine computes numbers.", "Numbers become music, said Ada.", "The engine weaves patterns like a loom weaves flowers."]
    let idx = index(book)
    each word in sorted(idx.keys()):
        print("{word:<10} {", ".join([str(p) each p in sorted(idx[word])])}")
    end
end
```

```text
become     2
computes   1
engine     1, 3
flowers    3
like       3
loom       3
music      2
numbers    1, 2
patterns   3
said       2
weaves     3
```

### Exercise

Write `page_list(pages)`, which turns a set of page numbers into the text
the index prints: sorted, separated by commas.

```vyrn starter
create page_list(pages: any) -> str:
    return ""
end

main:
    print(page_list({3, 1}), page_list({2}))
end
```

```text expect
1, 3 2
```

> Hint: `", ".join([str(p) each p in sorted(pages)])`

<details><summary>Solution</summary>

```vyrn solution
create page_list(pages: any) -> str:
    return ", ".join([str(p) each p in sorted(pages)])
end

main:
    print(page_list({3, 1}), page_list({2}))
end
```

</details>

## Today you learned

- A stack is last in, first out (undo, brackets, the call stack); a queue
  is first in, first out (jobs waiting).
- A linked list is nodes pointing to the next: instant to add at the front,
  slow to index.
- A binary search tree keeps items sorted for fast search, but must stay
  balanced to stay fast.
- Give data structures `len`, `contains` and `iter` and they work like
  built-in ones.
