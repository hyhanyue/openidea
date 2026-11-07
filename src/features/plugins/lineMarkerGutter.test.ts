import { afterEach, expect, it, vi } from "vitest";
import { EditorState } from "@codemirror/state";
import { EditorView } from "@codemirror/view";
import type { EditorDocContext } from "@/editor/api";
import { addFileIconProvider, registerExternalIcons } from "@/ui";
import type { PluginEvent } from "@/bindings";
import { markerIcon, pluginLineMarkerGutter } from "./lineMarkerGutter";
const mock = vi.hoisted(() => ({
  markers: vi.fn(),
  event: undefined as ((event: PluginEvent) => void) | undefined,
  off: vi.fn(),
  modelChanged: undefined as (() => void) | undefined,
  offModel: vi.fn(),
}));
vi.mock(import("@/platform/ipc"), async (original) => {
  const actual = await original();
  return { ...actual, commands: { ...actual.commands, pluginLineMarkers: mock.markers } };
});
vi.mock("./backend", () => ({
  pluginsBackend: () => ({
    onEvent: (fn: (event: PluginEvent) => void) => {
      mock.event = fn;
      return mock.off;
    },
  }),
}));
vi.mock("@/editor/lsp/availability", () => ({
  onLspRefresh: (_kind: string, fn: () => void) => {
    mock.modelChanged = fn;
    return mock.offModel;
  },
}));
vi.mock("./navigationButton", () => ({ navigationButton: () => document.createElement("button") }));
let view: EditorView | undefined;
afterEach(() => {
  view?.destroy();
  view = undefined;
  vi.useRealTimers();
  vi.clearAllMocks();
});
it("withdraws old interactive markers on reload and does not refresh on idle status changes", async () => {
  vi.useFakeTimers();
  mock.markers.mockResolvedValue({
    status: "ok",
    data: [{ pluginId: "test", line: 0, icon: "navigate", tooltip: "old", target: null }],
  });
  view = new EditorView({
    parent: document.body,
    state: EditorState.create({ doc: "select", extensions: pluginLineMarkerGutter({ doc: 1 } as EditorDocContext) }),
  });
  await vi.advanceTimersByTimeAsync(450);
  expect(view.dom.querySelector(".cm-openidea-gutter-pluginMarkers")).not.toBeNull();
  const calls = mock.markers.mock.calls.length;
  mock.event!({ type: "commandsChanged" });
  await vi.advanceTimersByTimeAsync(450);
  expect(mock.markers).toHaveBeenCalledTimes(calls);
  mock.markers.mockResolvedValue({ status: "ok", data: [] });
  mock.event!({ type: "documentIndexesChanged" });
  expect(view.dom.querySelector(".cm-openidea-gutter-pluginMarkers")).toBeNull();
  await vi.advanceTimersByTimeAsync(450);
  expect(view.dom.querySelector(".cm-openidea-gutter-pluginMarkers")).toBeNull();
  view.destroy();
  view = undefined;
  expect(mock.off).toHaveBeenCalledOnce();
});

it("resolves only the marker plugin's own declared and registered file icons", () => {
  const name = "ext:fileIcon/mybatis/mapper";
  const off = registerExternalIcons([[name, '<svg viewBox="0 0 16 16"><path fill="aqua" d="M1 1h12v12H1z"/></svg>']]);
  try {
    expect(markerIcon("file-icon:mapper", "mybatis")).toBe(name);
    expect(markerIcon("file-icon:mapper", "other")).toBe("gutter-implementing");
    for (const icon of ["file-icon:missing", "file-icon:../mapper", name, "<svg onload=alert(1)>"]) {
      expect(markerIcon(icon, "mybatis")).toBe("gutter-implementing");
    }
  } finally {
    off();
  }
});

it("repaints fallback markers when icon assets arrive and withdraws them when unloaded", async () => {
  vi.useFakeTimers();
  mock.markers.mockResolvedValue({
    status: "ok",
    data: [{ pluginId: "mybatis", line: 0, icon: "file-icon:mapper", tooltip: "mapping", target: null }],
  });
  view = new EditorView({
    parent: document.body,
    state: EditorState.create({ doc: "select", extensions: pluginLineMarkerGutter({ doc: 1 } as EditorDocContext) }),
  });
  await vi.advanceTimersByTimeAsync(450);
  const marker = () => view!.dom.querySelector(".cm-plugin-mapping-button")!;
  expect(marker().innerHTML).not.toContain('fill="aqua"');
  const off = registerExternalIcons([
    ["ext:fileIcon/mybatis/mapper", '<svg viewBox="0 0 16 16"><path fill="aqua" d="M1 1h12v12H1z"/></svg>'],
  ]);
  const remove = addFileIconProvider(() => null);
  try {
    await vi.advanceTimersByTimeAsync(450);
    expect(marker().innerHTML).toContain('fill="aqua"');
    off();
    remove();
    await vi.advanceTimersByTimeAsync(450);
    expect(marker().innerHTML).not.toContain('fill="aqua"');
    expect(marker().innerHTML).toContain("svg");
  } finally {
    off();
    remove();
  }
});

it("drops a pending marker result when project plugin settings change", async () => {
  vi.useFakeTimers();
  const { usePluginSettingsRevision } = await import("./settingsRevision");
  let finish!: (value: unknown) => void;
  mock.markers.mockImplementationOnce(
    () =>
      new Promise((resolve) => {
        finish = resolve;
      }),
  );
  view = new EditorView({
    parent: document.body,
    state: EditorState.create({
      doc: "class Client {}",
      extensions: pluginLineMarkerGutter({ doc: 1 } as EditorDocContext),
    }),
  });
  await vi.advanceTimersByTimeAsync(450);
  usePluginSettingsRevision.setState((s) => ({ revision: s.revision + 1 }));
  finish({
    status: "ok",
    data: [{ pluginId: "test", line: 0, icon: "navigate", tooltip: "stale profile", target: null }],
  });
  mock.markers.mockResolvedValue({ status: "ok", data: [] });
  await vi.advanceTimersByTimeAsync(450);
  expect(view.dom.querySelector(".cm-plugin-mapping-button")).toBeNull();
  expect(mock.markers).toHaveBeenCalledTimes(2);
});

it("refreshes library markers when the asynchronous Java model becomes ready", async () => {
  vi.useFakeTimers();
  mock.markers.mockResolvedValue({ status: "ok", data: [] });
  view = new EditorView({
    parent: document.body,
    state: EditorState.create({ doc: "Port port", extensions: pluginLineMarkerGutter({ doc: 1 } as EditorDocContext) }),
  });
  await vi.advanceTimersByTimeAsync(450);
  expect(view.dom.querySelector(".cm-openidea-gutter-pluginMarkers")).toBeNull();
  mock.markers.mockResolvedValue({
    status: "ok",
    data: [
      {
        pluginId: "hyhanyue.spring-beans",
        line: 0,
        icon: "navigate",
        tooltip: "Injected bean: libraryPort (@Bean (dependency))",
        target: null,
      },
    ],
  });
  mock.modelChanged!();
  await vi.advanceTimersByTimeAsync(450);
  expect(view.dom.querySelector(".cm-openidea-gutter-pluginMarkers")).not.toBeNull();
  view.destroy();
  view = undefined;
  expect(mock.offModel).toHaveBeenCalled();
});
