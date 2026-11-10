import { beforeEach, describe, expect, it, vi } from "vitest";
import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { AgentCommand, AgentConfigOption, AgentProvider } from "@/bindings";
import codexTranscript from "../../../crates/ide-agent/tests/fixtures/codex/simple.jsonl?raw";

const agentCommands = vi.fn();
const agentCreate = vi.fn();
const vcsBranches = vi.fn();
const vcsBranchInfo = vi.fn();
const clipboardImage = vi.fn();

vi.mock("@/platform/ipc", async (importOriginal) => {
  const real = await importOriginal<typeof import("@/platform/ipc")>();
  return {
    ...real,
    isTauri: vi.fn(() => true),
    commands: {
      ...real.commands,
      agentCommands: (provider: string, task: string | null) => agentCommands(provider, task),
      agentCreate: (request: unknown) => agentCreate(request),
      clipboardImage: () => clipboardImage(),
      vcsBranchInfo: (root: string | null) => vcsBranchInfo(root),
      vcsBranches: (root: string | null) => vcsBranches(root),
    },
  };
});
vi.mock("@tauri-apps/api/webview", () => ({
  getCurrentWebview: () => ({ onDragDropEvent: () => Promise.resolve(() => {}) }),
}));

import { isTauri } from "@/platform/ipc";
import { useProject } from "@/platform/project";
import type { ProjectInfo } from "@/bindings";
import { defaultTaskProject } from "./project";
import { HomeEditor } from "./HomeEditor";
import composerStyles from "./Composer.module.css";
import tagStyles from "@/ui/InlineTag.module.css";
import { composerDraft, HOME, updateComposer, useAgents } from "./store";
import { emptyDraft } from "./composerModel";

const command = (name: string, description: string, source: "command" | "skill" = "command"): AgentCommand => ({
  name,
  description,
  hint: null,
  source,
});
const claude: AgentProvider = {
  id: "claude",
  name: "Claude Code",
  command: "npx",
  installed: true,
  config: [],
  source: { kind: "builtin" },
  program: "claude",
  args: [],
  defaultProgram: "claude",
  defaultArgs: [],
  loginHint: null,
  commandOverridden: false,
  commandEditable: true,
  commandReadOnlyReason: null,
  env: {},
  // Offered by the agent's last session in this project.
  commands: [command("compact", "Free up context")],
};

beforeEach(() => {
  vi.mocked(isTauri).mockReturnValue(true);
  useProject.setState({ project: null });
  vcsBranches.mockReset().mockResolvedValue({ status: "ok", data: [] });
  vcsBranchInfo
    .mockReset()
    .mockImplementation((root: string | null) =>
      Promise.resolve({ status: "ok", data: { head: root === "/project-b" ? "b-main" : "a-main" } }),
    );
  agentCommands.mockReset();
  clipboardImage.mockReset().mockResolvedValue({ status: "ok", data: "data:image/png;base64,iVBORw0KGgo=" });
  agentCreate.mockReset().mockResolvedValue({ status: "error", error: { kind: "invalid", message: "stop here" } });
  useAgents.setState({
    providers: [claude],
    draft: { provider: "claude", config: {}, isolation: "local", base: null },
  });
  updateComposer(HOME, emptyDraft());
});

