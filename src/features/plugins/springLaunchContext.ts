/** Atomically replaces analysis facts in the host. No settings writes or environment logging. */
import { call, commands, isTauri } from "@/platform/ipc";
import { useProject } from "@/platform/project";
import { useSpringLaunchContext } from "@/platform/springLaunchContext";
import { activeSpringBeansPlugin } from "@/platform/springPlugins";
import { usePlugins } from "./store";

let pending = Promise.resolve();
let revision = -1;
let root: string | undefined;
let lastPlugin: string | null = null;

export function syncSpringLaunchContext(): Promise<void> {
  const state = useSpringLaunchContext.getState();
  const projectRoot = useProject.getState().project?.root;
  const plugin = activeSpringBeansPlugin(usePlugins.getState().plugins);
  if (!isTauri() || !projectRoot || !plugin) return Promise.resolve();
  if (state.revision === revision && projectRoot === root && plugin === lastPlugin) return pending;
  lastPlugin = plugin;
  revision = state.revision;
  root = projectRoot;
  const context = JSON.stringify({ projectRoot, context: state.context });
  // Serial writes prevent a slow earlier selection from overwriting the latest selection.
  pending = pending
    .catch(() => {})
    .then(async () => {
      if (
        useProject.getState().project?.root !== projectRoot ||
        useSpringLaunchContext.getState().revision !== state.revision ||
        activeSpringBeansPlugin(usePlugins.getState().plugins) !== plugin
      )
        return;
      try {
        await call(commands.pluginRunCommand(plugin, "openidea.setLaunchContext", context, null, []));
      } catch (error) {
        // A bounded/unsupported payload must never leave another selection's facts active.
        if (useProject.getState().project?.root !== projectRoot) return;
        try {
          await call(
            commands.pluginRunCommand(
              plugin,
              "openidea.setLaunchContext",
              JSON.stringify({ projectRoot, context: { version: 1, launch: null } }),
              null,
              [],
            ),
          );
        } catch {
          if (revision === state.revision && root === projectRoot) revision = -1;
          throw error;
        }
      }
    });
  return pending;
}
