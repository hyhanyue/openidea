import { beforeEach, expect, it, vi } from "vitest";
import type { WorkspaceSymbol } from "@/bindings";
import { sourceTarget } from "./navigation";
import type { RuntimeEndpointEntry, RuntimeRow } from "./model";

const mocks = vi.hoisted(() => ({ symbols: vi.fn(), endpoints: vi.fn() }));
vi.mock("@/platform/ipc", async (original) => {
  const real = await original<typeof import("@/platform/ipc")>();
  return { ...real, call: (p: unknown) => p, commands: { ...real.commands, lspWorkspaceSymbols: mocks.symbols } };
});
vi.mock("@/workbench/contributions", async (original) => ({
  ...(await original<typeof import("@/workbench/contributions")>()),
  searchEndpoints: mocks.endpoints,
}));
const root = "/project";
function symbol(name: string, path: string): WorkspaceSymbol {
  const at = name.lastIndexOf(".");
  return {
    name: name.slice(at + 1),
    container: name.slice(0, at),
    kind: "class",
    location: { path, range: { start: { line: 2, col: 0 }, end: { line: 20, col: 0 } } },
  };
}
const child = symbol("com.example.ChildController", "/project/ChildController.java");
const base = symbol("com.example.base.BaseController", "/project/base/BaseController.java");
const row: RuntimeRow = {
  name: "GET /child/inherited",
  context: "app",
  detail: "",
  className: "com.example.ChildController",
  methodName: "inherited",
  methods: ["GET"],
  paths: ["/child/inherited"],
};
const route: RuntimeEndpointEntry = {
  controller: "ChildController",
  qualifiedController: "com.example.ChildController",
  declaringClass: "com.example.base.BaseController",
  methodName: "inherited",
  method: "GET",
  url: "/child/inherited",
  path: base.location.path,
  rel: "base/BaseController.java",
  line: 9,
  col: 5,
};
beforeEach(() => {
  vi.clearAllMocks();
  mocks.symbols.mockResolvedValue([child, base]);
  mocks.endpoints.mockResolvedValue([route]);
});
it.each([child, base])("navigates an inherited method when Actuator reports $container.$name", async (type) => {
  const target = await sourceTarget({ ...row, className: `${type.container}.${type.name}` }, root);
  expect(target).toEqual({ path: base.location.path, selection: { line: 9, col: 5 } });
});
it("keeps the same-file fallback for legacy or incomplete metadata", async () => {
  const { qualifiedController: _qualified, declaringClass: _declaring, ...legacy } = route;
  mocks.endpoints.mockResolvedValue([legacy]);
  expect(await sourceTarget(row, root)).toBeNull();
  mocks.endpoints.mockResolvedValue([{ ...legacy, path: child.location.path }]);
  expect(await sourceTarget(row, root)).toMatchObject({ path: child.location.path });
  mocks.endpoints.mockResolvedValue([{ ...legacy, qualifiedController: route.qualifiedController }]);
  expect(await sourceTarget(row, root)).toBeNull();
  mocks.endpoints.mockResolvedValue([{ ...legacy, declaringClass: route.declaringClass }]);
  expect(await sourceTarget(row, root)).toBeNull();
});
it("rejects same-named controllers in another package and conflicting provider identities", async () => {
  mocks.endpoints.mockResolvedValue([
    { ...route, qualifiedController: "other.ChildController", declaringClass: "other.BaseController" },
  ]);
  expect(await sourceTarget(row, root)).toBeNull();
  mocks.endpoints.mockResolvedValue([route, { ...route, qualifiedController: "other.ChildController" }]);
  expect(await sourceTarget({ ...row, className: route.declaringClass }, root)).toBeNull();
});
it("rejects multiple source targets, ambiguous exact types, and unrelated declaration files", async () => {
  mocks.endpoints.mockResolvedValue([route, { ...route, line: 15 }]);
  expect(await sourceTarget(row, root)).toBeNull();
  mocks.endpoints.mockResolvedValue([route]);
  mocks.symbols.mockResolvedValue([
    child,
    base,
    { ...base, location: { ...base.location, path: "/project/duplicate/BaseController.java" } },
  ]);
  expect(await sourceTarget(row, root)).toBeNull();
  mocks.symbols.mockResolvedValue([child, base]);
  mocks.endpoints.mockResolvedValue([{ ...route, path: "/project/Unrelated.java" }]);
  expect(await sourceTarget(row, root)).toBeNull();
});
it("rejects proxy and unknown library identities without falling back to a simple name", async () => {
  expect(await sourceTarget({ ...row, className: row.className + "$$SpringCGLIB$$0" }, root)).toBeNull();
  expect(await sourceTarget({ ...row, className: "jdk.proxy1.$Proxy42" }, root)).toBeNull();
  mocks.symbols.mockResolvedValue([
    child,
    { ...base, location: { ...base.location, path: "/library/BaseController.java" } },
  ]);
  expect(await sourceTarget(row, root)).toBeNull();
  mocks.symbols.mockResolvedValue([base]);
  expect(await sourceTarget({ ...row, className: route.declaringClass }, root)).toBeNull();
});
it("still requires the exact method name, HTTP method and URL", async () => {
  for (const changed of [{ methodName: "other" }, { methods: ["POST"] }, { paths: ["/other"] }, { methods: [] }]) {
    expect(await sourceTarget({ ...row, ...changed }, root)).toBeNull();
  }
});
it("accepts unique nested JVM/source class identities for mappings and beans", async () => {
  const nested = symbol("com.example.Outer.Inner", "/project/Outer.java");
  mocks.symbols.mockResolvedValue([nested]);
  const mapping = { ...row, className: "com.example.Outer$Inner" };
  mocks.endpoints.mockResolvedValue([
    {
      ...route,
      controller: "Outer.Inner",
      qualifiedController: "com.example.Outer.Inner",
      declaringClass: "com.example.Outer.Inner",
      path: nested.location.path,
    },
  ]);
  expect(await sourceTarget(mapping, root)).toEqual({ path: nested.location.path, selection: { line: 9, col: 5 } });
  expect(mocks.symbols).toHaveBeenCalledWith(expect.any(Number), "Inner");
  expect(await sourceTarget({ ...mapping, methodName: undefined }, root)).toEqual({
    path: nested.location.path,
    selection: nested.location.range.start,
  });
});
it("rejects ambiguous normalized nested types and preserves original FQN priority", async () => {
  const dotted = symbol("com.example.Outer.Inner", "/project/Outer.java");
  const binary = symbol("com.example.Outer$Inner", "/project/LiteralDollar.java");
  mocks.symbols.mockResolvedValue([
    dotted,
    { ...dotted, location: { ...dotted.location, path: "/project/other/Outer.java" } },
  ]);
  expect(await sourceTarget({ ...row, className: "com.example.Outer$Inner", methodName: undefined }, root)).toBeNull();
  mocks.symbols.mockResolvedValue([dotted, binary]);
  expect(
    await sourceTarget({ ...row, className: "com.example.Outer$Inner", methodName: undefined }, root),
  ).toMatchObject({ path: binary.location.path });
  mocks.endpoints.mockResolvedValue([
    {
      ...route,
      qualifiedController: "com.example.Outer.Inner",
      declaringClass: "com.example.Outer.Inner",
      path: dotted.location.path,
    },
  ]);
  expect(await sourceTarget({ ...row, className: "com.example.Outer$Inner" }, root)).toBeNull();
});
it("resolves a nested controller's inherited method to the separate base file", async () => {
  const nested = symbol("com.example.Outer.Inner", "/project/Outer.java");
  mocks.symbols.mockResolvedValue([nested, base]);
  mocks.endpoints.mockResolvedValue([
    { ...route, controller: "Outer.Inner", qualifiedController: "com.example.Outer.Inner" },
  ]);
  expect(await sourceTarget({ ...row, className: "com.example.Outer$Inner" }, root)).toEqual({
    path: base.location.path,
    selection: { line: 9, col: 5 },
  });
  expect(await sourceTarget({ ...row, className: route.declaringClass }, root)).toEqual({
    path: base.location.path,
    selection: { line: 9, col: 5 },
  });
});
