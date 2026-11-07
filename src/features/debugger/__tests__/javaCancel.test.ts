import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { executeAction } from "@/platform/actions";
import { useProject } from "@/platform/project";
import { javaAgentDebugger, provideNotificationService, useDebugRunSessions } from "@/workbench/contributions";
import { IpcError, type RunConfig } from "@/platform/ipc";
import { registerDebugActions } from "../actions";
import { clearConsole, handleDebugEvent, removeSession, useDebug } from "../state";
import { debugWithAgent } from "../../run/javaDebug";
import { onRunExited, resetRuns, runtime, useRun } from "../../run/store";

const ipc = vi.hoisted(() => ({
  javaResolveLaunch: vi.fn(),
  runStart: vi.fn(async (_config?: unknown, _channel?: unknown) => ({ status: "ok", data: { id: 99, pty: false } })),
  runStop: vi.fn(async () => ({ status: "ok", data: null })),
  cancelRequest: vi.fn(async () => undefined),
  javaDebugServer: vi.fn(async () => ({ status: "ok", data: 5005 })),
  debugStart: vi.fn(async (_config?: unknown) => ({ status: "ok", data: 42 })),
  debugStop: vi.fn(async () => ({ status: "ok", data: null })),
  debugExceptionFilters: vi.fn(async () => ({ status: "ok", data: [] })),
  fsReadDir: vi.fn(async () => ({ status: "ok", data: [] })),
}));
vi.mock("@/platform/ipc", async (original) => {
  const real = await original<typeof import("@/platform/ipc")>();
  return { ...real, isTauri: () => true, commands: { ...real.commands, ...ipc } };
});
vi.mock("@tauri-apps/api/core", async (original) => ({
  ...(await original<typeof import("@tauri-apps/api/core")>()),
  Channel: class {
    onmessage = null;
  },
}));
const notify = vi.fn();
const expire = vi.fn();
const config: RunConfig = {
  id: "application:Main",
  name: "Main",
  kind: "application",
  command: "java",
  args: ["-cp", "${classpath}", "Main"],
  cwd: null,
  module: "",
  env: {},
  detected: true,
  isTest: false,
  debugAdapter: "java-debug",
};
let dispose: () => void;
let respond: (result: unknown) => void;
beforeEach(() => {
  vi.clearAllMocks();
  useDebug.setState({ sessions: {}, active: null, console: {} });
  useProject.setState({ project: { root: "/p" } as never });
  provideNotificationService({ notify, expire });
  ipc.javaResolveLaunch.mockImplementation(
    () =>
      new Promise((resolve) => {
        respond = resolve;
      }),
  );
  dispose = registerDebugActions();
});
afterEach(() => {
  dispose();
  useRun.setState({ sessions: [] });
  resetRuns();
});

async function startAndStop() {
  const starting = executeAction("debug.startConfig", config);
  await vi.waitFor(() => expect(ipc.javaResolveLaunch).toHaveBeenCalledOnce());
  const session = Object.values(useDebug.getState().sessions)[0]!;
  expect(session).toMatchObject({ name: "Main", status: "starting", runConfig: config });
  expect(
    useDebug
      .getState()
      .console[session.id]?.map((l) => l.text)
      .join("\n"),
  ).toContain("Building and resolving the classpath of Main…");
  expect(useDebugRunSessions.getState().sessions[0]?.configId).toBe(config.id);
  await executeAction("debug.stop");
  expect(ipc.cancelRequest).toHaveBeenCalledWith(ipc.javaResolveLaunch.mock.calls[0]![3]);
  expect(useDebug.getState().sessions[session.id]?.status).toBe("terminated");
  expect(useDebugRunSessions.getState().sessions).toEqual([]);
  return { starting };
}

function expectNoLaunch() {
  expect(ipc.javaDebugServer).not.toHaveBeenCalled();
  expect(ipc.debugStart).not.toHaveBeenCalled();
  expect(Object.values(useDebug.getState().sessions).every((s) => s.status === "terminated")).toBe(true);
  expect(notify).not.toHaveBeenCalled();
}

it("Stop cancels the resolve request and ends the preparation session without starting DAP", async () => {
  const { starting } = await startAndStop();
  respond({ status: "error", error: { kind: "cancelled", message: "Cancelled" } });
  await starting;
  expectNoLaunch();
});

it("ignores a successful resolve arriving after Stop, including compile warnings", async () => {
  const { starting } = await startAndStop();
  respond({ status: "ok", data: { errorCount: 1 } });
  await starting;
  expectNoLaunch();
});

it("ignores a failed resolve arriving after Stop", async () => {
  const { starting } = await startAndStop();
  respond({ status: "error", error: { kind: "internal", message: "Build failed" } });
  await starting;
  expectNoLaunch();
});

