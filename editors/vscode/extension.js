// Kalinga for VS Code: starts the language server (`kalinga lsp`), which provides
// diagnostics, completion, hover, signature help, navigation, rename and quick fixes,
// and adds commands to run, build and inspect a file, and to build and serve web pages
// (`kalinga serve`, shown in VS Code's Simple Browser). The status bar says whether the
// active file builds for the web or natively.
"use strict";

const vscode = require("vscode");
const { execFile } = require("child_process");
const fs = require("fs");
const http = require("http");
const path = require("path");
const { LanguageClient } = require("vscode-languageclient/node");

const LANG = "kalinga";

/** @type {LanguageClient | undefined} */
let client;

function compilerPath() {
  return vscode.workspace.getConfiguration("kalinga").get("compilerPath") || "kalinga";
}

/** `utkal`: the setting, else the one next to the compiler (or on PATH). */
function utkalPath() {
  const set = vscode.workspace.getConfiguration("kalinga").get("utkalPath");
  if (set) return set;
  const compiler = compilerPath();
  if (path.basename(compiler) === compiler) return "utkal";
  const exe = path.extname(compiler);
  return path.join(path.dirname(compiler), "utkal" + exe);
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

/** Runs `kalinga <args>` (or `utkal <args>` with `tool` "utkal") as a task in the terminal. */
function runTask(name, args, cwd, tool = "kalinga") {
  const exe = tool === "utkal" ? utkalPath() : compilerPath();
  const task = new vscode.Task(
    { type: "kalinga", command: `${tool} ${args[0]}` },
    vscode.TaskScope.Workspace,
    name,
    "kalinga",
    new vscode.ProcessExecution(exe, args, { cwd }),
  );
  task.presentationOptions = { reveal: vscode.TaskRevealKind.Always, clear: true, focus: false };
  return vscode.tasks.executeTask(task).then(undefined, (e) => vscode.window.showErrorMessage(String(e)));
}

/** The utkal project the active file belongs to, if any. */
function activeProject() {
  const editor = vscode.window.activeTextEditor;
  return editor ? targetOf(editor.document).project : undefined;
}

/** Whether the project is an app (has `src/main.kl`), so `utkal run` and `utkal build` apply. */
function isApp(project) {
  return fs.existsSync(path.join(project, "src", "main.kl"));
}

// Inside a utkal project the commands run the whole project with `utkal` (its packages,
// Rust crates and target included); a file outside one runs on its own with `kalinga`.

async function runFile() {
  const file = await activeKalingaFile();
  if (!file) return;
  const project = activeProject();
  if (project && isApp(project)) runTask(`utkal run (${path.basename(project)})`, ["run"], project, "utkal");
  else runTask(`run ${path.basename(file)}`, ["run", file], path.dirname(file));
}

async function testFile() {
  const file = await activeKalingaFile();
  if (!file) return;
  const project = activeProject();
  if (project) runTask(`utkal test (${path.basename(project)})`, ["test"], project, "utkal");
  else runTask(`test ${path.basename(file)}`, ["test", file], path.dirname(file));
}

async function buildFile() {
  const file = await activeKalingaFile();
  if (!file) return;
  const project = activeProject();
  if (project && isApp(project)) runTask(`utkal build (${path.basename(project)})`, ["build"], project, "utkal");
  else runTask(`build ${path.basename(file)}`, ["build", file], path.dirname(file));
}

/** Shows the code `kalinga emit` writes for the active file (`--web`: JavaScript). */
async function emitCode(web) {
  const file = await activeKalingaFile();
  if (!file) return;
  const args = web ? ["emit", "--web", file] : ["emit", file];
  execFile(compilerPath(), args, { maxBuffer: 32 * 1024 * 1024 }, async (err, stdout, stderr) => {
    if (err) {
      if (err.code === "ENOENT") showMissingCompiler();
      else vscode.window.showErrorMessage(stderr.trim() || String(err));
      return;
    }
    const doc = await vscode.workspace.openTextDocument({ language: web ? "javascript" : "rust", content: stdout });
    vscode.window.showTextDocument(doc, { viewColumn: vscode.ViewColumn.Beside, preview: true });
  });
}

/**
 * What a file builds for, by the compiler's rule: the `target` of the nearest utkal.toml,
 * else a web build when it imports `std/web` (the compiler also follows its imports).
 * @returns {{ web: boolean, project?: string, why: string }}
 */
function targetOf(doc) {
  if (!doc.isUntitled) {
    let dir = path.dirname(doc.fileName);
    for (;;) {
      const manifest = path.join(dir, "utkal.toml");
      if (fs.existsSync(manifest)) {
        const text = fs.readFileSync(manifest, "utf8");
        const web = /^\s*target\s*=\s*"web"/m.test(text);
        return {
          web,
          project: dir,
          why: `utkal.toml in ${path.basename(dir)} says target = "${web ? "web" : "native"}"`,
        };
      }
      const up = path.dirname(dir);
      if (up === dir) break;
      dir = up;
    }
  }
  if (/^\s*import\s+"std\/web"/m.test(doc.getText())) return { web: true, why: 'the file imports "std/web"' };
  return { web: false, why: 'the file does not import "std/web"' };
}

async function buildWeb() {
  const file = await activeKalingaFile();
  if (!file) return;
  const editor = vscode.window.activeTextEditor;
  const target = editor ? targetOf(editor.document) : { web: false };
  if (target.project && target.web && isApp(target.project)) {
    runTask(`utkal build (${path.basename(target.project)})`, ["build"], target.project, "utkal");
  } else {
    runTask(`build --web ${path.basename(file)}`, ["build", "--web", file], path.dirname(file));
  }
}

/** @type {vscode.TaskExecution | undefined} */
let serving;

/** Waits until something answers on `port` (the first build takes a moment). */
function waitForServer(port, timeoutMs) {
  const start = Date.now();
  return new Promise((resolve) => {
    const attempt = () => {
      const req = http.get({ host: "127.0.0.1", port, path: "/__kalinga/version", timeout: 1000 }, (res) => {
        res.resume();
        resolve(true);
      });
      req.on("error", () => {
        if (Date.now() - start > timeoutMs) resolve(false);
        else setTimeout(attempt, 300);
      });
      req.on("timeout", () => req.destroy());
    };
    attempt();
  });
}

/** `kalinga serve`: builds the page, serves it and opens it in VS Code's Simple Browser. */
async function serveWeb() {
  const file = await activeKalingaFile();
  if (!file) return;
  const editor = vscode.window.activeTextEditor;
  const target = editor ? targetOf(editor.document) : { web: true };
  const port = vscode.workspace.getConfiguration("kalinga").get("webPort") || 8080;
  if (serving) {
    serving.terminate();
    serving = undefined;
  }
  // A web project is served whole with `utkal serve`, whichever of its files is open.
  if (target.project && target.web && isApp(target.project)) {
    const name = `utkal serve (${path.basename(target.project)})`;
    serving = await runTask(name, ["serve", "--port", String(port)], target.project, "utkal");
  } else {
    serving = await runTask(
      `serve ${path.basename(file)}`,
      ["serve", file, "--port", String(port)],
      path.dirname(file),
    );
  }
  const url = `http://localhost:${port}/`;
  if (await waitForServer(port, 60000)) {
    vscode.commands.executeCommand("simpleBrowser.show", url);
  } else {
    vscode.window.showErrorMessage(`The page did not start on ${url}; see the "serve" terminal for the reason.`);
  }
}

/** Shows `web` or `native` for the active Kalinga file. */
function updateStatus(item) {
  const editor = vscode.window.activeTextEditor;
  if (!editor || editor.document.languageId !== LANG) {
    item.hide();
    vscode.commands.executeCommand("setContext", "kalinga.isWeb", false);
    return;
  }
  const t = targetOf(editor.document);
  item.text = t.web ? "$(globe) Kalinga: web" : "$(terminal) Kalinga: native";
  item.tooltip = `Builds ${t.web ? "for the web (JavaScript)" : "a native program"}: ${t.why}.${
    t.web ? " Click to serve the page." : ""
  }`;
  item.command = t.web ? "kalinga.serveWeb" : undefined;
  vscode.commands.executeCommand("setContext", "kalinga.isWeb", t.web);
  item.show();
}

async function activate(context) {
  context.subscriptions.push(
    vscode.commands.registerCommand("kalinga.run", runFile),
    vscode.commands.registerCommand("kalinga.test", testFile),
    vscode.commands.registerCommand("kalinga.build", buildFile),
    vscode.commands.registerCommand("kalinga.emit", () => emitCode(false)),
    vscode.commands.registerCommand("kalinga.emitJs", () => emitCode(true)),
    vscode.commands.registerCommand("kalinga.buildWeb", buildWeb),
    vscode.commands.registerCommand("kalinga.serveWeb", serveWeb),
    vscode.commands.registerCommand("kalinga.restartServer", () => restartClient(context)),
    vscode.workspace.onDidChangeConfiguration((e) => {
      if (e.affectsConfiguration("kalinga.compilerPath")) restartClient(context);
    }),
    vscode.tasks.onDidEndTask((e) => {
      if (serving && e.execution === serving) serving = undefined;
    }),
  );
  const status = vscode.window.createStatusBarItem(vscode.StatusBarAlignment.Right, 100);
  const refresh = () => updateStatus(status);
  context.subscriptions.push(
    status,
    vscode.window.onDidChangeActiveTextEditor(refresh),
    vscode.workspace.onDidSaveTextDocument(refresh),
    vscode.workspace.onDidChangeTextDocument((e) => {
      if (e.document === vscode.window.activeTextEditor?.document) refresh();
    }),
  );
  refresh();
  await startClient(context);
}

function deactivate() {
  return client ? client.stop() : undefined;
}

module.exports = { activate, deactivate };
