import { beforeEach, expect, it, vi } from "vitest";
import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { RuntimePanel } from "./RuntimePanel";
import { disconnect, useRuntime } from "./store";
import { useProject } from "@/platform/project";
import type { ProjectInfo } from "@/bindings";
import { endpoints, type Snapshot } from "./model";
const mocks = vi.hoisted(() => ({ invoke: vi.fn(), target: vi.fn(), cancel: vi.fn().mockResolvedValue(null) }));
vi.mock("@/platform/tauriCore", async (original) => ({
  ...(await original<typeof import("@/platform/tauriCore")>()),
  invokeLocal: (cmd: string, args: unknown) =>
    cmd === "cancel_request" ? mocks.cancel(args) : mocks.invoke(cmd, args),
}));
vi.mock("./navigation", () => ({ sourceTarget: mocks.target }));
const snapshot = () =>
  Object.fromEntries(endpoints.map((id) => [id, { state: "ok", rows: [] }])) as unknown as Snapshot;
beforeEach(() => {
  disconnect();
  vi.clearAllMocks();
  useProject.setState({ project: { root: "/p" } as ProjectInfo });
  mocks.invoke.mockResolvedValue(snapshot());
});
it("does not connect on opening the panel, connects on submit and clears the password input", async () => {
  render(<RuntimePanel />);
  expect(mocks.invoke).not.toHaveBeenCalled();
  fireEvent.change(screen.getByLabelText("Runtime username (optional)"), { target: { value: "synthetic user" } });
  fireEvent.change(screen.getByLabelText("Runtime password (memory only)"), {
    target: { value: "synthetic password" },
  });
  fireEvent.click(screen.getByRole("button", { name: "Connect" }));
  await waitFor(() => expect(useRuntime.getState().phase).toBe("connected"));
  expect(mocks.invoke).toHaveBeenCalledOnce();
  expect(screen.getByLabelText("Runtime password (memory only)")).toHaveValue("");
  fireEvent.click(screen.getByRole("button", { name: "Disconnect" }));
  expect(useRuntime.getState().phase).toBe("disconnected");
  expect(screen.getByLabelText("Runtime username (optional)")).toHaveValue("");
});
it("distinguishes permission, disabled and version errors from an empty result", () => {
  const data = snapshot();
  data.beans = { state: "unauthorized", rows: [] };
  data.env = { state: "disabled", rows: [] };
  data.configprops = { state: "incompatible", rows: [] };
  useRuntime.setState({ phase: "connected", snapshot: data });
  render(<RuntimePanel />);
  expect(screen.getByRole("status")).toHaveTextContent("Authentication required (401)");
  expect(screen.queryByText("No runtime facts match this filter")).not.toBeInTheDocument();
  fireEvent.change(screen.getByLabelText("Runtime endpoint"), { target: { value: "env" } });
  expect(screen.getByRole("status")).toHaveTextContent("Endpoint unavailable (404)");
  fireEvent.change(screen.getByLabelText("Runtime endpoint"), { target: { value: "configprops" } });
  expect(screen.getByRole("status")).toHaveTextContent("Unsupported Actuator response structure or version");
});
it("shows masked keys and refuses unresolved source navigation", async () => {
  const data = snapshot();
  data.env.rows = [{ name: "fixture.demo.value", context: "app", detail: "application.properties", masked: true }];
  data.beans.rows = [
    { name: "demoBean", context: "app", detail: "singleton", className: "com.example.Demo", dependencies: ["repo"] },
  ];
  useRuntime.setState({ phase: "connected", snapshot: data });
  mocks.target.mockResolvedValue(null);
  render(<RuntimePanel />);
  await act(async () => fireEvent.click(screen.getByRole("button", { name: "Locate runtime type source" })));
  expect(screen.getByRole("status")).toHaveTextContent("Source identity is unresolved or ambiguous");
  fireEvent.change(screen.getByLabelText("Runtime endpoint"), { target: { value: "env" } });
  expect(screen.getByText("Value hidden")).toBeInTheDocument();
  expect(screen.getByText("fixture.demo.value")).toBeInTheDocument();
  fireEvent.change(screen.getByLabelText("Filter runtime facts"), { target: { value: "absent" } });
  expect(screen.getByText("No runtime facts match this filter")).toBeInTheDocument();
});