describe("New Task composer commands", () => {
  it("reaches commands after the first twelve with the keyboard", async () => {
    agentCommands.mockResolvedValue({
      status: "ok",
      data: Array.from({ length: 30 }, (_, i) => command(`command-${i}`, "")),
    });
    render(<HomeEditor />);
    await waitFor(() => expect(agentCommands).toHaveBeenCalled());
    const box = screen.getByRole("textbox");
    fireEvent.change(box, { target: { value: "/" } });
    await screen.findByRole("listbox", { name: "Commands" });
    for (let i = 0; i < 29; i++) fireEvent.keyDown(box, { key: "ArrowDown" });
    fireEvent.keyDown(box, { key: "Enter" });
    expect(box).toHaveValue("/command-29 ");
  });

  it("groups commands before skills, skips headers, wraps and hides empty groups", async () => {
    agentCommands.mockResolvedValue({
      status: "ok",
      data: [command("triage", "", "skill"), command("compact", ""), command("review", "", "skill")],
    });
    render(<HomeEditor />);
    await waitFor(() => expect(agentCommands).toHaveBeenCalled());
    const box = screen.getByRole("textbox");
    fireEvent.change(box, { target: { value: "/" } });
    const list = await screen.findByRole("listbox", { name: "Commands" });
    expect(
      within(list)
        .getAllByRole("option")
        .map((o) => o.textContent),
    ).toEqual(["/compact", "/triage", "/review"]);
    expect(list.textContent).toBe("Commands/compactSkills/triage/review");
    expect(within(list).getByRole("option", { selected: true })).toHaveTextContent("/compact");
    fireEvent.keyDown(box, { key: "ArrowDown" });
    expect(within(list).getByRole("option", { selected: true })).toHaveTextContent("/triage");
    fireEvent.keyDown(box, { key: "ArrowUp" });
    fireEvent.keyDown(box, { key: "ArrowUp" });
    expect(within(list).getByRole("option", { selected: true })).toHaveTextContent("/review");
    fireEvent.keyDown(box, { key: "ArrowDown" });
    expect(within(list).getByRole("option", { selected: true })).toHaveTextContent("/compact");
    fireEvent.change(box, { target: { value: "/tri" } });
    expect(list.textContent).toBe("Skills/triage");
    fireEvent.keyDown(box, { key: "Tab" });
    expect(box).toHaveValue("/triage ");
    fireEvent.change(box, { target: { value: "/com" } });
    expect((await screen.findByRole("listbox", { name: "Commands" })).textContent).toBe("Commands/compact");
  });

  it("names the icon-only Start action and disables it while starting", async () => {
    agentCommands.mockResolvedValue({ status: "ok", data: [] });
    let finish!: (value: unknown) => void;
    agentCreate.mockReturnValue(new Promise((resolve) => (finish = resolve)));
    render(<HomeEditor />);
    const start = screen.getByRole("button", { name: "Start" });
    expect(start.textContent).toBe("");
    expect(start).toHaveAttribute("data-tooltip", "Start");
    expect(start).toBeDisabled();
    fireEvent.change(screen.getByRole("textbox"), { target: { value: "Do the task" } });
    expect(start).toBeEnabled();
    fireEvent.click(start);
    const starting = screen.getByRole("button", { name: "Starting…" });
    expect(starting).toBeDisabled();
    expect(starting.textContent).toBe("");
    expect(starting).toHaveAttribute("data-tooltip", "Starting…");
    fireEvent.keyDown(screen.getByRole("textbox"), { key: "Enter" });
    expect(agentCreate).toHaveBeenCalledTimes(1);
    await act(async () => finish({ status: "error", error: { kind: "invalid", message: "stop here" } }));
    expect(screen.getByRole("button", { name: "Start" })).toBeEnabled();
  });

  it("completes the project's skills merged with the agent's commands", async () => {
    agentCommands.mockResolvedValue({
      status: "ok",
      data: [
        command("compact", "Free up context"),
        command("triage", "One pass over open issues and PRs"),
        command("issue-fix", "Fix an issue"),
      ],
    });
    render(<HomeEditor />);
    await waitFor(() => expect(agentCommands).toHaveBeenCalledWith("claude", null));
    const box = screen.getByRole("textbox");
    fireEvent.change(box, { target: { value: "/trig" } });
    const list = await screen.findByRole("listbox", { name: "Commands" });
    const options = within(list).getAllByRole("option");
    expect(options.map((o) => o.textContent)).toEqual(["/triageOne pass over open issues and PRs"]);

    fireEvent.keyDown(box, { key: "Enter" });
    expect(useAgents.getState().composers.get(HOME)?.text).toBe("/triage ");

    // Sent as typed, so the agent runs the skill.
    fireEvent.change(box, { target: { value: "/triage dry-run" } });
    fireEvent.keyDown(box, { key: "Enter" });
    await waitFor(() => expect(agentCreate).toHaveBeenCalled());
    expect(agentCreate.mock.calls[0]![0]).toMatchObject({ prompt: { text: "/triage dry-run" }, provider: "claude" });
  });

  it("tags file references and skills where they stand", async () => {
    agentCommands.mockResolvedValue({ status: "ok", data: [command("triage", "One pass over issues")] });
    render(<HomeEditor />);
    await waitFor(() => expect(agentCommands).toHaveBeenCalled());
    fireEvent.change(screen.getByRole("textbox"), { target: { value: "12 @Cargo.toml and /triage" } });
    const mirror = document.querySelector(`.${composerStyles.mirror}`)!;
    expect(within(mirror as HTMLElement).getByText("@Cargo.toml")).toHaveClass(tagStyles.tag!);
    expect(within(mirror as HTMLElement).getByText("/triage")).toHaveClass(tagStyles.accent!);
  });

  it("offers nothing the project does not have", async () => {
    agentCommands.mockResolvedValue({ status: "ok", data: [command("compact", "Free up context")] });
    render(<HomeEditor />);
    await waitFor(() => expect(agentCommands).toHaveBeenCalled());
    const box = screen.getByRole("textbox");
    fireEvent.change(box, { target: { value: "/trig" } });
    expect(screen.queryByRole("listbox", { name: "Commands" })).toBeNull();
    fireEvent.change(box, { target: { value: "/com" } });
    expect(await screen.findByRole("option")).toHaveTextContent("/compact");
  });

  it("completes at the caret without replacing the rest of the message", async () => {
    const user = userEvent.setup();
    agentCommands.mockResolvedValue({ status: "ok", data: [command("compact", "Free up context")] });
    render(<HomeEditor />);
    await waitFor(() => expect(agentCommands).toHaveBeenCalled());
    const box = screen.getByRole("textbox");
    box.focus();
    fireEvent.change(box, { target: { value: "/com keep this context", selectionStart: 4, selectionEnd: 4 } });
    expect(await screen.findByRole("option")).toHaveTextContent("/compact");
    expect(box).toHaveFocus();

    await user.keyboard("{Tab}");
    expect(box).toHaveValue("/compact keep this context");
    await waitFor(() => expect(screen.queryByRole("listbox", { name: "Commands" })).toBeNull());
    expect(agentCreate).not.toHaveBeenCalled();
  });

  it("completes a command typed after other text in place", async () => {
    agentCommands.mockResolvedValue({ status: "ok", data: [command("triage", "One pass over open issues and PRs")] });
    render(<HomeEditor />);
    await waitFor(() => expect(agentCommands).toHaveBeenCalled());
    const box = screen.getByRole("textbox");
    fireEvent.change(box, { target: { value: "fix the failing build /tri" } });
    await screen.findByRole("listbox", { name: "Commands" });
    fireEvent.keyDown(box, { key: "Enter" });
    expect(useAgents.getState().composers.get(HOME)?.text).toBe("fix the failing build /triage ");
  });
});

