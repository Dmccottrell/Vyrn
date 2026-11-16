# Day 56: Charts

A table of numbers answers a question for whoever reads it carefully; a
chart answers it at a glance. Today you make the four charts that cover
most needs, learn which one fits which question, and put charts into web
pages. The project charts a year of weather for two cities.

## 56.1 Your first chart

`vyrn.chart` makes a chart from lists of numbers, and `save` writes it as
a PNG picture or an SVG drawing:

```vyrn
use vyrn.chart

main:
    let c = chart.bar(["North", "South", "East"], [57.5, 38, 24.5], title = "Revenue by region", y_label = "£")
    c.save("revenue.png")
    print(c)
end
```

```text
Chart(640x400)
```

Open `revenue.png` to see it: a bar per region, labelled, with a scale.
Every chart takes `title`, `x_label`, `y_label`, `width` and `height`.

## 56.2 Choosing the chart

The right chart depends on the question:

| Question | Chart | `vyrn.chart` |
|---|---|---|
| How do categories compare? | bar | `chart.bar(labels, values)` |
| How does something change over time? | line | `chart.line(xs, ys)` |
| Are two measurements related? | scatter | `chart.scatter(xs, ys)` |
| How are values spread out? | histogram | `chart.hist(values, bins)` |
| What share is each part of a whole? | pie | `chart.pie(labels, values)` |

Give several **series** as a map from name to values, and they share one
chart with a legend:

```vyrn
use vyrn.chart

main:
    let months = [1, 2, 3, 4, 5, 6]
    let temps = {"London": [5, 7, 11, 14, 17, 20], "Oslo": [-3, -2, 2, 7, 12, 16]}
    chart.line(months, temps, title = "Average temperature", x_label = "month", y_label = "°C").save("temps.png")
    print("saved temps.png")
end
```

```text
saved temps.png
```

A histogram groups values into **bins** and shows how many fall in each,
which reveals the shape of the data: where most values are, and whether
there are odd ones far away:

```vyrn
use vyrn.chart
use vyrn.random

main:
    random.seed(3)
    let heights = [round(random.float(150, 175) + random.float(0, 20), 1) each _ in 0 until 500]
    chart.hist(heights, 12, title = "Heights of 500 people", x_label = "cm").save("heights.png")
    print(min(heights), max(heights))
end
```

```text
151.7 193.1
```

## 56.3 Charts that tell the truth

A chart can mislead without a single wrong number. Three rules:

- **Bars start at zero.** A bar's length *is* its value; cut off the bottom
  and a 5% difference looks like 50%.
- **Label everything**: what is measured, in which unit, over what time.
  A chart without labels is a picture, not information.
- **Don't use too many series.** Past about five lines, nobody can tell
  them apart; use several small charts instead.

Pie charts are the easiest to misread (people judge angles badly): use one
only for a few parts of a whole, and prefer bars when in doubt.

### Exercise

Make a scatter chart of hours studied against test score, titled "Study
and scores", with both axes labelled, saved as `study.png`. Then print how
many points it shows.

```vyrn starter
use vyrn.chart

main:
    let hours = [1, 2, 2, 3, 4, 5, 6, 7]
    let scores = [52, 58, 61, 60, 70, 74, 79, 88]
end
```

```text expect
8 points
```

> Hint: `chart.scatter(hours, scores, title = ..., x_label = "hours", y_label = "score")`
> Hint: then `.save("study.png")`, and print `len(hours)`.

<details><summary>Solution</summary>

```vyrn solution
use vyrn.chart

main:
    let hours = [1, 2, 2, 3, 4, 5, 6, 7]
    let scores = [52, 58, 61, 60, 70, 74, 79, 88]
    chart.scatter(hours, scores, title = "Study and scores", x_label = "hours", y_label = "score").save("study.png")
    print(len(hours), "points")
end
```

</details>

## 56.4 Charts in web pages

`c.svg()` gives the chart as SVG text, which a web page can include
directly: no picture file needed, and it stays sharp at any size. A route
can draw a chart from live data every time the page is asked for:

