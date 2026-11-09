import { expect, it, vi } from "vitest";
import type { PluginInfo } from "@/platform/ipc";
import { useProject } from "@/platform/project";
import { publishSpringLaunchContext, useSpringLaunchContext } from "@/platform/springLaunchContext";
import { activeSpringBeansPlugin } from "@/platform/springPlugins";
import { usePlugins } from "./store";
import { syncSpringLaunchContext } from "./springLaunchContext";

const command = vi.hoisted(() => vi.fn(async (..._args: unknown[]) => ({ status: "ok", data: null })));
vi.mock("@/platform/ipc", async (original) => ({
  ...(await original<typeof import("@/platform/ipc")>()),
  isTauri: () => true,
  commands: { ...(await original<typeof import("@/platform/ipc")>()).commands, pluginRunCommand: command },
}));
const plugin = (id: string, enabled = true, status = "active") => ({ id, enabled, status }) as PluginInfo;

it("routes the actual call to the enabled stable fork ID and clears only transient facts", async () => {
  useProject.setState({ project: { root: "/project" } as never });
  usePlugins.setState({ plugins: [plugin("aritti.spring-beans", false, "disabled"), plugin("hyhanyue.spring-beans")] });
  publishSpringLaunchContext({
    id: "dev",
    name: "dev",
    kind: "spring-boot",
    command: "java",
    args: ["a.App", "--spring.profiles.active=dev"],
    env: { SECRET: "never-transfer" },
    cwd: null,
    detected: false,
    isTest: false,
    debugAdapter: "java-debug",
  });
  await syncSpringLaunchContext();
  expect(command).toHaveBeenLastCalledWith(
    "hyhanyue.spring-beans",
    "openidea.setLaunchContext",
    expect.any(String),
    null,
    [],
    "project",
    null,
  );
  expect(JSON.parse(command.mock.calls.at(-1)![2] as string).context.launch.profiles.values).toEqual(["dev"]);
  expect(command.mock.calls.at(-1)![2]).not.toContain("never-transfer");
  publishSpringLaunchContext(null);
  expect(useSpringLaunchContext.getState().context).toBeNull();
  await syncSpringLaunchContext();
  expect(command).toHaveBeenLastCalledWith(
    "hyhanyue.spring-beans",
    "openidea.setLaunchContext",
    JSON.stringify({ projectRoot: "/project", context: null }),
    null,
    [],
    "project",
    null,
  );
  usePlugins.setState({ plugins: [plugin("aritti.spring-beans")] });
  await syncSpringLaunchContext();
  expect(command).toHaveBeenLastCalledWith(
    "aritti.spring-beans",
    "openidea.setLaunchContext",
    JSON.stringify({ projectRoot: "/project", context: null }),
    null,
    [],
    "project",
    null,
  );
  usePlugins.setState({ plugins: [plugin("hyhanyue.spring-beans-local")] });
  await syncSpringLaunchContext();
  expect(command).toHaveBeenLastCalledWith(
    "hyhanyue.spring-beans-local",
    "openidea.setLaunchContext",
    JSON.stringify({ projectRoot: "/project", context: null }),
    null,
    [],
    "project",
    null,
  );
  const count = command.mock.calls.length;
  usePlugins.setState({ plugins: [plugin("other.spring-beans")] });
  await syncSpringLaunchContext();
  expect(command).toHaveBeenCalledTimes(count);
  useProject.setState({ project: null });
  usePlugins.setState({ plugins: [] });
});

it("never selects disabled, failed, unconsented or unrelated plugins", () => {
  expect(
    activeSpringBeansPlugin([
      plugin("hyhanyue.spring-beans", false),
      plugin("aritti.spring-beans", true, "failed"),
      plugin("hyhanyue.spring-beans-local", true, "needsConsent"),
      plugin("other.spring-beans"),
    ]),
  ).toBeNull();
});

it("replaces rejected launch facts with unknown instead of retaining another profile", async () => {
  useProject.setState({ project: { root: "/fallback" } as never });
  usePlugins.setState({ plugins: [plugin("hyhanyue.spring-beans")] });
  publishSpringLaunchContext({
    id: "new",
    name: "new",
    kind: "spring-boot",
    command: "java",
    args: ["a.App", "--spring.profiles.active=prod"],
    env: {},
    cwd: null,
    detected: false,
    isTest: false,
    debugAdapter: "java-debug",
  });
  command.mockRejectedValueOnce(new Error("unsupported bounded context"));
  await syncSpringLaunchContext();
  expect(command).toHaveBeenLastCalledWith(
    "hyhanyue.spring-beans",
    "openidea.setLaunchContext",
    JSON.stringify({ projectRoot: "/fallback", context: { version: 1, launch: null } }),
    null,
    [],
    "project",
    null,
  );
  useProject.setState({ project: null });
  usePlugins.setState({ plugins: [] });
});
