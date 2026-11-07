import { afterEach, describe, expect, it, vi } from "vitest";
import { commands } from "@/platform/ipc";
import { projectControllers } from "./endpoints";
import { MappingProject } from "./mappings";
import { clearRouteIndex, routeIndex } from "./routeIndex";
import { parseSource } from "./source";

const scan = vi.spyOn(commands, "springRouteSources");
const model = (files: Record<string, string>) =>
  new MappingProject(Object.entries(files).map(([path, text]) => ({ path, text, file: parseSource(text) })));
afterEach(() => {
  clearRouteIndex();
  scan.mockReset();
});

describe("runtime navigation identity seam", () => {
  it("keeps the concrete controller separate from an inherited declaration and its real coordinates", async () => {
    const base = {
      path: "/p/shared/Base.java",
      text: 'package shared; public abstract class Base { /* 😀 */ @GetMapping("/inherited") public String get(){} }',
    };
    scan.mockResolvedValue({
      status: "ok",
      data: [base, { path: "/p/a/Child.java", text: "package a; @RestController class Child extends shared.Base {}" }],
    });
    expect(await routeIndex("/p")).toMatchObject([
      {
        qualifiedController: "a.Child",
        declaringClass: "shared.Base",
        path: base.path,
        line: 0,
        col: base.text.indexOf("get()"),
        method: "GET",
        url: "/inherited",
      },
    ]);
  });
  it("identifies an interface override by its implementation declaration, including unsaved text", () => {
    const files = {
      "/p/Api.java": 'package api; interface Api<T> { @GetMapping("/x") String get(T query); }',
      "/p/C.java":
        "package web; @RestController class C implements api.Api<String> { public String get(String query){} }",
    };
    const p = model(files);
    for (const text of [undefined, files["/p/C.java"].replace("{}", "{ return query; }")]) {
      expect(projectControllers(p, "/p/C.java", text)[0]?.endpoints[0]).toMatchObject({
        qualifiedController: "web.C",
        declaringClass: "web.C",
      });
    }
  });
  it("retains distinct same-named controllers even when their method target is the same base file", async () => {
    scan.mockResolvedValue({
      status: "ok",
      data: [
        {
          path: "/p/Base.java",
          text: 'package shared; abstract class Base { @GetMapping("/same") public void get(){} }',
        },
        { path: "/p/a/Child.java", text: "package a; @RestController class Child extends shared.Base {}" },
        { path: "/p/b/Child.java", text: "package b; @RestController class Child extends shared.Base {}" },
      ],
    });
    const rows = await routeIndex("/p");
    expect(rows).toHaveLength(2);
    expect(rows.map((r) => r.qualifiedController)).toEqual(["a.Child", "b.Child"]);
    expect(new Set(rows.map((r) => r.path))).toEqual(new Set(["/p/Base.java"]));
  });
  it("emits actual JVM names for nested classes and omits ambiguous identities", () => {
    const nested = model({
      "/p/Outer.java":
        'package demo; class Outer { @RestController static class Child { @GetMapping("/x") void get(){} } }',
    });
    expect(projectControllers(nested, "/p/Outer.java")[0]?.endpoints[0]).toMatchObject({
      qualifiedController: "demo.Outer$Child",
      declaringClass: "demo.Outer$Child",
    });
    const duplicate = model({
      "/p/one/C.java": 'package demo; @RestController class C { @GetMapping("/x") void get(){} }',
      "/p/two/C.java": 'package demo; @RestController class C { @GetMapping("/x") void get(){} }',
    });
    for (const path of ["/p/one/C.java", "/p/two/C.java"]) {
      const e = projectControllers(duplicate, path)[0]?.endpoints[0];
      expect(e).not.toHaveProperty("qualifiedController");
      expect(e).not.toHaveProperty("declaringClass");
    }
  });
  it("does not synthesize a declaring class for an unavailable library contract", async () => {
    scan.mockResolvedValue({
      status: "ok",
      data: [{ path: "/p/C.java", text: "package web; @RestController class C extends external.LibraryController {}" }],
    });
    expect(await routeIndex("/p")).toEqual([]);
  });
});
