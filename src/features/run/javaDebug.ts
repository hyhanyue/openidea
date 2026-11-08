/**
 * Debugging Maven / Gradle run configurations (IDEA debugs those too): the
 * build tool starts the JVM with a JDWP agent that waits for a debugger
 * (`spring-boot:run` → `-Dspring-boot.run.jvmArguments`, Surefire →
 * `-Dmaven.surefire.debug`, Gradle → `--debug-jvm`); when the agent reports
 * its port, the Java debugger attaches. Output and the test tree stay in the Run tab.
 * Maven goals that run the application inside Maven's JVM (`tomcat7:run`, `jetty:run`,
 * `exec:java`) get the agent through `MAVEN_OPTS`; any other configuration set to the Java
 * debugger (a script such as `catalina.sh run`) through `JAVA_TOOL_OPTIONS`, which every JVM reads.
 */
import { isCancelled } from "@/platform/ipc";
import { tx } from "@/platform/i18n";
import { javaAgentDebugger, notifications } from "@/workbench/contributions";
import type { RunConfig } from "@/platform/ipc";
import { isJvmApplication, JDWP_AGENT, jdwpListeningPort, resolveJavaModule } from "@/platform/javaLaunch";
import { observeRunOutput, setDebugAttachment, startRun, stopSession, useRun, waitForExit } from "./store";
import { DELEGATED_DEBUG, DELEGATED_MAIN } from "./gradleRun";

const isMavenTestGoal = (a: string) =>
  /^(test|verify|integration-test|surefire:test|failsafe:integration-test)$/.test(a);
const isGradleJvmTask = (a: string) => /(^|:)(test|bootRun|run|integrationTest|check)$/.test(a);
/** A plugin goal that runs the application in Maven's own JVM (`tomcat7:run`, `jetty:run-war`, `exec:java`). */
const isMavenInProcessGoal = (a: string) => /^[\w.-]+(:[\w.-]+)*:(run|run-war|run-exploded|java)$/.test(a);
const JVM_ADAPTERS = ["java-debug", "kotlin-debug-adapter"];

function withOption(env: Record<string, string>, key: string, value: string): Record<string, string> {
  const current = env[key]?.trim();
  return { ...env, [key]: current ? `${current} ${value}` : value };
}

function withProperty(args: readonly string[], key: string, value: string): string[] {
  const prefix = `-D${key}=`;
  const i = args.findIndex((a) => a.startsWith(prefix));
  if (i < 0) return [...args, prefix + value];
  const current = args[i]!.slice(prefix.length).trim();
  const out = [...args];
  out[i] = prefix + (current ? `${current} ${value}` : value);
  return out;
}

/** The configuration with a waiting JDWP agent, or null when it starts no debuggable JVM. */
export function jdwpLaunch(config: RunConfig): RunConfig | null {
  if (config.kind === "tomcat") return { ...config, params: { ...config.params, tomcatDebug: "true" } };
  const args = config.args;
  // java-debug merges env into its tooling JVM's environment. Let the Run manager own
  // isolated project JVMs instead, then attach the same debugger over loopback JDWP.
  if (isJvmApplication(config) && config.includeSystemEnv === false) return { ...config, args: [JDWP_AGENT, ...args] };
  // A single source file (`java Hi.java`): the agent goes before the file. Compiled first
  // (`javac … && java -cp …`): through `JDK_JAVA_OPTIONS`, which only the `java` launcher reads.
  if (config.kind === "java-file")
    return args.length
      ? { ...config, args: [JDWP_AGENT, ...args] }
      : { ...config, env: withOption(config.env, "JDK_JAVA_OPTIONS", JDWP_AGENT) };
  if (config.kind === "maven" || /(^|\/)mvnw?(\.cmd)?$/.test(config.command)) {
    if (args.some((a) => a === "spring-boot:run" || /:spring-boot-maven-plugin:[^:]*:run$/.test(a)))
      return { ...config, args: withProperty(args, "spring-boot.run.jvmArguments", JDWP_AGENT) };
    if (config.isTest || args.some(isMavenTestGoal)) {
      const failsafe = args.some((a) => /integration-test|verify|failsafe/.test(a));
      let out = withProperty(args, "maven.surefire.debug", JDWP_AGENT);
      if (failsafe) out = withProperty(out, "maven.failsafe.debug", JDWP_AGENT);
      return { ...config, args: out };
    }
    if (JVM_ADAPTERS.includes(config.debugAdapter ?? "") || args.some(isMavenInProcessGoal))
      return { ...config, env: withOption(config.env, "MAVEN_OPTS", JDWP_AGENT) };
    return null;
  }
  if (config.kind === "gradle" || /(^|\/)gradlew?(\.bat)?$/.test(config.command)) {
    // A delegated Application: its init script adds the agent (port 0) to the JavaExec JVM.
    if (args.some((a) => a.startsWith(DELEGATED_MAIN)))
      return args.includes(DELEGATED_DEBUG) ? config : { ...config, args: [...args, DELEGATED_DEBUG] };
    if (!args.some(isGradleJvmTask)) return null;
    return args.includes("--debug-jvm") ? config : { ...config, args: [...args, "--debug-jvm"] };
  }
  // Application / Spring Boot configurations are launched by java-debug itself.
  if (JVM_ADAPTERS.includes(config.debugAdapter ?? "") && !isJvmApplication(config))
    return { ...config, env: withOption(config.env, "JAVA_TOOL_OPTIONS", JDWP_AGENT) };
  return null;
}

