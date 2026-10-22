// Kalinga VS Code extension: live diagnostics via `kalinga check`, plus run/build/emit commands.
const vscode = require("vscode");
const { execFile } = require("child_process");
const path = require("path");
const completion = require("./completion");

const LANG = "kalinga";
const CHECK_DELAY_MS = 400;

/** @type {vscode.DiagnosticCollection} */
let diagnostics;
const timers = new Map();
let warnedMissingCompiler = false;

function config() {
  return vscode.workspace.getConfiguration("kalinga");
}

function compilerPath() {
  return config().get("compilerPath") || "kalinga";
}

function reportMissingCompiler(err) {
  if (err && err.code === "ENOENT" && !warnedMissingCompiler) {
    warnedMissingCompiler = true;
    vscode.window
      .showWarningMessage(
        `Kalinga compiler not found ("${compilerPath()}"). Install it with \`cargo install --path .\` in the kalinga-lang repo, or set "kalinga.compilerPath".`,
        "Open Settings"
      )
      .then((choice) => {
        if (choice) vscode.commands.executeCommand("workbench.action.openSettings", "kalinga.compilerPath");
      });
    return true;
  }
  return false;
}

/**
 * Parses `<file>:<line>:<col>: error: <message>` (file may itself contain colons on Windows).
 * @returns {vscode.Diagnostic[]}
 */
function parseErrors(stderr, document) {
  const out = [];
  for (const line of stderr.split(/\r?\n/)) {
    const m = /:(\d+):(\d+): error: (.*)$/.exec(line);
    if (!m) continue;
    const lineNo = Math.max(0, Math.min(parseInt(m[1], 10) - 1, document.lineCount - 1));
    const col = Math.max(0, parseInt(m[2], 10) - 1);
    const pos = new vscode.Position(lineNo, col);
    const range =
      document.getWordRangeAtPosition(pos) ||
      new vscode.Range(pos, pos.translate(0, document.lineAt(lineNo).text.length > col ? 1 : 0));
    const d = new vscode.Diagnostic(range, m[3], vscode.DiagnosticSeverity.Error);
    d.source = "kalinga";
    out.push(d);
  }
  return out;
}

function check(document) {
  if (document.languageId !== LANG) return;
  const version = document.version;
  const label = path.basename(document.fileName) || "untitled.kl";
  const child = execFile(compilerPath(), ["check", label, "--stdin"], { timeout: 10000 }, (err, _stdout, stderr) => {
    if (err && reportMissingCompiler(err)) return;
    // Ignore results for text that has changed since the check started.
    if (document.isClosed || document.version !== version) return;
    if (!err) {
      diagnostics.delete(document.uri);
      return;
    }
    let found = parseErrors(stderr || "", document);
    if (found.length === 0 && stderr && stderr.trim()) {
      found = [new vscode.Diagnostic(new vscode.Range(0, 0, 0, 0), stderr.trim(), vscode.DiagnosticSeverity.Error)];
    }
    diagnostics.set(document.uri, found);
  });
  if (child.stdin) {
    child.stdin.on("error", () => {});
    child.stdin.end(document.getText());
  }
}

function scheduleCheck(document) {
  if (document.languageId !== LANG) return;
  const key = document.uri.toString();
  clearTimeout(timers.get(key));
  timers.set(
    key,
    setTimeout(() => {
      timers.delete(key);
      check(document);
    }, CHECK_DELAY_MS)
  );
}

/** Returns the active saved Kalinga file, saving it first if needed. */
async function activeKalingaFile() {
  const editor = vscode.window.activeTextEditor;
  if (!editor || editor.document.languageId !== LANG) {
    vscode.window.showErrorMessage("Open a Kalinga (.kl) file first.");
    return undefined;
  }
  const doc = editor.document;
  if (doc.isUntitled) {
    vscode.window.showErrorMessage("Save the file before running it.");
    return undefined;
  }
  if (doc.isDirty) await doc.save();
  return doc.fileName;
}

function runTask(name, args, cwd) {
  const task = new vscode.Task(
    { type: "kalinga", command: args[0] },
    vscode.TaskScope.Workspace,
    name,
    "kalinga",
    new vscode.ProcessExecution(compilerPath(), args, { cwd })
  );
  task.presentationOptions = { reveal: vscode.TaskRevealKind.Always, clear: true, focus: false };
  return vscode.tasks.executeTask(task).then(undefined, (e) => vscode.window.showErrorMessage(String(e)));
}

