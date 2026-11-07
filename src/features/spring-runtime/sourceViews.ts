import { toolWindowService, useContributions } from "@/workbench/contributions";

export function sourceViewId(view: "beans" | "bean-graph"): string | null {
  const windows = useContributions.getState().toolWindows;
  for (const plugin of ["hyhanyue.spring-beans", "hyhanyue.spring-beans-local", "aritti.spring-beans"]) {
    const id = `plugin.${plugin}.${view}`;
    if (windows.has(id)) return id;
  }
  return null;
}
export function showSourceView(view: "beans" | "bean-graph") {
  const id = sourceViewId(view);
  if (id) toolWindowService().show(id, true);
}
