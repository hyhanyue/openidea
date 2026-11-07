import { create } from "zustand";
import { invokeLocal } from "@/platform/tauriCore";
import { nextRequestId } from "@/platform/ipc";
import { useProject } from "@/platform/project";
import type { Snapshot } from "./model";

interface State {
  base: string;
  phase: "disconnected" | "connecting" | "connected" | "error";
  snapshot: Snapshot | null;
}
export const useRuntime = create<State>(() => ({ base: "", phase: "disconnected", snapshot: null }));
let generation = 0;
let pending: ReturnType<typeof nextRequestId> | null = null;
// Credentials live only in this window's memory until disconnect / project change.
let credentials = { user: "", password: "" };

export function disconnect() {
  generation++;
  if (pending !== null) void invokeLocal("cancel_request", { request: pending }).catch(() => {});
  pending = null;
  credentials = { user: "", password: "" };
  useRuntime.setState({ base: "", phase: "disconnected", snapshot: null });
}
export async function connect(base: string, user = "", password = "") {
  disconnect();
  credentials = { user, password };
  useRuntime.setState({ base, phase: "connecting" });
  await refresh();
}
export async function refresh() {
  const base = useRuntime.getState().base;
  if (!base) return;
  const token = ++generation;
  if (pending !== null) void invokeLocal("cancel_request", { request: pending }).catch(() => {});
  const request = nextRequestId();
  pending = request;
  useRuntime.setState({ phase: "connecting", snapshot: null });
  try {
    // Independent adapter while the unified native build regenerates the shared bindings.
    const snapshot = await invokeLocal<Snapshot>("spring_runtime_snapshot", { request, base, ...credentials });
    if (generation === token) useRuntime.setState({ snapshot, phase: "connected" });
  } catch {
    // Never surface transport error text: it can contain request/authentication details.
    if (generation === token) useRuntime.setState({ snapshot: null, phase: "error" });
  } finally {
    if (generation === token) pending = null;
  }
}

export function watchProject(): () => void {
  return useProject.subscribe((state, previous) => {
    if (state.project?.root !== previous.project?.root) disconnect();
  });
}