describe("New Task model settings", () => {
  const effortId = "reasoning_effort";
  const codexModels = codexTranscript
    .trim()
    .split("\n")
    .map((line) => JSON.parse(line))
    .find((entry) => entry.dir === "in" && entry.frame.result?.data?.[0]?.supportedReasoningEfforts).frame.result
    .data as {
    id: string;
    displayName: string;
    defaultReasoningEffort: string;
    supportedReasoningEfforts: { reasoningEffort: string; description: string }[];
  }[];
  const byModel = Object.fromEntries(
    codexModels.map((model) => [
      model.id,
      {
        defaultValue: model.defaultReasoningEffort,
        values: model.supportedReasoningEfforts.map(({ reasoningEffort, description }) => ({
          value: reasoningEffort,
          name:
            reasoningEffort === "xhigh" ? "Extra high" : reasoningEffort[0]!.toUpperCase() + reasoningEffort.slice(1),
          description: description || null,
        })),
      },
    ]),
  );

  function openSettings() {
    const modelButton = screen.getByRole("button", { name: "Model" });
    fireEvent.click(modelButton);
    return screen.getByRole("dialog", { name: "Model & effort settings" });
  }

  beforeEach(() => {
    agentCommands.mockResolvedValue({ status: "ok", data: [] });
    const model = {
      id: "model",
      name: "Model",
      description: null,
      category: "model",
      current: "gpt-5.5",
      via: "option" as const,
      values: [
        ...codexModels.map((m) => ({ value: m.id, name: m.displayName, description: null })),
        { value: "plain", name: "Plain", description: null },
      ],
      modelReasoning: {
        id: effortId,
        name: "Reasoning",
        byModel: { ...byModel, plain: { defaultValue: null, values: [] } },
      },
    };
    const effort: AgentConfigOption = {
      id: effortId,
      name: "Reasoning",
      description: null,
      category: "thought_level",
      current: "xhigh",
      via: "option",
      values: byModel["gpt-5.5"]!.values,
    };
    useAgents.setState({
      providers: [{ ...claude, id: "codex", name: "Codex", config: [model, effort] }],
      draft: { provider: "codex", config: { [effortId]: "xhigh" }, isolation: "local", base: null },
    });
  });

  it("uses the selected model's levels, defaults an unsupported draft effort and submits a supported choice", async () => {
    render(<HomeEditor />);
    const dialog = openSettings();
    expect(within(dialog).getByRole("slider", { name: "Reasoning" })).toHaveAttribute("max", "3");
    fireEvent.click(screen.getByRole("option", { name: /GPT-5.6-LUNA/ }));
    expect(useAgents.getState().draft.config.model).toBe("gpt-5.6-luna");
    const slider = within(dialog).getByRole("slider", { name: "Reasoning" });
    expect(slider).toHaveAttribute("max", "2");
    expect(slider).toHaveAttribute("aria-valuetext", "Medium");
    fireEvent.change(slider, { target: { value: "2" } });
    fireEvent.keyDown(dialog, { key: "Escape" });

    fireEvent.change(screen.getByRole("textbox"), { target: { value: "Do the task" } });
    fireEvent.click(screen.getByRole("button", { name: "Start" }));
    await waitFor(() => expect(agentCreate).toHaveBeenCalled());
    expect(agentCreate.mock.calls[0]![0].config).toEqual([
      { id: "model", value: "gpt-5.6-luna" },
      { id: effortId, value: "high" },
    ]);
  });

  it("hides effort for a model without levels and shows it again when switching back", () => {
    render(<HomeEditor />);
    openSettings();
    fireEvent.click(screen.getByRole("option", { name: "Plain" }));
    expect(screen.queryByRole("slider", { name: "Reasoning" })).not.toBeInTheDocument();
    expect(useAgents.getState().draft.config[effortId]).toBeUndefined();

    fireEvent.click(screen.getByRole("button", { name: "Model" }));
    fireEvent.click(screen.getByRole("option", { name: /GPT-5.6-LUNA/ }));
    expect(within(openSettings()).getByRole("slider", { name: "Reasoning" })).toHaveAttribute("max", "2");
  });
});

