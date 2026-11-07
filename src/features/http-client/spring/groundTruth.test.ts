import { readFileSync, readdirSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";
import { MappingProject } from "./mappings";
import { projectControllers } from "./endpoints";
import { parseSource } from "./source";

const fixture = resolve("fixtures/java-spring-mappings");
const root = resolve(fixture, "src/main/java");
const sources = readdirSync(root, { recursive: true })
  .filter((p) => String(p).endsWith(".java"))
  .map((p) => {
    const path = resolve(root, String(p)),
      text = readFileSync(path, "utf8");
    return { path, text, file: parseSource(text) };
  });
const project = new MappingProject(sources);
interface Row {
  stack: string;
  controller: string;
  methodName: string;
  paths: string[];
  methods: string[];
  consumes: string[];
  produces: string[];
}
const groundTruth = JSON.parse(readFileSync(resolve(fixture, "ground-truth.json"), "utf8")) as Row[];

describe("Java8 Boot2.7.18 / Spring5.3.31 ground truth", () => {
  it.each(["mvc", "webflux"])("matches every real %s annotated handler and no foreign or invalid handler", (stack) => {
    const actual = sources
      .flatMap((s) => projectControllers(project, s.path))
      .flatMap((c) =>
        c.endpoints.map((e) => ({
          controller: c.name,
          methodName: e.methodName,
          paths: [...e.paths].sort(),
          methods: [...e.httpMethods].sort(),
          consumes: e.consumes ? [e.consumes] : [],
          produces: e.produces ? [e.produces] : [],
        })),
      );
    const order = (rows: Omit<Row, "stack">[]) =>
      rows.sort((a, b) => `${a.controller}.${a.methodName}`.localeCompare(`${b.controller}.${b.methodName}`));
    const expected = groundTruth.filter((r) => r.stack === stack).map(({ stack: _, ...row }) => row);
    expect(order(actual)).toEqual(order(expected));
  });
});
