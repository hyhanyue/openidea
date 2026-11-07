import type { PluginInfo } from "./ipc";

/** IDs used by source development, the personal fork and its known migration package. */
export const SPRING_BEANS_PLUGIN_IDS = [
  "hyhanyue.spring-beans",
  "aritti.spring-beans",
  "hyhanyue.spring-beans-local",
] as const;
export const isSpringBeansPlugin = (id: string) => SPRING_BEANS_PLUGIN_IDS.some((known) => known === id);

export function activeSpringBeansPlugin(
  plugins: readonly Pick<PluginInfo, "id" | "enabled" | "status">[],
): string | null {
  return (
    SPRING_BEANS_PLUGIN_IDS.find((id) =>
      plugins.some((p) => p.id === id && p.enabled && (p.status === "active" || p.status === "inactive")),
    ) ?? null
  );
}