describe("New Task composer paste", () => {
  it("attaches the clipboard's image when WebKitGTK pastes an empty clipboardData", async () => {
    render(<HomeEditor />);
    fireEvent.paste(screen.getByRole("textbox"), { clipboardData: { types: [], files: [] } });
    await waitFor(() =>
      expect(composerDraft(HOME).images).toEqual([{ mimeType: "image/png", data: "iVBORw0KGgo=", name: "image" }]),
    );
  });

  it("leaves a text paste to the text box", () => {
    render(<HomeEditor />);
    fireEvent.paste(screen.getByRole("textbox"), { clipboardData: { types: ["text/plain"], files: [] } });
    expect(clipboardImage).not.toHaveBeenCalled();
  });
});

describe("workspace task project", () => {
  const project = {
    root: "/workspace",
    name: "Workspace",
    trusted: true,
    isGit: false,
    workspace: { version: 1, name: "Workspace", projects: ["/project-a", "/project-b"] },
  } as ProjectInfo;
  it("defaults to the active member, last used member, then first member", () => {
    expect(defaultTaskProject(project, "/project-b/src/main.ts", "/project-a")).toBe("/project-b");
    expect(defaultTaskProject(project, null, "/project-b")).toBe("/project-b");
    expect(defaultTaskProject(project, "/project-b-other/file", "/removed")).toBe("/project-a");
    expect(defaultTaskProject({ ...project, workspace: null }, "/project-b/file", "/project-b")).toBe("/workspace");
  });
  it.each(["local", "worktree"] as const)("sends the selected project with %s isolation", async (isolation) => {
    vi.mocked(isTauri).mockReturnValue(false);
    useProject.setState({ project });
    agentCommands.mockResolvedValue({ status: "ok", data: [] });
    useAgents.setState({ draft: { ...useAgents.getState().draft, projectRoot: "/project-b", isolation } });
    render(<HomeEditor />);
    const selector = screen.getByRole("button", { name: "Project" });
    expect(selector).toHaveTextContent("project-b");
    await waitFor(() => expect(vcsBranchInfo).toHaveBeenCalledWith("/project-b"));
    expect(vcsBranches).toHaveBeenCalledWith("/project-b");
    fireEvent.click(selector);
    expect(within(screen.getByRole("listbox")).getByText("/project-a")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("option", { name: /project-a/ }));
    fireEvent.change(screen.getByRole("textbox"), { target: { value: "Update member" } });
    await waitFor(() => expect(screen.getByRole("button", { name: "Start" })).toBeEnabled());
    fireEvent.keyDown(screen.getByRole("textbox"), { key: "Enter" });
    await waitFor(() => expect(agentCreate).toHaveBeenCalled());
    expect(agentCreate.mock.calls[0]![0]).toMatchObject({ projectRoot: "/project-a", isolation });
    expect(vcsBranchInfo).toHaveBeenLastCalledWith("/project-a");
    expect(vcsBranches).toHaveBeenLastCalledWith("/project-a");
    await vi.dynamicImportSettled();
  });
});
