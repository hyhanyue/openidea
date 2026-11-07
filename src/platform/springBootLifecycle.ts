/** Boot startup milestones inferred from complete console lines, independent of process liveness. */
export type SpringBootPhase = "preparing" | "compiling" | "starting" | "ready" | "failed" | "stopping" | "stopped";
export type SpringBootFailure = "port" | "missingBean" | "ambiguousBean" | "binding" | "startup";
export interface SpringBootLifecycle {
  phase: SpringBootPhase;
  failure?: SpringBootFailure;
}

export function isSpringBootLaunch(config: { kind: string; args: readonly string[] }): boolean {
  return (
    config.kind === "spring-boot" ||
    (config.kind === "maven" &&
      config.args.some((a) => a === "spring-boot:run" || /:spring-boot-maven-plugin:[^:]*:run$/.test(a)))
  );
}

export function springBootLine(current: SpringBootLifecycle, line: string): SpringBootLifecycle {
  if (current.phase === "stopping" || current.phase === "stopped") return current;
  const text = line;
  const failure: SpringBootFailure | undefined =
    /Port \d+ was already in use|Address already in use|PortInUseException/.test(text)
      ? "port"
      : /NoUniqueBeanDefinitionException|required a single bean, but \d+ were found|expected single matching bean but found/.test(
            text,
          )
        ? "ambiguousBean"
        : /NoSuchBeanDefinitionException|required a bean of type .* that could not be found/.test(text)
          ? "missingBean"
          : /ConfigurationPropertiesBindException|BindException: Failed to bind|Failed to bind properties under|Binding to target .* failed/.test(
                text,
              )
            ? "binding"
            : /APPLICATION FAILED TO START|Application run failed/.test(text)
              ? "startup"
              : undefined;
  if (failure) {
    // A generic banner must not hide the more useful cause already seen.
    if (failure === "startup" && current.failure) return current;
    return { phase: "failed", failure };
  }
  if (current.phase === "failed") return current;
  if (/\bStarted [\w.$]+ in [\d.,]+ seconds \((?:JVM|process) running for [\d.,]+\)/.test(text))
    return { phase: "ready" };
  return current;
}

export function springBootEnded(
  current: SpringBootLifecycle,
  code: number | null,
  stopped = false,
): SpringBootLifecycle {
  if (stopped) return { phase: "stopped" };
  if (current.phase === "failed") return current;
  return code === 0 ? { phase: "stopped" } : { phase: "failed", failure: "startup" };
}