it("still reports a real resolve failure in the preparation console", async () => {
  const starting = executeAction("debug.startConfig", config);
  await vi.waitFor(() => expect(ipc.javaResolveLaunch).toHaveBeenCalledOnce());
  respond({ status: "error", error: { kind: "internal", message: "Build failed" } });
  await starting;
  expect(notify).toHaveBeenLastCalledWith(
    expect.objectContaining({
      level: "error",
      title: "Cannot start debugging",
      body: "Build failed",
    }),
  );
  expect(
    Object.values(useDebug.getState().console)
      .flat()
      .map((line) => line.text)
      .join("\n"),
  ).toContain("Build failed");
  expect(ipc.debugStart).not.toHaveBeenCalled();
});

it.each([
  "jdtls: Given URI does not belong to any Java project.",
  "jdtls: Launch configuration 1793460461218 references non-existing project parent.",
])("explains how to import a module when Debug fails with %s", async (message) => {
  const starting = executeAction("debug.startConfig", { ...config, module: "web" });
  await vi.waitFor(() => expect(ipc.javaResolveLaunch).toHaveBeenCalledOnce());
  respond({ status: "error", error: { kind: "protocol", message } });
  await starting;
  expect(notify).toHaveBeenLastCalledWith(
    expect.objectContaining({
      title: "Cannot start debugging",
      body: expect.stringContaining("Java module 'web' is not imported."),
    }),
  );
  expect(
    Object.values(useDebug.getState().console)
      .flat()
      .map((line) => line.text)
      .join("\n"),
  ).toContain("open its folder as a project");
  expect(ipc.javaDebugServer).not.toHaveBeenCalled();
  expect(ipc.debugStart).not.toHaveBeenCalled();
});

it("keeps progress and the originating configuration when DAP takes over", async () => {
  const starting = executeAction("debug.startConfig", config);
  await vi.waitFor(() => expect(ipc.javaResolveLaunch).toHaveBeenCalledOnce());
  respond({
    status: "ok",
    data: {
      build: "succeeded",
      errorCount: 0,
      errors: [],
      hints: [],
      classPaths: ["/p/target/classes"],
      modulePaths: [],
      pathListSeparator: ":",
      javaExec: "java",
    },
  });
  await starting;
  expect(Object.keys(useDebug.getState().sessions)).toEqual(["42"]);
  expect(useDebug.getState().sessions[42]).toMatchObject({ status: "running", runConfig: config });
  expect(
    useDebug
      .getState()
      .console[42]?.map((line) => line.text)
      .join("\n"),
  ).toContain("Building and resolving the classpath of Main…");
});

it.each(["success", "failure"])("handles a late DAP %s after preparation was cancelled", async (outcome) => {
  let fail!: (error: Error) => void;
  let finish!: (value: { status: "ok"; data: number }) => void;
  ipc.debugStart.mockImplementationOnce(
    () =>
      new Promise((resolve, reject) => {
        finish = resolve;
        fail = reject;
      }),
  );
  const starting = executeAction("debug.startConfig", config);
  await vi.waitFor(() => expect(ipc.javaResolveLaunch).toHaveBeenCalledOnce());
  respond({
    status: "ok",
    data: {
      build: "succeeded",
      errorCount: 0,
      errors: [],
      hints: [],
      classPaths: ["/p/target/classes"],
      modulePaths: [],
      pathListSeparator: ":",
      javaExec: "java",
    },
  });
  await vi.waitFor(() => expect(ipc.debugStart).toHaveBeenCalledOnce());
  await executeAction("debug.stop");
  if (outcome === "success") finish({ status: "ok", data: 42 });
  else fail(new Error("Adapter start failed"));
  await starting;
  if (outcome === "success") expect(ipc.debugStop).toHaveBeenCalledWith(42);
  expect(notify).not.toHaveBeenCalled();
  expect(useDebugRunSessions.getState().sessions).toEqual([]);
});

it("preflights the actual Java debugger and offers install before an owned JVM exists", async () => {
  ipc.javaDebugServer.mockRejectedValueOnce(
    new IpcError({ kind: "unavailable", message: "java-debug not loaded into jdtls" }),
  );
  await expect(javaAgentDebugger()!.prepare(config, "")).rejects.toThrow("java-debug not loaded");
  expect(ipc.debugStart).not.toHaveBeenCalled();
  expect(notify).toHaveBeenCalledWith(
    expect.objectContaining({
      title: "The Java debugger is not installed",
      actions: [expect.objectContaining({ label: "Install" })],
    }),
  );
});

