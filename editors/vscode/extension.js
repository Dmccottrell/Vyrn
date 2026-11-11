// Vyrn for VS Code: errors from `vyrn check` as you work, formatting with
// `vyrn fmt`, and commands to run a file or the tests. Everything comes
// from the `vyrn` command, so the editor and the compiler always agree.

const vscode = require("vscode");
const { execFile } = require("child_process");
const fs = require("fs");
const path = require("path");
const { parseCheckOutput } = require("./diagnostics");

function vyrnPath() {
  return vscode.workspace.getConfiguration("vyrn").get("path") || "vyrn";
}

function run(args, cwd) {
  return new Promise((resolve) => {
    execFile(vyrnPath(), args, { cwd, timeout: 20000 }, (error, stdout, stderr) => {
      if (error && error.code === "ENOENT") {
        vscode.window.showErrorMessage(
          "Vyrn: the `vyrn` command was not found. Install it (`cargo install --path .` in the Vyrn repository) or set `vyrn.path`.",
        );
      }
      resolve({ ok: !error, stdout: stdout || "", stderr: stderr || "" });
    });
  });
}

// Checks a document. Unsaved text is checked from a hidden copy next to the
// file, so `use` lines find the same modules.
async function check(document, collection) {
  if (document.languageId !== "vyrn" || document.uri.scheme !== "file") return;
  const file = document.uri.fsPath;
  const dir = path.dirname(file);
  let target = file;
  let temp = null;
  if (document.isDirty) {
    temp = path.join(dir, `.${path.basename(file, ".vyrn")}.check.vyrn`);
    fs.writeFileSync(temp, document.getText());
    target = temp;
  }
  const result = await run(["check", target], dir);
  if (temp) fs.rmSync(temp, { force: true });

  const byFile = new Map();
  for (const p of parseCheckOutput(result.stderr)) {
    let where = p.file ? path.resolve(dir, p.file) : file;
    if (temp && path.resolve(where) === path.resolve(temp)) where = file;
    const range = new vscode.Range(p.line - 1, p.column - 1, p.line - 1, p.column - 1 + p.width);
    const message = p.hint ? `${p.message}\n${p.hint}` : p.message;
    const d = new vscode.Diagnostic(range, message, vscode.DiagnosticSeverity.Error);
    d.source = "vyrn";
    if (!byFile.has(where)) byFile.set(where, []);
    byFile.get(where).push(d);
  }
  collection.delete(document.uri);
  for (const [where, list] of byFile) collection.set(vscode.Uri.file(where), list);
}

async function format(document) {
  const dir = path.dirname(document.uri.fsPath);
  const temp = path.join(dir, `.${path.basename(document.uri.fsPath, ".vyrn")}.fmt.vyrn`);
  fs.writeFileSync(temp, document.getText());
  const result = await run(["fmt", temp], dir);
  const formatted = fs.existsSync(temp) ? fs.readFileSync(temp, "utf8") : null;
  fs.rmSync(temp, { force: true });
  if (!result.ok || formatted === null || formatted === document.getText()) return [];
  const all = new vscode.Range(0, 0, document.lineCount, 0);
  return [vscode.TextEdit.replace(all, formatted)];
}

function terminal(command) {
  const t = vscode.window.createTerminal("Vyrn");
  t.show();
  t.sendText(command);
}

function activate(context) {
  const collection = vscode.languages.createDiagnosticCollection("vyrn");
  context.subscriptions.push(collection);
  let timer = null;

  context.subscriptions.push(
    vscode.workspace.onDidOpenTextDocument((d) => check(d, collection)),
    vscode.workspace.onDidSaveTextDocument((d) => check(d, collection)),
    vscode.workspace.onDidChangeTextDocument((e) => {
      if (!vscode.workspace.getConfiguration("vyrn").get("checkOnType")) return;
      clearTimeout(timer);
      timer = setTimeout(() => check(e.document, collection), 400);
    }),
    vscode.workspace.onDidCloseTextDocument((d) => collection.delete(d.uri)),
    vscode.languages.registerDocumentFormattingEditProvider("vyrn", { provideDocumentFormattingEdits: format }),
    vscode.commands.registerCommand("vyrn.run", () => {
      const editor = vscode.window.activeTextEditor;
      if (editor) terminal(`${vyrnPath()} run "${editor.document.uri.fsPath}"`);
    }),
    vscode.commands.registerCommand("vyrn.test", () => terminal(`${vyrnPath()} test`)),
  );
  vscode.workspace.textDocuments.forEach((d) => check(d, collection));
}

function deactivate() {}

module.exports = { activate, deactivate };
