import { EditorState } from "@codemirror/state";
import { EditorView } from "@codemirror/view";
import { afterEach, describe, expect, it, vi } from "vitest";
import { _addEditor, _removeEditor, _setActiveEditor, type TextEditorHandle } from "@/editor/api";
import { commands, emitLocalIdeEvent } from "@/platform/ipc";
import { ipcPath } from "@/platform/paths";
import { useProject } from "@/platform/project";
import { showContextMenu, type MenuItem } from "@/ui";
import { generateAtCaret, springEditorExtension, springMenuItems } from "./editor";
import { registerSpringEndpointProvider, routeIndex } from "./routeIndex";

vi.mock("@/ui", async (original) => ({ ...(await original<typeof import("@/ui")>()), showContextMenu: vi.fn() }));
const scan = vi.spyOn(commands, "springRouteSources");
const source = { path: "/p/C.java", text: "@RestController\nclass C implements Api {\n public String get(){}\n}" };
const api = (path: string) => ({
  path: "/p/Api.java",
  text: `interface Api { @RequestMapping(path={"${path}","/second"},method={RequestMethod.POST,RequestMethod.PUT}) String get(); }`,
});
const views: EditorView[] = [];
function choices(item: MenuItem | undefined): string[] {
  return item && "submenu" in item ? (item.submenu?.map((x) => ("label" in x ? (x.label ?? "") : "")) ?? []) : [];
}
let dispose: (() => void) | undefined;
function editor(path: string, text: string) {
  const parent = document.createElement("div");
  document.body.append(parent);
  const view = new EditorView({
    parent,
    state: EditorState.create({
      doc: text,
      extensions: [springEditorExtension({ path, doc: 1, language: "java", uiMode: null, readOnly: false })],
    }),
  });
  views.push(view);
  return { view, parent };
}
afterEach(() => {
  views.splice(0).forEach((v) => v.destroy());
  dispose?.();
  dispose = undefined;
  useProject.setState({ project: null });
  document.body.innerHTML = "";
  scan.mockReset();
  vi.mocked(showContextMenu).mockClear();
});

describe("editor consumers of the shared Spring model", () => {
  it("loads inherited contracts for gutter and menus, then follows unsaved dependency edits", async () => {
    useProject.setState({ project: { root: "/p" } as never });
    dispose = registerSpringEndpointProvider();
    scan.mockResolvedValue({ status: "ok", data: [source, api("/old")] });
    const { parent } = editor(source.path, source.text);
    await expect.poll(() => parent.querySelectorAll(".cm-openidea-gutter-spring button").length).toBe(2);
    const choice = springMenuItems(source.path, source.text, source.text.indexOf("get()"))[0]!;
    expect(choices(choice)).toEqual(["POST /old", "POST /second", "PUT /old", "PUT /second"]);
    (parent.querySelectorAll(".cm-openidea-gutter-spring button")[1] as HTMLButtonElement).click();
    await expect.poll(() => choices(vi.mocked(showContextMenu).mock.calls[0]?.[1][0])).toEqual(choices(choice));
    const dependency = editor("/p/Api.java", api("/old").text);
    dependency.view.dispatch({
      changes: { from: 0, to: dependency.view.state.doc.length, insert: api("/unsaved").text },
    });
    await expect
      .poll(() => choices(springMenuItems(source.path, source.text, source.text.indexOf("get()"))[0])[0])
      .toBe("POST /unsaved");
    expect((await routeIndex("/p"))[0]?.url).toBe("/unsaved");
    expect(scan).toHaveBeenCalledTimes(1);
  });
  it("updates from filesystem revisions and never dispatches a delayed scan into a destroyed editor", async () => {
    useProject.setState({ project: { root: "/p" } as never });
    dispose = registerSpringEndpointProvider();
    scan.mockResolvedValue({ status: "ok", data: [source, api("/old")] });
    const { view, parent } = editor(source.path, source.text);
    await expect.poll(() => parent.querySelectorAll(".cm-openidea-gutter-spring button").length).toBe(2);
    scan.mockResolvedValue({ status: "ok", data: [source, api("/disk")] });
    emitLocalIdeEvent({ type: "fsChanged", created: [], modified: [ipcPath("/p/Api.java")], removed: [] });
    await expect
      .poll(() => choices(springMenuItems(source.path, source.text, source.text.indexOf("get()"))[0])[0])
      .toBe("POST /disk");
    let finish!: (value: { status: "ok"; data: (typeof source)[] }) => void;
    scan.mockImplementationOnce(
      () =>
        new Promise((resolve) => {
          finish = resolve;
        }),
    );
    emitLocalIdeEvent({ type: "fsChanged", created: [], modified: [ipcPath("/p/Api.java")], removed: [] });
    await expect.poll(() => !!finish).toBe(true);
    view.destroy();
    views.splice(views.indexOf(view), 1);
    const dispatch = vi.spyOn(view, "dispatch");
    finish({ status: "ok", data: [source, api("/late")] });
    await new Promise((resolve) => setTimeout(resolve, 30));
    expect(dispatch).not.toHaveBeenCalled();
  });

  it("abandons request generation when the document changes during a pending project scan", async () => {
    useProject.setState({ project: { root: "/p" } as never });
    dispose = registerSpringEndpointProvider();
    let finish!: (value: { status: "ok"; data: (typeof source)[] }) => void;
    scan.mockImplementationOnce(
      () =>
        new Promise((resolve) => {
          finish = resolve;
        }),
    );
    const parent = document.createElement("div");
    document.body.append(parent);
    const view = new EditorView({
      parent,
      state: EditorState.create({ doc: source.text, selection: { anchor: source.text.indexOf("get()") } }),
    });
    views.push(view);
    const handle = {
      view,
      ctx: { path: source.path, language: "java", doc: 1, uiMode: null, readOnly: false },
    } as TextEditorHandle;
    _addEditor(handle);
    _setActiveEditor(handle);
    const create = vi.spyOn(commands, "fsCreateFile");
    try {
      const pending = generateAtCaret("endpoint");
      view.dispatch({ changes: { from: 0, insert: "// edited\n" } });
      finish({ status: "ok", data: [source, api("/old")] });
      await pending;
      expect(create).not.toHaveBeenCalled();
    } finally {
      _removeEditor(handle);
      _setActiveEditor(null);
      create.mockRestore();
    }
  });
});