it("cancels a late native attach and disposes its actual debug session", async () => {
  let respond!: (value: Awaited<ReturnType<typeof ipc.debugStart>>) => void;
  ipc.debugStart.mockImplementationOnce(
    () =>
      new Promise((resolve) => {
        respond = resolve;
      }),
  );
  const prepared = await javaAgentDebugger()!.prepare(config, "");
  expect(ipc.debugStart).not.toHaveBeenCalled();
  const controller = new AbortController();
  const attaching = prepared.attach(64947, 99, controller.signal);
  await vi.waitFor(() => expect(ipc.debugStart).toHaveBeenCalled());
  const launch = vi.mocked(ipc.debugStart).mock.calls[0]![0] as unknown as { extraJson: string };
  expect(JSON.parse(launch.extraJson)).toMatchObject({ debugServer: 5005, port: 64947 });
  controller.abort();
  respond({ status: "ok", data: 42 });
  expect(await attaching).toBe(false);
  expect(ipc.debugStop).toHaveBeenCalledWith(42);
  expect(useDebugRunSessions.getState().sessions).toEqual([]);
});

it("routes owned process output to its actual Debug console and ignores closed or unrelated sessions", async () => {
  ipc.debugStart.mockResolvedValueOnce({ status: "ok", data: 42 }).mockResolvedValueOnce({ status: "ok", data: 43 });
  const first = await javaAgentDebugger()!.prepare(config, "");
  const second = await javaAgentDebugger()!.prepare({ ...config, id: "prod", name: "prod" }, "");
  expect(await first.attach(65028, 99, new AbortController().signal)).toBe(true);
  expect(await second.attach(65029, 100, new AbortController().signal)).toBe(true);
  first.output(99, { kind: "stdout", text: "\x1b[32mBoot dev started\x1b[0m\n" });
  first.output(100, { kind: "stderr", text: "wrong run\n" });
  second.output(100, { kind: "stderr", text: "Boot prod error\n" });
  const text = (id: number) => (useDebug.getState().console[id] ?? []).map((line) => line.text).join("");
  await vi.waitFor(() => expect(text(42)).toBe("Boot dev started\n"));
  expect(text(43)).toBe("Boot prod error\n");
  expect(useDebug.getState().console[42]![0]!.spans[0]).toEqual({ text: "Boot dev started", fg: "var(--ansi-green)" });
  clearConsole(42);
  first.output(99, { kind: "stdout", text: "after clear\n" });
  await vi.waitFor(() => expect(text(42)).toBe("after clear\n"));
  first.output(99, { kind: "stdout", text: "queued before close\n" });
  removeSession(42);
  first.output(99, { kind: "stdout", text: "after close\n" });
  second.output(100, { kind: "stdout", text: "still prod\n" });
  await vi.waitFor(() => expect(text(43)).toContain("still prod"));
  expect(useDebug.getState().console[42]).toBeUndefined();
  handleDebugEvent(43, { kind: "terminated" });
  second.output(100, { kind: "stderr", text: "late terminated error\n" });
  await new Promise((done) => setTimeout(done, 30));
  expect(text(43)).not.toContain("late terminated error");
});

it("bridges real owned Run startup replay and live chunks into the actual Debug console store", async () => {
  await debugWithAgent({ ...config, kind: "spring-boot", includeSystemEnv: false, args: ["-cp", "lib/*", "Main"] });
  const channel = ipc.runStart.mock.calls[0]![1] as {
    onmessage(chunk: { kind: "stdout" | "stderr"; text: string }): void;
  };
  channel.onmessage({ kind: "stdout", text: "before attach\nListening for transport dt_socket at address: 64947\n" });
  await vi.waitFor(() => expect(ipc.debugStart).toHaveBeenCalledOnce());
  const consoleText = () => (useDebug.getState().console[42] ?? []).map((line) => line.text).join("");
  await vi.waitFor(() => expect(consoleText()).toContain("before attach\n"));
  channel.onmessage({ kind: "stdout", text: "Started Main in 1.25 seconds (JVM running for 2.5)\n" });
  channel.onmessage({ kind: "stderr", text: "live stderr\n" });
  await vi.waitFor(() => expect(consoleText()).toContain("live stderr\n"));
  expect(consoleText().match(/before attach/g)).toHaveLength(1);
  expect(consoleText()).toContain("Started Main in 1.25 seconds");
  const run = useRun.getState().sessions[0]!;
  expect(runtime(run.id)!.console.lines.some((line) => line.text === "live stderr")).toBe(true);
  onRunExited(99, 0);
  await vi.waitFor(() => expect(useRun.getState().sessions[0]!.status).toBe("finished"));
  const before = consoleText();
  channel.onmessage({ kind: "stdout", text: "late ended run output\n" });
  await new Promise((done) => setTimeout(done, 30));
  expect(consoleText()).toBe(before);
});
