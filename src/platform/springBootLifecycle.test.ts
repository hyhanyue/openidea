import { expect, it } from "vitest";
import { springBootLine, springBootEnded, isSpringBootLaunch, type SpringBootFailure } from "./springBootLifecycle";

const starting = { phase: "starting" as const };
it("waits for Boot's startup milestone, not Tomcat port binding or an arbitrary Started line", () => {
  expect(springBootLine(starting, "Tomcat started on port(s): 8080 (http)")).toBe(starting);
  expect(springBootLine(starting, "Started transaction")).toBe(starting);
  expect(
    springBootLine(starting, "INFO --- [main] a.App : Started App in 3.321 seconds (JVM running for 4.01)"),
  ).toEqual({ phase: "ready" });
});
it.each<[string, SpringBootFailure]>([
  ["Port 8080 was already in use.", "port"],
  ["NoSuchBeanDefinitionException: a.Missing", "missingBean"],
  ["Parameter 0 required a bean of type 'a.Missing' that could not be found.", "missingBean"],
  ["NoUniqueBeanDefinitionException: expected single matching bean but found 2", "ambiguousBean"],
  ["Failed to bind properties under 'probe.number' to int", "binding"],
  ["APPLICATION FAILED TO START", "startup"],
])("classifies failure without retaining raw property values: %s", (line, failure) => {
  const failed = springBootLine(starting, line);
  expect(failed).toEqual({ phase: "failed", failure });
  expect(springBootLine(failed, "Started App in 1.2 seconds (JVM running for 3.4)")).toBe(failed);
  expect(springBootLine(failed, "APPLICATION FAILED TO START")).toBe(failed);
});
it("preserves failure until exit, distinguishes user stop, and never revives a stopped app", () => {
  const failed = { phase: "failed" as const, failure: "binding" as const };
  expect(springBootEnded(failed, 1)).toBe(failed);
  expect(springBootEnded(failed, 130, true)).toEqual({ phase: "stopped" });
  expect(springBootEnded({ phase: "ready" }, 1)).toEqual({ phase: "failed", failure: "startup" });
  const stopped = { phase: "stopped" as const };
  expect(springBootLine(stopped, "Started App in 1.2 seconds (JVM running for 3.4)")).toBe(stopped);
});
it("recognizes direct and fully-qualified Maven Boot launches only", () => {
  expect(
    isSpringBootLaunch({ kind: "maven", args: ["org.springframework.boot:spring-boot-maven-plugin:2.7.18:run"] }),
  ).toBe(true);
  expect(isSpringBootLaunch({ kind: "maven", args: ["test"] })).toBe(false);
});
