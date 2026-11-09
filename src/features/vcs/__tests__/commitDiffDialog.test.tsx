import { act, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { Settings } from "@/bindings";
import { useProject } from "@/platform/project";
import { provideEditorService } from "@/workbench/contributions";
import { executeAction, registerAction } from "@/platform/actions";
import { setContext } from "@/platform/context";
import { installKeyboardDispatcher, useKeymap } from "@/platform/keymap";
import { ContextMenuHost } from "@/ui";
import { registerVcsActions } from "../actions";
import { showCommitDialog } from "../CommitDialog";
import { ChangesTree } from "../CommitView";
import { useCommit } from "../commitStore";
import type { DiffData } from "../diffInput";
import { VcsDialogHost } from "../dialogs";
import { useVcs } from "../store";

const openInput = vi.fn(async () => undefined);
provideEditorService({ openInput } as never);
vi.mock("../DiffEditor", () => ({
  DiffEditor: ({ input }: { input: { data: DiffData } }) => (
    <button data-autofocus>{input.data.chain?.map((item) => item.path).join(", ")}</button>
  ),
}));

const file = (name: string) => ({
  path: `/p/${name}`,
  relPath: name,
  staged: null,
  unstaged: "modified" as const,
  renamedFrom: null,
});

beforeEach(() => {
  openInput.mockClear();
  useProject.setState({ settings: null });
  useVcs.setState({
    available: true,
    root: "/p",
    repo: "/p",
    roots: ["/p"],
    status: [file("Alpha.java"), file("Beta.java")],
  });
  useCommit.setState({ message: "", selection: [], checks: new Map(), signOff: false, groupByDir: false });
});

describe("Commit dialog diff", () => {
  afterEach(() => {
    const commit = screen.queryByRole("dialog", { name: "Commit Changes" });
    if (commit) fireEvent.click(within(commit).getByRole("button", { name: "Cancel" }));
  });

  it.each(["double-click", "F4", "Show Diff menu", "Show Diff action"])(
    "opens above Commit by %s and preserves its state and focus",
    async (entry) => {
      setContext("vcsAvailable", true);
      useProject.setState({ settings: { vcs: { commit: { nonModal: false } } } as Settings });
      const closeEditorTab = vi.fn();
      const dispose = [
        installKeyboardDispatcher(),
        registerVcsActions(),
        registerAction({ id: "editor.closeTab", title: "Close Tab", run: closeEditorTab }),
      ];
      useKeymap.setState({ preset: "intellij-macos", overrides: {} });
      try {
        render(
          <>
            <VcsDialogHost />
            <ContextMenuHost />
          </>,
        );
        act(() => showCommitDialog());
        const commit = screen.getByRole("dialog", { name: "Commit Changes" });
        const view = within(commit);
        const tree = view.getByRole("tree");
        const alpha = view.getByText("Alpha.java");
        const beta = view.getByRole("checkbox", { name: "Include Beta.java in commit" });
        const signOff = view.getByRole("checkbox", { name: "Sign-off commit" });
        const message = view.getByRole("textbox", { name: "Commit message" });
        fireEvent.change(message, { target: { value: "Draft commit message" } });
        fireEvent.click(signOff);
        fireEvent.mouseDown(beta, { button: 0 });
        fireEvent.mouseDown(alpha, { button: 0 });
        tree.focus();

        if (entry === "double-click") fireEvent.doubleClick(alpha);
        else if (entry === "F4") fireEvent.keyDown(tree, { key: "F4", code: "F4" });
        else if (entry === "Show Diff menu") {
          fireEvent.contextMenu(alpha);
          fireEvent.click(screen.getByRole("menuitem", { name: /Show Diff/ }));
        } else await act(() => executeAction("vcs.showDiff"));

        const diff = await screen.findByRole("dialog", { name: "Diff" });
        const diffContent = within(diff).getByRole("button", { name: "/p/Alpha.java, /p/Beta.java" });
        expect(diffContent).toHaveFocus();
        expect(openInput).not.toHaveBeenCalled();
        fireEvent.keyDown(diffContent, entry === "F4" ? { key: "w", code: "KeyW", metaKey: true } : { key: "Escape" });
        expect(closeEditorTab).not.toHaveBeenCalled();
        expect(diff).not.toBeInTheDocument();
        expect(screen.getByRole("dialog", { name: "Commit Changes" })).toBe(commit);
        expect(tree).toHaveFocus();
        expect(message).toHaveValue("Draft commit message");
        expect(beta).toHaveAttribute("aria-checked", "false");
        expect(signOff).toBeChecked();
        expect(alpha.closest('[role="treeitem"]')).toHaveAttribute("aria-selected", "true");
      } finally {
        dispose.forEach((off) => off());
      }
    },
  );

  it.each(["commit", "local"] as const)("keeps the %s tool window's editor preview", (mode) => {
    render(<ChangesTree mode={mode} />);
    fireEvent.doubleClick(screen.getByText("Alpha.java"));
    expect(openInput).toHaveBeenCalledWith(
      expect.objectContaining({ kind: "diff", data: expect.objectContaining({ path: "/p/Alpha.java" }) }),
      { preview: true, focus: true },
    );
  });
});
