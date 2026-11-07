import { afterEach, expect, it, vi } from "vitest";
import { useProject } from "@/platform/project";
import { _emitEditorEvent } from "@/editor/api";
import { emitLocalIdeEvent } from "@/platform/ipc";
import { usePluginSettingsRevision } from "./settingsRevision";

afterEach(() => {
  useProject.setState({ project: null, settings: null });
  vi.useRealTimers();
});

it("coalesces config and source saves, including imported files, without idle polling", async () => {
  vi.useFakeTimers();
  useProject.setState({ project: { root: "/project", trusted: true } as never });
  const before = usePluginSettingsRevision.getState().revision;
  _emitEditorEvent({ type: "docSaved", doc: 1, path: "/project/src/main/resources/shared.yml" });
  emitLocalIdeEvent({
    type: "fsChanged",
    created: [],
    modified: ["/project/src/main/java/Settings.java"],
    removed: [],
  });
  await vi.advanceTimersByTimeAsync(149);
  expect(usePluginSettingsRevision.getState().revision).toBe(before);
  await vi.advanceTimersByTimeAsync(1);
  expect(usePluginSettingsRevision.getState().revision).toBe(before + 1);
  await vi.advanceTimersByTimeAsync(5000);
  expect(usePluginSettingsRevision.getState().revision).toBe(before + 1);
});

it("ignores other projects and generated output but refreshes a removed source folder", async () => {
  vi.useFakeTimers();
  useProject.setState({ project: { root: "/project", trusted: true } as never });
  const before = usePluginSettingsRevision.getState().revision;
  emitLocalIdeEvent({
    type: "fsChanged",
    created: [],
    removed: [],
    modified: [
      "/project-sibling/application.yml",
      "/other/Bean.java",
      "/project/target/classes/application.yml",
      "/project/README.md",
    ],
  });
  await vi.advanceTimersByTimeAsync(1000);
  expect(usePluginSettingsRevision.getState().revision).toBe(before);
  emitLocalIdeEvent({ type: "fsChanged", created: [], modified: [], removed: ["/project/src/main/java/settings"] });
  await vi.advanceTimersByTimeAsync(150);
  expect(usePluginSettingsRevision.getState().revision).toBe(before + 1);
});

it("drops delayed source work on project change and refreshes plugin setting changes immediately", async () => {
  vi.useFakeTimers();
  useProject.setState({ project: { root: "/project", trusted: true } as never });
  _emitEditorEvent({ type: "docSaved", doc: 1, path: "/project/application.properties" });
  useProject.setState({ project: { root: "/new", trusted: true } as never });
  const before = usePluginSettingsRevision.getState().revision;
  await vi.advanceTimersByTimeAsync(1000);
  expect(usePluginSettingsRevision.getState().revision).toBe(before);
  useProject.setState({
    settings: { plugins: { "aritti.spring-beans": { "springBeans.activeProfiles": "prod" } } } as never,
  });
  expect(usePluginSettingsRevision.getState().revision).toBe(before + 1);
});