```vyrn
use vyrn.chart
use vyrn.html
use vyrn.http

route GET "/report" (sales):
    let c = chart.bar(sales.keys(), sales.values(), title = "Sales this week", y_label = "units")
    return "<!doctype html><h1>Weekly report</h1>" + c.svg() + "<p>Total: {sum(sales.values())} units</p>"
end

main:
    http.provide("sales", {"Mon": 12, "Tue": 18, "Wed": 9})
    let page = html.parse(http.test("GET", "/report").text)
    print(page.select_one("h1").text, "|", len(page.select("svg")), "chart |", page.select_one("p").text)
end
```

```text
Weekly report | 1 chart | Total: 39 units
```

## 56.5 Project: a year of weather

Monthly temperatures and rainfall for two cities, from a CSV file. The
program makes three charts, each answering one question, and a small HTML
report that holds them:

```vyrn
use vyrn.chart
use vyrn.table

const WEATHER = "month,city,temp,rain\n1,London,5,55\n2,London,6,41\n3,London,9,42\n4,London,11,44\n5,London,14,49\n6,London,17,45\n7,London,19,45\n8,London,19,50\n9,London,16,49\n10,London,13,69\n11,London,9,59\n12,London,6,55\n1,Oslo,-3,49\n2,Oslo,-3,36\n3,Oslo,1,47\n4,Oslo,6,41\n5,Oslo,11,53\n6,Oslo,15,65\n7,Oslo,17,81\n8,Oslo,16,89\n9,Oslo,12,90\n10,Oslo,7,84\n11,Oslo,2,73\n12,Oslo,-2,55\n"

main:
    let w = table.from_csv(WEATHER)
    let months = list(1 to 12)
    let cities = w.unique("city")
    let temps = {c: w.where("city", "==", c).column("temp") each c in cities}
    let rain = {c: w.where("city", "==", c).sum("rain") each c in cities}

    let charts = [
        chart.line(months, temps, title = "Temperature through the year", x_label = "month", y_label = "°C"),
        chart.bar(cities, [rain[c] each c in cities], title = "Rain in a year", y_label = "mm"),
        chart.scatter(w.column("temp"), w.column("rain"), title = "Is warmer wetter?", x_label = "°C", y_label = "mm of rain"),
    ]
    let body = "".join(["<section>" + c.svg() + "</section>" each c in charts])
    write_text("weather.html", "<!doctype html><title>Weather</title><h1>A year of weather</h1>" + body)

    each c in cities:
        let t = temps[c]
        print("{c}: {min(t)} to {max(t)} °C, {rain[c]} mm of rain")
    end
    print("wrote weather.html with {len(charts)} charts")
end
```

```text
London: 5 to 19 °C, 603 mm of rain
Oslo: -3 to 17 °C, 763 mm of rain
wrote weather.html with 3 charts
```

Open `weather.html` in a browser. Each chart answers one question, with
its title phrased as that question or its answer. The scatter chart's
"Is warmer wetter?" is the kind of question to ask of any two
measurements; a pattern in a scatter chart is a clue, though not proof
that one causes the other.

### Exercise

Add a fourth chart: the temperature *range* (warmest month minus coldest)
for each city, as bars. Print the ranges.

```vyrn starter
use vyrn.chart

main:
    let temps = {"London": [5, 6, 9, 11, 14, 17, 19, 19, 16, 13, 9, 6], "Oslo": [-3, -3, 1, 6, 11, 15, 17, 16, 12, 7, 2, -2]}
end
```

```text expect
{"London": 14, "Oslo": 20}
```

> Hint: `ranges = {c: max(t) - min(t) each c, t in temps.items()}`
> Hint: `chart.bar(ranges.keys(), ranges.values(), ...)`

<details><summary>Solution</summary>

```vyrn solution
use vyrn.chart

main:
    let temps = {"London": [5, 6, 9, 11, 14, 17, 19, 19, 16, 13, 9, 6], "Oslo": [-3, -3, 1, 6, 11, 15, 17, 16, 12, 7, 2, -2]}
    let ranges = {c: max(t) - min(t) each c, t in temps.items()}
    chart.bar(ranges.keys(), ranges.values(), title = "Temperature range", y_label = "°C").save("ranges.png")
    print(ranges)
end
```

</details>

## Today you learned

- `chart.bar`, `line`, `scatter`, `hist` and `pie` draw charts; `save`
  writes PNG or SVG; a map of series adds a legend.
- Choose the chart by the question: compare, change over time, relation,
  spread, share.
- Bars start at zero, label everything, keep it simple.
- `c.svg()` puts a chart straight into a web page.