/** Runs an owned JVM only after the adapter is available; failed attach releases that exact process. */
export async function debugWithAgent(config: RunConfig, show?: boolean): Promise<boolean> {
  const launch = jdwpLaunch(config);
  if (!launch) return false;
  let prepared;
  try {
    if (isJvmApplication(launch)) launch.module = await resolveJavaModule(config);
    const service = javaAgentDebugger();
    if (!service) throw new Error(tx("Debugger is not available"));
    prepared = await service.prepare(config, launch.module ?? config.module ?? config.cwd ?? "");
  } catch (error) {
    if (!isCancelled(error))
      notifications().notify({
        group: "run",
        level: "error",
        title: tx("Cannot start debugging"),
        body: error instanceof Error ? error.message : String(error),
      });
    return true;
  }
  let sessionId: string | null = null;
  let ownedRunId: number | null = null;
  const controller = new AbortController();
  const waiting = new Set<number>();
  const attempted = new Set<number>();
  const timers = new Set<ReturnType<typeof setTimeout>>();
  let failed = false;
  let off = () => {};
  let offOutput: (() => void) | null = null;
  const current = () => useRun.getState().sessions.find((s) => s.id === sessionId && s.runId === ownedRunId);
  const mark = (debugAttachment: "waiting" | "attached" | "failed") => {
    if (!current()) return;
    setDebugAttachment(sessionId!, ownedRunId!, debugAttachment);
  };
  const dispose = () => {
    off();
    offOutput?.();
    offOutput = null;
    for (const timer of timers) clearTimeout(timer);
    timers.clear();
    controller.abort();
  };
  const release = async () => {
    const run = current();
    if (failed || !run || run.status !== "running") return;
    if (run.stopped) {
      dispose();
      return;
    }
    failed = true;
    mark("failed");
    dispose();
    setDebugAttachment(run.id, ownedRunId!, "failed", tx("Debugger attachment failed. Stopping this launch's JVM."));
    await stopSession(run.id);
    if (current()?.stopped) await waitForExit([run.id]);
    if (current()?.status === "running")
      notifications().notify({
        group: "run",
        level: "error",
        title: tx("Cannot attach the debugger"),
        body: tx("This JVM is still running. Stop it before retrying Debug."),
      });
  };
  const attach = async (port: number) => {
    if (attempted.has(port) || failed || controller.signal.aborted || current()?.status !== "running") return;
    attempted.add(port);
    const timer = setTimeout(() => {
      void release();
    }, 30_000);
    timers.add(timer);
    try {
      if (!(await prepared.attach(port, ownedRunId!, controller.signal, show))) await release();
      else if (!controller.signal.aborted && current()) {
        mark("attached");
        offOutput ??= observeRunOutput(sessionId!, ownedRunId!, (chunk) => prepared.output(ownedRunId!, chunk));
      }
    } catch {
      await release();
    } finally {
      clearTimeout(timer);
      timers.delete(timer);
    }
  };
  const session = await startRun(config, undefined, undefined, {
    launch,
    show,
    captureOutput: true,
    onExit: dispose,
    onLine: (line) => {
      const port = jdwpListeningPort(line);
      if (port === null) return;
      if (sessionId) void attach(port);
      else waiting.add(port);
    },
  });
  if (!session || session.runId === null || session.status !== "running" || session.stopped) {
    dispose();
    return true;
  }
  sessionId = session.id;
  ownedRunId = session.runId;
  mark("waiting");
  off = useRun.subscribe(() => {
    const run = current();
    if (!run || run.stopped || run.status !== "running") dispose();
  });
  await Promise.all([...waiting].map(attach));
  return true;
}
