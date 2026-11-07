/** Refresh plugin output when project settings or source dependencies change. */
import { create } from "zustand";
import { onEditorEvent } from "@/editor/api";
import { onIdeEvent } from "@/platform/ipc";
import { useProject } from "@/platform/project";
import { useSpringLaunchContext } from "@/platform/springLaunchContext";
import { activeSpringBeansPlugin } from "@/platform/springPlugins";
import { syncSpringLaunchContext } from "./springLaunchContext";
import { usePlugins } from "./store";

export const usePluginSettingsRevision = create<{ revision: number }>(() => ({ revision: 0 }));
const bump = () => usePluginSettingsRevision.setState((s) => ({ revision: s.revision + 1 }));
let timer: ReturnType<typeof setTimeout> | undefined;
function sourceChanged(paths: string[], removed = false) {
  const root = useProject.getState().project?.root;
  if (!root) return;
  const prefix = root.endsWith("/") ? root : `${root}/`;
  if (
    !paths.some((path) => {
      if (!path.startsWith(prefix)) return false;
      const relative = path.slice(prefix.length);
      const beforeSource = relative.split(/(?:^|\/)src\//)[0] ?? relative;
      if (/(^|\/)(target|build|node_modules|\.git)(\/|$)/.test(beforeSource)) return false;
      if (relative.startsWith(".openidea/plugins/")) return false;
      return (
        relative === ".openidea/run.toml" ||
        relative === ".openidea/run.local.toml" ||
        /\.(java|kt|xml|properties|ya?ml)$/.test(relative) ||
        (removed && !path.split("/").pop()?.includes("."))
      );
    })
  )
    return;
  clearTimeout(timer);
  timer = setTimeout(bump, 150);
}
const offs = [
  useProject.subscribe((state, previous) => {
    if (
      state.project?.root !== previous.project?.root ||
      JSON.stringify(state.settings?.plugins ?? {}) !== JSON.stringify(previous.settings?.plugins ?? {})
    ) {
      clearTimeout(timer);
      bump();
    }
  }),
  onIdeEvent("fsChanged", (event) => {
    sourceChanged([...event.created, ...event.modified]);
    sourceChanged(event.removed, true);
  }),
  onEditorEvent((event) => {
    if (event.type === "docSaved") sourceChanged([event.path]);
  }),
];
function refreshLaunchContext() {
  bump();
  void syncSpringLaunchContext().then(bump, () => {});
}
const offLaunch = useSpringLaunchContext.subscribe(refreshLaunchContext);
const offPlugins = usePlugins.subscribe((state, previous) => {
  if (activeSpringBeansPlugin(state.plugins) !== activeSpringBeansPlugin(previous.plugins)) refreshLaunchContext();
});
if (import.meta.hot)
  import.meta.hot.dispose(() => {
    clearTimeout(timer);
    offs.forEach((off) => off());
    offLaunch();
    offPlugins();
  });
