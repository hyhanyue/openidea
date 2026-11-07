/** Refresh plugin output when its effective project settings change. */
import { create } from "zustand";
import { useProject } from "@/platform/project";
import { useSpringLaunchContext } from "@/platform/springLaunchContext";
import { syncSpringLaunchContext } from "./springLaunchContext";
import { activeSpringBeansPlugin } from "@/platform/springPlugins";
import { usePlugins } from "./store";

export const usePluginSettingsRevision = create<{ revision: number }>(() => ({ revision: 0 }));
const off = useProject.subscribe((state, previous) => {
  if (
    state.project?.root !== previous.project?.root ||
    JSON.stringify(state.settings?.plugins ?? {}) !== JSON.stringify(previous.settings?.plugins ?? {})
  ) {
    usePluginSettingsRevision.setState((s) => ({ revision: s.revision + 1 }));
  }
});
function refreshLaunchContext() {
  // Invalidate in-flight results immediately and refresh again after the host accepted the facts.
  usePluginSettingsRevision.setState((s) => ({ revision: s.revision + 1 }));
  void syncSpringLaunchContext().then(
    () => usePluginSettingsRevision.setState((s) => ({ revision: s.revision + 1 })),
    () => {},
  );
}
const offLaunch = useSpringLaunchContext.subscribe(refreshLaunchContext);
const offPlugins = usePlugins.subscribe((state, previous) => {
  if (activeSpringBeansPlugin(state.plugins) !== activeSpringBeansPlugin(previous.plugins)) refreshLaunchContext();
});
if (import.meta.hot)
  import.meta.hot.dispose(() => {
    off();
    offLaunch();
    offPlugins();
  });
