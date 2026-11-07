import { afterEach, beforeEach, expect, it, vi } from "vitest";
import type { RunConfig } from "@/platform/ipc";
import { provideJavaAgentDebugger, provideNotificationService } from "@/workbench/contributions";
import { debugWithAgent } from "./javaDebug";
import { onRunExited, resetRuns, runtime, stopSession, useRun } from "./store";
import { sessionDetail } from "./services";

const ipc = vi.hoisted(() => ({
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
const prepare = vi.fn(async (_config: RunConfig, _module: string) => ({ attach }));
const notify = vi.fn();
let off: () => void;
beforeEach(() => {
  vi.clearAllMocks();
  attach.mockResolvedValue(true);
  prepare.mockResolvedValue({ attach });
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
