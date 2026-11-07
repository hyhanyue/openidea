import { afterEach, beforeEach, expect, it, vi } from "vitest";
import type { RunConfig } from "@/platform/ipc";
import { provideJavaAgentDebugger, provideNotificationService } from "@/workbench/contributions";
import { debugWithAgent } from "./javaDebug";
import { onRunExited, resetRuns, runtime, startRun, closeSession, stopSession, useRun } from "./store";
import { sessionDetail } from "./services";

const ipc = vi.hoisted(() => ({
  channels: [] as Array<{ onmessage: ((chunk: { kind: "stdout" | "stderr"; text: string }) => void) | null }>,
  runStart: vi.fn(async () => ({ status: "ok" as const, data: { id: 99, pty: false } })),
  runStop: vi.fn(async (_id: number) => ({ status: "ok" as const, data: null })),
}));
vi.mock("@/platform/ipc", async (original) => ({
  ...(await original<typeof import("@/platform/ipc")>()),
  isTauri: () => true,
  commands: { ...(await original<typeof import("@/platform/ipc")>()).commands, ...ipc },
}));
vi.mock("@tauri-apps/api/core", async (original) => ({
  ...(await original<typeof import("@tauri-apps/api/core")>()),
  Channel: class {
    onmessage = null;
    constructor() {
      ipc.channels.push(this);
    }
  },
}));
const config: RunConfig = {
  id: "dev",
  name: "Spring dev",
  kind: "spring-boot",
  command: "/jdk8/bin/java",
  args: ["-cp", "lib/*", "a.Main"],
  cwd: null,
  module: "",
  env: {},
  detected: false,
  isTest: false,
  debugAdapter: "java-debug",
  includeSystemEnv: false,
};
const attach = vi.fn(async (_port: number, _process: number, _signal: AbortSignal): Promise<boolean> => true);
const output = vi.fn();
const prepare = vi.fn(async (_config: RunConfig, _module: string) => ({ attach, output }));
const notify = vi.fn();
let off: () => void;
beforeEach(() => {
  vi.clearAllMocks();
  ipc.channels.length = 0;
  attach.mockResolvedValue(true);
  prepare.mockResolvedValue({ attach, output });
  ipc.runStop.mockImplementation(async (id) => {
    setTimeout(() => onRunExited(id, 130), 0);
    return { status: "ok", data: null };
  });
  provideNotificationService({ notify });
  off = provideJavaAgentDebugger({ prepare });
});
afterEach(() => {
  vi.useRealTimers();
  off();
  useRun.setState({ sessions: [] });
  resetRuns();
});
const current = () => useRun.getState().sessions.find((s) => s.config.id === config.id)!;
const announce = (port = 64947) =>
  runtime(current().id)!.console.append({
    kind: "stdout",
    text: `Listening for transport dt_socket at address: ${port}\n`,
  });

