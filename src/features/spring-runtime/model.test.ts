import { expect, it } from "vitest";
import { mappingCandidates, visibleRows, type RuntimeRow } from "./model";
import { exactClass } from "./navigation";
import type { WorkspaceSymbol } from "@/bindings";
import type { EndpointEntry } from "@/workbench/contributions";

const route: EndpointEntry = {
  method: "GET",
  url: "/demo",
  controller: "Controller",
  methodName: "get",
  path: "/p/Controller.java",
  rel: "Controller.java",
  line: 4,
  col: 2,
};
const row: RuntimeRow = {
  name: "GET /demo",
  context: "app",
  detail: "",
  className: "com.example.Controller",
  methodName: "get",
  methods: ["GET"],
  paths: ["/demo"],
};
it("preserves runtime identity and rejects mappings with unknown methods or paths", () => {
  expect(mappingCandidates(row, [route])).toEqual([route]);
  expect(mappingCandidates({ ...row, methods: [] }, [route])).toEqual([]);
  expect(mappingCandidates({ ...row, paths: ["/other"] }, [route])).toEqual([]);
  expect(mappingCandidates(row, [route, { ...route, methodName: "other" }])).toEqual([route]);
  expect(mappingCandidates(row, [route, { ...route, line: 12 }])).toHaveLength(2);
});
it("requires exact package identity and rejects same-named classes and duplicate source definitions", () => {
  const symbol: WorkspaceSymbol = {
    name: "Controller",
    kind: "class",
    container: "com.example",
    location: { path: "/p/Controller.java", range: { start: { line: 1, col: 0 }, end: { line: 2, col: 0 } } },
  };
  expect(exactClass([symbol], row.className!, "/p")).toEqual(symbol);
  expect(exactClass([{ ...symbol, container: "other" }], row.className!, "/p")).toBeNull();
  expect(
    exactClass(
      [symbol, { ...symbol, location: { ...symbol.location, path: "/p/sub/Controller.java" } }],
      row.className!,
      "/p",
    ),
  ).toBeNull();
  expect(exactClass([symbol], row.className!, "/else")).toBeNull();
  expect(exactClass([symbol], "Controller", "/p")).toBeNull();
  expect(exactClass([{ ...symbol, name: "Outer.Inner" }], "Outer$Inner", "/p")).toBeNull();
  expect(exactClass([{ ...symbol, name: "com.example.Controller" }], row.className!, "/p")).not.toBeNull();
});
it("filters runtime dependencies and context without inventing rows for failed endpoints", () => {
  expect(visibleRows({ state: "ok", rows: [{ ...row, dependencies: ["repository"] }] }, "REPOSITORY")).toHaveLength(1);
  expect(visibleRows({ state: "disabled", rows: [] }, "")).toEqual([]);
});
it("normalizes only nested type separators and never rewrites dollar signs in packages", () => {
  const nested = { ...row, className: "com$example.Outer$Inner" };
  const source = {
    ...route,
    controller: "Outer.Inner",
    qualifiedController: "com$example.Outer.Inner",
    declaringClass: "com$example.Outer.Inner",
  };
  expect(mappingCandidates(nested, [source])).toHaveLength(1);
  expect(
    mappingCandidates(nested, [
      { ...source, qualifiedController: "com.example.Outer.Inner", declaringClass: "com.example.Outer.Inner" },
    ]),
  ).toEqual([]);
});
