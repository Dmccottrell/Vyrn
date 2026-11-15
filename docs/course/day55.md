# Day 55: Grouping and joining

Yesterday's questions were about one column at a time. The interesting
questions are about **groups** ("which region sells most?") and about
**combining** tables ("which customers bought the most expensive
products?"). Today you group rows and summarise each group, add computed
columns, and join tables that share a key. The project answers a
manager's questions about a year of sales.

## 55.1 Grouping

`group_by(column, value, how)` makes one row per distinct value of
`column`, summarising `value` in each group with `how`: `"sum"`,
`"mean"`, `"median"`, `"min"`, `"max"` or `"count"`:

```vyrn
use vyrn.table

const SALES = "region,product,units,price\nNorth,Tea,10,3.5\nSouth,Tea,4,3.5\nNorth,Cake,6,2.0\nEast,Tea,7,3.5\nSouth,Cake,12,2.0\nNorth,Tea,3,3.5\n"

main:
    let s = table.from_csv(SALES)
    print(s.group_by("region", "units", "sum"))
    print(s.group_by("product", "units", "mean"))
    print(s.group_by("region"))
end
```

```text
region  units_sum
------  ---------
North          19
South          16
East            7
product  units_mean
-------  ----------
Tea             6.0
Cake            9.0
region  count
------  -----
North       3
South       2
East        1
```

Without a value, `group_by` counts the rows in each group. Groups appear
in the order they're first seen; sort the result to rank them.

## 55.2 Computed columns

Often the column you want to summarise isn't in the data: revenue is units
times price. Compute it for every row with a comprehension and add it:

```vyrn
use vyrn.table

const SALES = "region,product,units,price\nNorth,Tea,10,3.5\nSouth,Tea,4,3.5\nNorth,Cake,6,2.0\nEast,Tea,7,3.5\nSouth,Cake,12,2.0\nNorth,Tea,3,3.5\n"

main:
    var s = table.from_csv(SALES)
    s = s.add_column("revenue", [r["units"] * r["price"] each r in s])
    let by_region = s.group_by("region", "revenue", "sum").sort("revenue_sum", descending = true)
    print(by_region)
    let total = s.sum("revenue")
    each r in by_region:
        print("{r["region"]:<6} {100 * r["revenue_sum"] / total:5.1f}%")
    end
end
```

```text
region  revenue_sum
------  -----------
North          57.5
South          38.0
East           24.5
North   47.9%
South   31.7%
East    20.4%
```

Shares of a total are often more telling than the totals themselves.

### Exercise

Which product brings in the most revenue? Print the product with the
largest revenue sum, and its revenue.

```vyrn starter
use vyrn.table

const SALES = "product,units,price\nTea,10,3.5\nCake,6,2.0\nCoffee,3,4.0\nTea,4,3.5\nCake,20,2.0\n"

main:
    let s = table.from_csv(SALES)
end
```

```text expect
Cake 52.0
```

> Hint: add a `revenue` column, then `group_by("product", "revenue", "sum")`.
> Hint: sort descending and take `.row(0)`.

<details><summary>Solution</summary>

```vyrn solution
use vyrn.table

const SALES = "product,units,price\nTea,10,3.5\nCake,6,2.0\nCoffee,3,4.0\nTea,4,3.5\nCake,20,2.0\n"

main:
    var s = table.from_csv(SALES)
    s = s.add_column("revenue", [r["units"] * r["price"] each r in s])
    let best = s.group_by("product", "revenue", "sum").sort("revenue_sum", descending = true).row(0)
    print(best["product"], best["revenue_sum"])
end
```

</details>

## 55.3 Joining tables

Well-organised data keeps each fact once: orders in one table, customers
in another, linked by a customer id. To answer a question that needs
both, **join** them on that key: each order gets its customer's columns
beside it:

```vyrn
use vyrn.table

const ORDERS = "order_id,customer_id,total\n1,10,25.0\n2,11,40.0\n3,10,15.0\n4,99,5.0\n"
const CUSTOMERS = "customer_id,name,city\n10,Ada,London\n11,Alan,Leeds\n12,Grace,York\n"

main:
    let orders = table.from_csv(ORDERS)
    let customers = table.from_csv(CUSTOMERS)
    print(orders.join(customers, "customer_id"))
    print(orders.join(customers, "customer_id", how = "left").column("name"))
end
```

```text
order_id  customer_id  total  name  city
--------  -----------  -----  ----  ------
       1           10   25.0  Ada   London
       2           11   40.0  Alan  Leeds
       3           10   15.0  Ada   London
["Ada", "Alan", "Ada", none]
```

Look at what each kind of join keeps:

| Join | Keeps |
|---|---|
| inner (the default) | only orders whose customer exists: order 4's customer 99 is unknown, so it's left out |
| `how = "left"` | every order; missing customers get empty cells |

A left join is the safer default when you're counting things: an inner
join silently drops rows that don't match, and your totals come out low.
Comparing the two row counts is a quick check for bad keys.

## 55.4 Project: a year of sales

A manager asks three questions: Which city brings in the most? Who are our
top three customers? Is any customer ordering but missing from the
customer list? Joining, computing, grouping and sorting answer all three:

```vyrn
use vyrn.table

const ORDERS = "order_id,customer_id,product,qty\n1,10,Tea,4\n2,11,Cake,2\n3,10,Coffee,1\n4,12,Tea,10\n5,13,Cake,5\n6,11,Tea,3\n7,14,Coffee,2\n8,10,Cake,1\n"
const CUSTOMERS = "customer_id,name,city\n10,Ada,London\n11,Alan,Leeds\n12,Grace,London\n13,Mo,York\n"
const PRODUCTS = "product,price\nTea,3.5\nCake,2.0\nCoffee,4.0\n"

main:
    let orders = table.from_csv(ORDERS).join(table.from_csv(PRODUCTS), "product")
    let lines = orders.add_column("revenue", [r["qty"] * r["price"] each r in orders])
    let full = lines.join(table.from_csv(CUSTOMERS), "customer_id", how = "left")

    print("Revenue by city:")
    print(full.drop_missing("city").group_by("city", "revenue", "sum").sort("revenue_sum", descending = true))

    print("Top customers:")
    let top = full.drop_missing("name").group_by("name", "revenue", "sum").sort("revenue_sum", descending = true).head(3)
    each r in top:
        print("  {r["name"]:<6} £{r["revenue_sum"]:.2f}")
    end

    let unknown = full.keep([r["name"] == none each r in full])
    print("Orders from unknown customers:", unknown.column("customer_id"), "worth £{unknown.sum("revenue"):.2f}")
end
```

```text
Revenue by city:
city    revenue_sum
------  -----------
London         55.0
Leeds          14.5
York           10.0
Top customers:
  Grace  £35.00
  Ada    £20.00
  Alan   £14.50
Orders from unknown customers: [14] worth £8.00
```

Three answers, and a data problem found on the way: customer 14 orders but
isn't in the customer list. Finding problems like that is half the value
of analysis. Had the code used an inner join, that order would have
vanished from every total without a word.

### Exercise

Add a fourth answer: the average order value (revenue per order) for each
city, highest first, rounded to 2 places.

```vyrn starter
use vyrn.table

main:
    let full = table.from_csv("city,revenue\nLondon,14\nLeeds,7\nLondon,35\nYork,10\nLeeds,6.5\nLondon,6\n")
end
```

```text expect
London 18.33
York 10.0
Leeds 6.75
```

> Hint: `group_by("city", "revenue", "mean")`, sorted descending.
> Hint: `round(r["revenue_mean"], 2)`.

<details><summary>Solution</summary>

```vyrn solution
use vyrn.table

main:
    let full = table.from_csv("city,revenue\nLondon,14\nLeeds,7\nLondon,35\nYork,10\nLeeds,6.5\nLondon,6\n")
    let avg = full.group_by("city", "revenue", "mean").sort("revenue_mean", descending = true)
    each r in avg:
        print(r["city"], round(r["revenue_mean"], 2))
    end
end
```

</details>

## Today you learned

- `group_by(column, value, how)` summarises each group; without a value it
  counts rows.
- Add computed columns with `add_column` and a comprehension.
- `join(other, key)` combines tables; an inner join drops unmatched rows,
  `how = "left"` keeps them. Compare row counts to catch bad keys.
- Analysis answers questions and finds data problems.