it("probes availability before starting a suspended JVM", async () => {
  prepare.mockRejectedValueOnce(new Error("java-debug not loaded into jdtls"));
  expect(await debugWithAgent(config)).toBe(true);
  expect(ipc.runStart).not.toHaveBeenCalled();
  expect(attach).not.toHaveBeenCalled();
  expect(useRun.getState().sessions).toEqual([]);
});
it("attaches successfully once per port and retains the owned JVM", async () => {
  await debugWithAgent(config);
  expect(prepare).toHaveBeenCalledWith(config, "");
  expect(current().debugAttachment).toBe("waiting");
  announce();
  announce();
  await vi.waitFor(() => expect(current().debugAttachment).toBe("attached"));
  expect(attach).toHaveBeenCalledExactlyOnceWith(64947, 99, expect.any(AbortSignal));
  expect(ipc.runStop).not.toHaveBeenCalled();
});
it.each(["false", "error", "cancel"])("stops only the owned RunId on attach %s", async (mode) => {
  useRun.setState({
    sessions: [
      {
        id: "other",
        config: { ...config, id: "other" },
        runId: 77,
        status: "running",
        exitCode: null,
        startedAt: 0,
        endedAt: null,
        stdinClosed: false,
      },
    ],
  });
  if (mode === "false") attach.mockResolvedValueOnce(false);
  else
    attach.mockRejectedValueOnce(
      mode === "cancel" ? new DOMException("cancelled", "AbortError") : new Error("attach failed"),
    );
  await debugWithAgent(config);
  announce();
  await vi.waitFor(() => expect(current().status).toBe("finished"));
  expect(ipc.runStop).toHaveBeenCalledExactlyOnceWith(99);
  expect(current().debugAttachment).toBe("failed");
  expect(useRun.getState().sessions.find((s) => s.id === "other")?.status).toBe("running");
});
it("keeps failed Stop visibly live and available to retry", async () => {
  attach.mockResolvedValueOnce(false);
  ipc.runStop.mockRejectedValueOnce(new Error("stop failed"));
  await debugWithAgent(config);
  announce();
  await vi.waitFor(() =>
    expect(notify).toHaveBeenCalledWith(
      expect.objectContaining({ body: "This JVM is still running. Stop it before retrying Debug." }),
    ),
  );
  expect(current()).toMatchObject({ status: "running", debugAttachment: "failed" });
  expect(current().stopped).not.toBe(true);
  expect(sessionDetail(current())).toBe("Debugger attachment failed; JVM is still running");
  await stopSession(current().id);
  await vi.waitFor(() => expect(current().status).toBe("finished"));
});
it("ignores a late attach failure after the tab owns a different RunId", async () => {
  let respond!: (value: boolean) => void;
  attach.mockImplementationOnce(
    () =>
      new Promise((resolve) => {
        respond = resolve;
      }),
  );
  await debugWithAgent(config);
  announce();
  await vi.waitFor(() => expect(attach).toHaveBeenCalled());
  const signal = attach.mock.calls[0]![2];
  useRun.setState((state) => ({
    sessions: state.sessions.map((s) => ({ ...s, runId: 100, debugAttachment: undefined })),
  }));
  expect(signal.aborted).toBe(true);
  respond(false);
  await Promise.resolve();
  await Promise.resolve();
  expect(ipc.runStop).not.toHaveBeenCalled();
  expect(current()).toMatchObject({ runId: 100, status: "running", debugAttachment: undefined });
});
it("cancels an in-flight attach when the user stops its owned run", async () => {
  let respond!: (value: boolean) => void;
  attach.mockImplementationOnce(
    () =>
      new Promise((resolve) => {
        respond = resolve;
      }),
  );
  await debugWithAgent(config);
  announce();
  await vi.waitFor(() => expect(attach).toHaveBeenCalled());
  await stopSession(current().id);
  expect(attach.mock.calls[0]![2].aborted).toBe(true);
  respond(false);
  await vi.waitFor(() => expect(current().status).toBe("finished"));
  expect(ipc.runStop).toHaveBeenCalledExactlyOnceWith(99);
});

it("releases a JVM if adapter attachment never completes", async () => {
  vi.useFakeTimers();
  attach.mockImplementationOnce(() => new Promise(() => {}));
  await debugWithAgent(config);
  announce();
  await vi.advanceTimersByTimeAsync(30_100);
  expect(ipc.runStop).toHaveBeenCalledExactlyOnceWith(99);
  expect(current()).toMatchObject({ status: "finished", debugAttachment: "failed" });
});

const emit = (kind: "stdout" | "stderr", text: string, channel = ipc.channels.length - 1) =>
  ipc.channels[channel]!.onmessage?.({ kind, text });