async function runFile() {
  const file = await activeKalingaFile();
  if (file) runTask(`run ${path.basename(file)}`, ["run", file], path.dirname(file));
}

async function buildFile() {
  const file = await activeKalingaFile();
  if (file) runTask(`build ${path.basename(file)}`, ["build", file], path.dirname(file));
}

async function emitRust() {
  const file = await activeKalingaFile();
  if (!file) return;
  execFile(compilerPath(), ["emit", file], { maxBuffer: 32 * 1024 * 1024 }, async (err, stdout, stderr) => {
    if (err) {
      if (!reportMissingCompiler(err)) vscode.window.showErrorMessage(stderr.trim() || String(err));
      return;
    }
    const doc = await vscode.workspace.openTextDocument({ language: "rust", content: stdout });
    vscode.window.showTextDocument(doc, { viewColumn: vscode.ViewColumn.Beside, preview: true });
  });
}

const ITEM_KINDS = {
  keyword: vscode.CompletionItemKind.Keyword,
  variable: vscode.CompletionItemKind.Variable,
  field: vscode.CompletionItemKind.Field,
  method: vscode.CompletionItemKind.Method,
  function: vscode.CompletionItemKind.Function,
  constructor: vscode.CompletionItemKind.Constructor,
  class: vscode.CompletionItemKind.Class,
  interface: vscode.CompletionItemKind.Interface,
  type: vscode.CompletionItemKind.TypeParameter,
};

const completionProvider = {
  provideCompletionItems(document, position) {
    const items = completion.complete(document.getText(), document.offsetAt(position));
    return items.map((it) => {
      const item = new vscode.CompletionItem(it.label, ITEM_KINDS[it.kind] || vscode.CompletionItemKind.Text);
      item.insertText = it.snippet ? new vscode.SnippetString(it.insertText) : it.insertText;
      if (it.detail) item.detail = it.detail;
      if (it.documentation) item.documentation = new vscode.MarkdownString(it.documentation);
      if (it.sortText) item.sortText = it.sortText;
      if (it.label.startsWith("override fn ")) item.filterText = it.label.slice("override fn ".length);
      return item;
    });
  },
};

const signatureProvider = {
  provideSignatureHelp(document, position) {
    const sig = completion.signatureHelp(document.getText(), document.offsetAt(position));
    if (!sig) return null;
    const info = new vscode.SignatureInformation(sig.label, sig.documentation ? new vscode.MarkdownString(sig.documentation) : undefined);
    info.parameters = sig.params.map((p) => new vscode.ParameterInformation(p));
    const help = new vscode.SignatureHelp();
    help.signatures = [info];
    help.activeSignature = 0;
    help.activeParameter = sig.activeParameter;
    return help;
  },
};

function activate(context) {
  diagnostics = vscode.languages.createDiagnosticCollection(LANG);
  context.subscriptions.push(
    diagnostics,
    vscode.commands.registerCommand("kalinga.run", runFile),
    vscode.commands.registerCommand("kalinga.build", buildFile),
    vscode.commands.registerCommand("kalinga.emit", emitRust),
    vscode.languages.registerCompletionItemProvider(LANG, completionProvider, "."),
    vscode.languages.registerSignatureHelpProvider(LANG, signatureProvider, "(", ","),
    vscode.workspace.onDidOpenTextDocument(check),
    vscode.workspace.onDidSaveTextDocument(check),
    vscode.workspace.onDidChangeTextDocument((e) => {
      if (config().get("checkOnType")) scheduleCheck(e.document);
    }),
    vscode.workspace.onDidCloseTextDocument((doc) => diagnostics.delete(doc.uri)),
    vscode.workspace.onDidChangeConfiguration((e) => {
      if (e.affectsConfiguration("kalinga.compilerPath")) {
        warnedMissingCompiler = false;
        vscode.workspace.textDocuments.forEach(check);
      }
    })
  );
  vscode.workspace.textDocuments.forEach(check);
}

function deactivate() {
  for (const t of timers.values()) clearTimeout(t);
  timers.clear();
}

module.exports = { activate, deactivate };
