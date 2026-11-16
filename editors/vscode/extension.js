// Vyrn for VS Code. The language server (`vyrn lsp`) gives errors as you
// type, completion, hover, go to definition, signature help, the outline
// and formatting. Without the `vscode-languageclient` package (run
// `npm install` here), the extension falls back to running `vyrn check`
// and `vyrn fmt`. Everything comes from the `vyrn` command, so the editor
// and the compiler always agree.

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

// Starts `vyrn lsp`. Returns false when the client library is missing or
// the server is turned off, so the fallback below takes over.
function startLanguageServer(context) {
  if (!vscode.workspace.getConfiguration("vyrn").get("languageServer")) return false;
  let lc;
  try {
    lc = require("vscode-languageclient/node");
  } catch (_) {
    return false;
  }
  const server = { command: vyrnPath(), args: ["lsp"] };
  const client = new lc.LanguageClient(
    "vyrn",
    "Vyrn",
    { run: server, debug: server },
    { documentSelector: [{ scheme: "file", language: "vyrn" }] },
  );
  client.start();
  context.subscriptions.push({ dispose: () => client.stop() });
  return true;
}

function registerCommands(context) {
  context.subscriptions.push(
    vscode.commands.registerCommand("vyrn.run", () => {
      const editor = vscode.window.activeTextEditor;
      if (editor) terminal(`${vyrnPath()} run "${editor.document.uri.fsPath}"`);
    }),
    vscode.commands.registerCommand("vyrn.test", () => terminal(`${vyrnPath()} test`)),
    vscode.commands.registerCommand("vyrn.repl", () => terminal(vyrnPath())),
    vscode.commands.registerCommand("vyrn.debug", () => {
      const editor = vscode.window.activeTextEditor;
      if (!editor) return;
      vscode.debug.startDebugging(vscode.workspace.getWorkspaceFolder(editor.document.uri), {
        type: "vyrn",
        request: "launch",
        name: "Debug this file",
        program: editor.document.uri.fsPath,
      });
    }),
  );
}

// F5 on a .vyrn file: `vyrn debug --dap` speaks the Debug Adapter Protocol,
// and without a launch.json the open file is debugged.
function registerDebugger(context) {
  context.subscriptions.push(
    vscode.debug.registerDebugAdapterDescriptorFactory("vyrn", {
      createDebugAdapterDescriptor: () => new vscode.DebugAdapterExecutable(vyrnPath(), ["debug", "--dap"]),
    }),
    vscode.debug.registerDebugConfigurationProvider("vyrn", {
      resolveDebugConfiguration(folder, config) {
        if (!config.type && !config.request && !config.name) {
          const editor = vscode.window.activeTextEditor;
          if (!editor || editor.document.languageId !== "vyrn") return config;
          config = { type: "vyrn", request: "launch", name: "Debug this file", program: editor.document.uri.fsPath };
        }
        if (!config.program) {
          return vscode.window.showErrorMessage("Set `program` to the .vyrn file to debug.").then(() => undefined);
        }
        return config;
      },
    }),
  );
}

function activate(context) {
  registerCommands(context);
  registerDebugger(context);
  if (startLanguageServer(context)) return;
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
  );
  vscode.workspace.textDocuments.forEach((d) => check(d, collection));
}

function deactivate() {}

module.exports = { activate, deactivate };
