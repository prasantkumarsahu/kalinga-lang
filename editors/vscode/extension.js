// Kalinga for VS Code: starts the language server (`kalinga lsp`), which provides
// diagnostics, completion, hover, signature help, navigation, rename and quick fixes,
// and adds commands to run, build and inspect a file.
"use strict";

const vscode = require("vscode");
const { execFile } = require("child_process");
const path = require("path");
const { LanguageClient } = require("vscode-languageclient/node");

const LANG = "kalinga";

/** @type {LanguageClient | undefined} */
let client;

function compilerPath() {
  return vscode.workspace.getConfiguration("kalinga").get("compilerPath") || "kalinga";
}

function showMissingCompiler() {
  const message =
    `The Kalinga compiler was not found ("${compilerPath()}"). Install it with ` +
    "`cargo install --path .` in the kalinga-lang repository, or set `kalinga.compilerPath`.";
  vscode.window.showWarningMessage(message, "Open Settings").then((choice) => {
    if (choice) vscode.commands.executeCommand("workbench.action.openSettings", "kalinga.compilerPath");
  });
}

async function startClient(context) {
  const server = { command: compilerPath(), args: ["lsp"] };
  client = new LanguageClient(
    "kalinga",
    "Kalinga",
    { run: server, debug: server },
    {
      documentSelector: [
        { scheme: "file", language: LANG },
        { scheme: "untitled", language: LANG },
      ],
    },
  );
  try {
    await client.start();
    context.subscriptions.push(client);
  } catch {
    client = undefined;
    const message =
      `Could not start the Kalinga language server (\`${compilerPath()} lsp\`). It needs Kalinga 0.5 or later: ` +
      "install it with `cargo install --path .` in the kalinga-lang repository, or set `kalinga.compilerPath`.";
    vscode.window.showWarningMessage(message, "Open Settings").then((choice) => {
      if (choice) vscode.commands.executeCommand("workbench.action.openSettings", "kalinga.compilerPath");
    });
  }
}

async function restartClient(context) {
  if (client) {
    await client.stop().catch(() => {});
    client = undefined;
  }
  await startClient(context);
}

/** The active saved Kalinga file, saving it first if needed. */
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
    new vscode.ProcessExecution(compilerPath(), args, { cwd }),
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
      if (err.code === "ENOENT") showMissingCompiler();
      else vscode.window.showErrorMessage(stderr.trim() || String(err));
      return;
    }
    const doc = await vscode.workspace.openTextDocument({ language: "rust", content: stdout });
    vscode.window.showTextDocument(doc, { viewColumn: vscode.ViewColumn.Beside, preview: true });
  });
}

async function activate(context) {
  context.subscriptions.push(
    vscode.commands.registerCommand("kalinga.run", runFile),
    vscode.commands.registerCommand("kalinga.build", buildFile),
    vscode.commands.registerCommand("kalinga.emit", emitRust),
    vscode.commands.registerCommand("kalinga.restartServer", () => restartClient(context)),
    vscode.workspace.onDidChangeConfiguration((e) => {
      if (e.affectsConfiguration("kalinga.compilerPath")) restartClient(context);
    }),
  );
  await startClient(context);
}

function deactivate() {
  return client ? client.stop() : undefined;
}

module.exports = { activate, deactivate };
