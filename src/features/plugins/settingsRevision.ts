/** Refresh plugin output when its effective project settings change. */
import { create } from "zustand";
import { useProject } from "@/platform/project";

export const usePluginSettingsRevision = create<{ revision: number }>(() => ({ revision: 0 }));
const off = useProject.subscribe((state, previous) => {
  if (
    state.project?.root !== previous.project?.root ||
    JSON.stringify(state.settings?.plugins ?? {}) !== JSON.stringify(previous.settings?.plugins ?? {})
  ) {
    usePluginSettingsRevision.setState((s) => ({ revision: s.revision + 1 }));
  }
});
if (import.meta.hot) import.meta.hot.dispose(off);
