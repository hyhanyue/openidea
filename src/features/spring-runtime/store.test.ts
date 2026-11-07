import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { connect, disconnect, refresh, useRuntime, watchProject } from "./store";
import { useProject } from "@/platform/project";
import type { ProjectInfo } from "@/bindings";
import type { Snapshot } from "./model";
const mocks = vi.hoisted(() => ({ invoke: vi.fn(), cancel: vi.fn() }));
vi.mock("@/platform/tauriCore", async (original) => ({
  ...(await original<typeof import("@/platform/tauriCore")>()),
  invokeLocal: (cmd: string, args: unknown) =>
    cmd === "cancel_request" ? mocks.cancel(args) : mocks.invoke(cmd, args),
}));
vi.mock("@/platform/ipc", async (original) => {
  const real = await original<typeof import("@/platform/ipc")>();
  return { ...real, commands: { ...real.commands, cancelRequest: mocks.cancel } };
});
const snapshot = Object.fromEntries(
  ["beans", "conditions", "env", "configprops", "mappings"].map((id) => [id, { state: "ok", rows: [] }]),
) as unknown as Snapshot;
beforeEach(() => {
  disconnect();
  vi.clearAllMocks();
  mocks.cancel.mockResolvedValue(null);
  mocks.invoke.mockResolvedValue(snapshot);
  useProject.setState({ project: { root: "/p" } as ProjectInfo });
});
afterEach(() => disconnect());
it("connects only explicitly and keeps authentication out of persisted UI state", async () => {
  await refresh();
  expect(mocks.invoke).not.toHaveBeenCalled();
  await connect("http://localhost:19187/actuator", "fixture-user", "synthetic-fixture-password");
  expect(mocks.invoke).toHaveBeenCalledWith(
    "spring_runtime_snapshot",
    expect.objectContaining({
      base: "http://localhost:19187/actuator",
      user: "fixture-user",
      password: "synthetic-fixture-password",
    }),
  );
  expect(useRuntime.getState().phase).toBe("connected");
  expect(JSON.stringify(useRuntime.getState())).not.toContain("synthetic-fixture-password");
  await refresh();
  expect(mocks.invoke.mock.calls[1]![1].password).toBe("synthetic-fixture-password");
  disconnect();
  await refresh();
  expect(mocks.invoke).toHaveBeenCalledTimes(2);
});
it("cancels disconnect and ignores an obsolete response", async () => {
  let finish!: (s: Snapshot) => void;
  mocks.invoke.mockImplementationOnce(
    () =>
      new Promise<Snapshot>((resolve) => {
        finish = resolve;
      }),
  );
  const connecting = connect("http://localhost:19187/actuator");
  disconnect();
  finish(snapshot);
  await connecting;
  expect(mocks.cancel).toHaveBeenCalledOnce();
  expect(useRuntime.getState()).toMatchObject({ phase: "disconnected", snapshot: null });
});
it("disconnects on project switch and does not surface raw transport errors", async () => {
  const stop = watchProject();
  await connect("http://localhost:19187/actuator");
  useProject.setState({ project: null });
  expect(useRuntime.getState().phase).toBe("disconnected");
  stop();
  mocks.invoke.mockRejectedValueOnce(new Error("synthetic sensitive transport details"));
  await connect("http://localhost:19187/actuator");
  expect(useRuntime.getState()).toMatchObject({ phase: "error", snapshot: null });
  expect(JSON.stringify(useRuntime.getState())).not.toContain("transport details");
});
