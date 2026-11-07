/** Versioned, transient facts and references for first-party Spring analysis; never a process environment. */
import { create } from "zustand";
import type { RunConfig } from "./ipc";
import { parseJavaArgs } from "./javaLaunch";
import { parseArgs } from "./commandLine";

export type SpringPropertyFact = { state: "known"; value: string } | { state: "unknown" };
export interface SpringLaunchContext {
  version: 1;
  launch: {
    configId: string;
    name: string;
    kind: string;
    persisted: boolean;
    mainClass: string | null;
    module: string | null;
    cwd: string | null;
    profiles: {
      state: "known" | "unknown";
      values: string[];
      source: "commandLine" | "systemProperties" | "environment" | "unknown";
    };
    commandLineProperties: Record<string, SpringPropertyFact>;
    systemProperties: Record<string, SpringPropertyFact>;
    /** Explicit configured keys only. Inherited process environment is never enumerated. */
    environment: Record<string, SpringPropertyFact>;
    inheritEnvironment: boolean;
    envFiles: string[];
    unresolvedSources: string[];
  };
}

export const useSpringLaunchContext = create<{ context: SpringLaunchContext | null; revision: number }>(() => ({
  context: null,
  revision: 0,
}));
const sensitive = /password|passwd|secret|token|credential|private|api.?key|auth|cookie|session|url|uri/i;
const fact = (key: string, value: string): SpringPropertyFact =>
  !sensitive.test(key) &&
  (/^(true|false)$/.test(value) ||
    (["spring.profiles.active", "SPRING_PROFILES_ACTIVE"].includes(key) && /^[\w., -]*$/.test(value)))
    ? { state: "known", value }
    : { state: "unknown" };

/** Args describe execution; params can be stale editor metadata. Persisted values stay in the authorized project. */
export function springLaunchContext(config: RunConfig | null, persisted = false): SpringLaunchContext | null {
  if (!config || config.target) return null;
  let vm: string[];
  let program: string[];
  let main: string | null;
  let module = config.module ?? null;
  let cwd = config.cwd;
  if (config.kind === "spring-boot" || config.kind === "application") {
    const parsed = parseJavaArgs(config.args);
    main = parsed.mainClass;
    vm = parsed.vmArgs;
    program = parsed.programArgs;
  } else if (
    config.kind === "maven" &&
    config.args.some((a) => a === "spring-boot:run" || /:spring-boot-maven-plugin:[^:]*:run$/.test(a))
  ) {
    const property = (key: string) =>
      config.args
        .filter((a) => a.startsWith(`-D${key}=`))
        .at(-1)
        ?.slice(key.length + 3);
    vm = parseArgs(property("spring-boot.run.jvmArguments") ?? "");
    program = parseArgs(property("spring-boot.run.arguments") ?? "");
    const profiles = property("spring-boot.run.profiles");
    if (profiles !== undefined) program.unshift(`--spring.profiles.active=${profiles}`);
    main = property("spring-boot.run.main-class") ?? null;
    module ??= config.cwd ?? "";
    cwd = property("spring-boot.run.workingDirectory") ?? config.cwd;
  } else return null;
  const system = new Map<string, string>();
  for (const arg of vm) {
    const m = /^-D([^=]+)=(.*)$/s.exec(arg);
    if (m) system.set(m[1]!, m[2]!);
  }
  const cli = new Map<string, string[]>();
  for (const arg of program) {
    const m = /^--([^=]+)(?:=(.*))?$/s.exec(arg);
    if (!m) continue;
    const values = cli.get(m[1]!) ?? [];
    values.push(m[2] ?? "");
    cli.set(m[1]!, values);
  }
  const commandLine = new Map([...cli].map(([key, values]) => [key, values.join(",")]));
  const env = new Map(Object.entries(config.env));
  const inherit = config.includeSystemEnv !== false;
  const envFiles = config.envFiles ?? [];
  const implicitJvm =
    inherit ||
    !!envFiles.length ||
    ["JAVA_TOOL_OPTIONS", "JDK_JAVA_OPTIONS", "_JAVA_OPTIONS"].some((key) => env.has(key));
  const json = env.has("SPRING_APPLICATION_JSON") || system.has("spring.application.json");
  let profile = commandLine.get("spring.profiles.active");
  let source: SpringLaunchContext["launch"]["profiles"]["source"] = profile === undefined ? "unknown" : "commandLine";
  if (profile === undefined && !implicitJvm && !json) {
    profile = system.get("spring.profiles.active");
    if (profile !== undefined) source = "systemProperties";
    else {
      profile = env.get("SPRING_PROFILES_ACTIVE");
      if (profile !== undefined) source = "environment";
    }
  }
  const known = profile !== undefined && /^[\w., -]*$/.test(profile);
  const properties = (input: Map<string, string>) =>
    Object.fromEntries([...input].map(([key, value]) => [key, fact(key, value)]));
  return {
    version: 1,
    launch: {
      configId: config.id,
      name: config.name,
      kind: config.kind,
      persisted,
      mainClass: main,
      module,
      cwd,
      profiles: {
        state: known ? "known" : "unknown",
        values: known
          ? (profile ?? "")
              .split(",")
              .map((p) => p.trim())
              .filter(Boolean)
          : [],
        source: known ? source : "unknown",
      },
      commandLineProperties: properties(commandLine),
      systemProperties: properties(system),
      environment: properties(env),
      inheritEnvironment: inherit,
      envFiles: [...envFiles],
      unresolvedSources: [
        ...(inherit ? ["inheritedEnvironment"] : []),
        ...(implicitJvm ? ["implicitJvmOptions"] : []),
        ...(json ? ["springApplicationJson"] : []),
        ...(envFiles.length ? ["envFiles"] : []),
      ],
    },
  };
}

export function publishSpringLaunchContext(config: RunConfig | null, persisted = false) {
  const context = springLaunchContext(config, persisted);
  if (JSON.stringify(context) === JSON.stringify(useSpringLaunchContext.getState().context)) return;
  useSpringLaunchContext.setState((s) => ({ context, revision: s.revision + 1 }));
}
