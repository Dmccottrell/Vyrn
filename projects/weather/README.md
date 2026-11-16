# Weather Report

A year of weather in four cities, analysed into charts and a web page. Written in [Vyrn](https://github.com/dmccottrell/vyrn).

## Run it

Install Vyrn first (see its README), then in this folder:

```powershell
vyrn run
vyrn test
```

Then open `out/report.html` in a browser.

## Files

| File | What it does |
|---|---|
| `main.vyrn` | runs the analysis and writes the report |
| `data.vyrn` | makes realistic weather data, with seasons |
| `analysis.vyrn` | the questions: highs, lows, rain per city and month |
| `report.vyrn` | the HTML report template |
| `tests/` | checks each question on hand-made data |

## What it shows

CSV files, tables (filter, sort, group, average), charts as PNG and SVG, and HTML templates. Every file is commented: read them top to bottom.
