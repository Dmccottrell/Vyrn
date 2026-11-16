// Turns `vyrn check` output into problems: message, file, position, width,
// and the hint. No VS Code dependency, so it can be tested on its own.
//
//   error: cannot assign to `n`: it is declared with `let`
//    --> main.vyrn:3:5
//     |
//   3 |     n = 2
//     |     ^
//     = help: declare it with `var n` to change it

function parseCheckOutput(text) {
  const problems = [];
  let current = null;
  for (const line of text.split(/\r?\n/)) {
    const error = /^error: (.*)$/.exec(line);
    if (error) {
      current = { message: error[1], file: null, line: 1, column: 1, width: 1, hint: null };
      problems.push(current);
      continue;
    }
    if (!current) continue;
    const where = /^\s*--> (.*):(\d+):(\d+)\s*$/.exec(line);
    if (where) {
      current.file = where[1];
      current.line = Number(where[2]);
      current.column = Number(where[3]);
      continue;
    }
    const marker = /^\s*\|\s*[ \t]*(\^+)\s*$/.exec(line);
    if (marker) {
      current.width = marker[1].length;
      continue;
    }
    const help = /^\s*= help: (.*)$/.exec(line);
    if (help) current.hint = help[1];
  }
  return problems;
}

module.exports = { parseCheckOutput };