it("replays pre-attach stdout/stderr once, then streams partial chunks while preserving Run output", async () => {
  let respond!: (value: boolean) => void;
  attach.mockImplementationOnce(
    () =>
      new Promise((resolve) => {
        respond = resolve;
      }),
  );
  await debugWithAgent(config);
  emit("stdout", "before attach\n");
  emit("stderr", "early error\n");
  emit("stdout", "Listening for transport dt_socket at address: 64947\n");
  await vi.waitFor(() => expect(attach).toHaveBeenCalled());
  emit("stdout", "while attaching\n");
  respond(true);
  await vi.waitFor(() => expect(current().debugAttachment).toBe("attached"));
  expect(output.mock.calls).toEqual([
    [99, { kind: "stdout", text: "before attach\n" }],
    [99, { kind: "stderr", text: "early error\n" }],
    [99, { kind: "stdout", text: "Listening for transport dt_socket at address: 64947\n" }],
    [99, { kind: "stdout", text: "while attaching\n" }],
  ]);
  const count = output.mock.calls.length;
  announce();
  expect(output).toHaveBeenCalledTimes(count);
  emit("stdout", "partial stdout");
  emit("stderr", "partial stderr");
  expect(output).toHaveBeenLastCalledWith(99, { kind: "stderr", text: "partial stderr" });
  await vi.waitFor(() =>
    expect(runtime(current().id)!.console.lines.some((line) => line.text.includes("partial stderr"))).toBe(true),
  );
  expect(runtime(current().id)!.console.lines.some((line) => line.text === "before attach")).toBe(true);
});

it("detaches old output on Stop and replays only the restarted generation", async () => {
  await debugWithAgent(config);
  emit("stdout", "old startup\n");
  announce();
  await vi.waitFor(() => expect(current().debugAttachment).toBe("attached"));
  const firstChannel = 0;
  await stopSession(current().id);
  const count = output.mock.calls.length;
  emit("stdout", "after Stop\n", firstChannel);
  expect(output).toHaveBeenCalledTimes(count);
  await vi.waitFor(() => expect(current().status).toBe("finished"));
  ipc.runStart.mockResolvedValueOnce({ status: "ok", data: { id: 100, pty: false } });
  await debugWithAgent(config);
  output.mockClear();
  emit("stdout", "new startup\n");
  announce(65028);
  await vi.waitFor(() => expect(current().debugAttachment).toBe("attached"));
  expect(output.mock.calls).toEqual([[100, { kind: "stdout", text: "new startup\n" }]]);
  emit("stderr", "old late error\n", firstChannel);
  expect(output.mock.calls).toEqual([[100, { kind: "stdout", text: "new startup\n" }]]);
});

it("keeps output arriving during startup replay ordered and unique", async () => {
  await debugWithAgent(config);
  emit("stdout", "history one\n");
  emit("stderr", "history two\n");
  output.mockImplementationOnce(() => emit("stdout", "live during replay\n"));
  announce();
  await vi.waitFor(() => expect(current().debugAttachment).toBe("attached"));
  expect(output.mock.calls).toEqual([
    [99, { kind: "stdout", text: "history one\n" }],
    [99, { kind: "stderr", text: "history two\n" }],
    [99, { kind: "stdout", text: "live during replay\n" }],
  ]);
});

it("bounds replay memory and does not capture ordinary Run output", async () => {
  await debugWithAgent(config);
  emit("stdout", "x".repeat(2 * 1024 * 1024));
  expect(runtime(current().id)!.output!.characters).toBeLessThanOrEqual(1024 * 1024);
  announce();
  await vi.waitFor(() => expect(current().debugAttachment).toBe("attached"));
  expect(output.mock.calls[0]).toEqual([99, { kind: "stdout", text: "x".repeat(1024 * 1024) }]);
  const ordinary = await startRun({ ...config, id: "ordinary", includeSystemEnv: true });
  expect(runtime(ordinary!.id)!.output).toBeUndefined();
});

it("disconnects the output bridge when its Run tab closes", async () => {
  await debugWithAgent(config);
  emit("stdout", "before close\n");
  announce();
  await vi.waitFor(() => expect(current().debugAttachment).toBe("attached"));
  const id = current().id;
  closeSession(id);
  const count = output.mock.calls.length;
  emit("stderr", "late closed output\n", 0);
  expect(output).toHaveBeenCalledTimes(count);
  expect(useRun.getState().sessions.some((s) => s.id === id)).toBe(false);
  await new Promise((done) => setTimeout(done, 50));
});
