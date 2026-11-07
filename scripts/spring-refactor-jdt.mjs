/** Capture real JDT rename edits for the Java8 refactor fixture. Writes no source. */
import { spawn } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { pathToFileURL, fileURLToPath } from "node:url";

const [fixture, jdtls, jdkRuntime, jdk8, scratch, output] = process.argv.slice(2).map((p) => path.resolve(p));
if (!output) throw new Error("Usage: node spring-refactor-jdt.mjs fixture jdtls jdkRuntime jdk8 scratch output.json");
fs.mkdirSync(scratch, { recursive: true });
const config = path.join(scratch, "config");
fs.cpSync(path.join(jdtls, "config_mac"), config, { recursive: true });
const launcher = fs
  .readdirSync(path.join(jdtls, "plugins"))
  .find((p) => /^org\.eclipse\.equinox\.launcher_[\d].*\.jar$/.test(p));
const settings = {
  java: {
    configuration: {
      runtimes: [{ name: "JavaSE-1.8", path: jdk8, default: true }],
      updateBuildConfiguration: "automatic",
    },
    import: { maven: { enabled: true }, gradle: { enabled: false } },
    autobuild: { enabled: true },
  },
};
const log = fs.openSync(path.join(scratch, "stderr.log"), "w");
const server = spawn(
  path.join(jdkRuntime, "bin/java"),
  [
    "-Declipse.application=org.eclipse.jdt.ls.core.id1",
    "-Declipse.product=org.eclipse.jdt.ls.core.product",
    `-Dosgi.install.area=${jdtls}`,
    "-Dosgi.requiredJavaVersion=21",
    "-Xmx512m",
    "--add-modules=ALL-SYSTEM",
    "--add-opens",
    "java.base/java.util=ALL-UNNAMED",
    "--add-opens",
    "java.base/java.lang=ALL-UNNAMED",
    "-jar",
    path.join(jdtls, "plugins", launcher),
    "-configuration",
    config,
    "-data",
    path.join(scratch, "workspace"),
  ],
  { stdio: ["pipe", "pipe", log] },
);
const pending = new Map();
let seq = 0;
let buffer = Buffer.alloc(0);
let ready = false;
function send(message) {
  const body = Buffer.from(JSON.stringify(message));
  server.stdin.write(`Content-Length: ${body.length}\r\n\r\n`);
  server.stdin.write(body);
}
function request(method, params, timeout = 120_000) {
  const id = ++seq;
  return new Promise((resolve, reject) => {
    const timer = setTimeout(() => {
      pending.delete(id);
      reject(new Error(`Timeout: ${method}`));
    }, timeout);
    pending.set(id, {
      resolve: (r) => {
        clearTimeout(timer);
        resolve(r);
      },
      reject: (e) => {
        clearTimeout(timer);
        reject(e);
      },
    });
    send({ jsonrpc: "2.0", id, method, params });
  });
}
server.stdout.on("data", (data) => {
  buffer = Buffer.concat([buffer, data]);
  while (true) {
    const head = buffer.indexOf("\r\n\r\n");
    if (head < 0) break;
    const match = /Content-Length:\s*(\d+)/i.exec(buffer.subarray(0, head).toString());
    if (!match) throw new Error("Invalid LSP header");
    const count = Number(match[1]);
    if (buffer.length < head + 4 + count) break;
    const message = JSON.parse(buffer.subarray(head + 4, head + 4 + count).toString());
    buffer = buffer.subarray(head + 4 + count);
    if (message.method && message.id != null) {
      let result = null;
      if (message.method === "workspace/configuration")
        result = message.params.items.map(
          ({ section }) => section?.split(".").reduce((v, k) => v?.[k], settings) ?? settings,
        );
      if (message.method === "workspace/workspaceFolders")
        result = [{ uri: pathToFileURL(fixture).href, name: "semantic-fixture" }];
      if (message.method === "workspace/applyEdit")
        result = { applied: false, failureReason: "Capture only; workspace transaction owns source writes" };
      send({ jsonrpc: "2.0", id: message.id, result });
    } else if (message.id != null) {
      const promise = pending.get(message.id);
      pending.delete(message.id);
      if (message.error) promise?.reject(new Error(JSON.stringify(message.error)));
      else promise?.resolve(message.result);
    } else if (message.method === "language/status") {
      process.stdout.write(`${JSON.stringify(message.params)}\n`);
      if (message.params.type === "ServiceReady") ready = true;
    }
  }
});
server.on("exit", (code) => {
  for (const p of pending.values()) p.reject(new Error(`JDT exited ${code}`));
  pending.clear();
});
try {
  const rootUri = pathToFileURL(fixture).href;
  await request("initialize", {
    processId: process.pid,
    rootUri,
    workspaceFolders: [{ uri: rootUri, name: "semantic-fixture" }],
    capabilities: {
      general: { positionEncodings: ["utf-16"] },
      workspace: {
        configuration: true,
        workspaceFolders: true,
        workspaceEdit: { documentChanges: true, resourceOperations: ["rename"], failureHandling: "transactional" },
      },
      textDocument: { rename: { prepareSupport: true } },
    },
    initializationOptions: { settings },
  });
  send({ jsonrpc: "2.0", method: "initialized", params: {} });
  send({ jsonrpc: "2.0", method: "workspace/didChangeConfiguration", params: { settings } });
  const deadline = Date.now() + 180_000;
  while (!ready && Date.now() < deadline) await new Promise((r) => setTimeout(r, 1000));
  if (!ready) throw new Error("JDT did not finish importing fixture");
  const java = path.join(fixture, "src/main/java/dev/openidea/refactor/UserMapper.java");
  const uri = pathToFileURL(java).href;
  const text = fs.readFileSync(java, "utf8");
  send({
    jsonrpc: "2.0",
    method: "textDocument/didOpen",
    params: { textDocument: { uri, languageId: "java", version: 1, text } },
  });
  const index = text.indexOf("find(");
  const prefix = text.slice(0, index).split("\n");
  const position = { line: prefix.length - 1, character: prefix.at(-1).length };
  const prepared = await request("textDocument/prepareRename", { textDocument: { uri }, position });
  if (!prepared) throw new Error("JDT refused mapper method rename");
  const edit = await request("textDocument/rename", { textDocument: { uri }, position, newName: "lookup" });
  const files = [];
  const edits = (fileUri, values) =>
    files.push({
      kind: "edit",
      path: fileURLToPath(fileUri),
      edits: values.map((e) => ({
        range: {
          start: { line: e.range.start.line, col: e.range.start.character },
          end: { line: e.range.end.line, col: e.range.end.character },
        },
        new_text: e.newText,
      })),
    });
  for (const [fileUri, values] of Object.entries(edit.changes ?? {})) edits(fileUri, values);
  for (const change of edit.documentChanges ?? []) {
    if (!change.textDocument) throw new Error(`Unexpected resource operation: ${change.kind}`);
    edits(change.textDocument.uri, change.edits);
  }
  if (files.length < 2) throw new Error("JDT did not include the Java caller");
  fs.writeFileSync(output, JSON.stringify({ files, preconditions: [] }, null, 2));
  process.stdout.write(`Captured real JDT rename: ${files.length} Java files -> ${output}\n`);
  await request("shutdown", null, 5000);
  send({ jsonrpc: "2.0", method: "exit", params: null });
} finally {
  server.kill("SIGTERM");
  fs.closeSync(log);
}
