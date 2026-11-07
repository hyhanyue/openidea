import { readFileSync, readdirSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";
import { MappingProject } from "./mappings";
import { projectControllers } from "./endpoints";
import { parseSource } from "./source";

const fixture = resolve("fixtures/java-spring-mappings"),
  root = resolve(fixture, "src/main/java");
const sources = readdirSync(root, { recursive: true })
  .filter((p) => String(p).endsWith(".java"))
  .map((p) => {
    const path = resolve(root, String(p)),
      text = readFileSync(path, "utf8");
    return { path, text, file: parseSource(text) };
  });
interface Identity {
  stack: string;
  qualifiedController: string;
  declaringClass: string;
  methodName: string;
  paths: string[];
  methods: string[];
}
const evidence = JSON.parse(readFileSync(resolve(fixture, "navigation-identities.json"), "utf8")) as Identity[];
const project = new MappingProject(sources);
const order = (rows: Omit<Identity, "stack">[]) =>
  rows.sort((a, b) =>
    `${a.qualifiedController}.${a.methodName}`.localeCompare(`${b.qualifiedController}.${b.methodName}`),
  );

describe("navigation identities agree with real Spring handler types", () => {
  it.each(["mvc", "webflux"])("matches controller and declaring JVM names for every real %s handler", (stack) => {
    const actual = sources
      .flatMap((s) => projectControllers(project, s.path))
      .flatMap((c) =>
        c.endpoints.map((e) => ({
          qualifiedController: e.qualifiedController!,
          declaringClass: e.declaringClass!,
          methodName: e.methodName,
          paths: [...e.paths].sort(),
          methods: [...e.httpMethods].sort(),
        })),
      );
    expect(order(actual)).toEqual(order(evidence.filter((e) => e.stack === stack).map(({ stack: _, ...row }) => row)));
    const inherited = actual.find((e) => e.qualifiedController.endsWith(".InheritedController"));
    expect(inherited?.declaringClass).toBe("dev.openidea.mappings.api.GenericBase");
    const override = actual.find((e) => e.qualifiedController.endsWith(".CrossController"));
    expect(override?.declaringClass).toBe(override?.qualifiedController);
  });
});
