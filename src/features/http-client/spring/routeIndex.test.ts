import { afterEach, describe, expect, it, vi } from "vitest";
import { commands, emitLocalIdeEvent } from "@/platform/ipc";
import { useProject } from "@/platform/project";
import { ipcPath } from "@/platform/paths";
import { searchEndpoints } from "@/workbench/contributions";
import {
  clearRouteIndex,
  controllersForSource,
  isRouteSource,
  knownControllers,
  registerSpringEndpointProvider,
  routeIndex,
  updateRouteSource,
} from "./routeIndex";

const scanSources = vi.spyOn(commands, "springRouteSources");

const source = (method = "searchMenus", url = "/menu/search") => ({
  path: "/p/DeepLinkController.java",
  text: `@RestController
@RequestMapping("/deeplink")
class DeepLinkController {
    @PostMapping("${url}")
    String ${method}() { return "ok"; }
}`,
});

let dispose: (() => void) | undefined;
afterEach(() => {
  dispose?.();
  dispose = undefined;
  clearRouteIndex();
  scanSources.mockReset();
});

describe("the request-path index follows the workspace", () => {
  it("is invalidated by a change to a mapping source", () => {
    for (const [path, expected] of [
      ["/p/A.java", true],
      ["/p/B.kt", true],
      ["/p/C.kts", true],
      ["/p/pom.xml", false],
      ["/p/notes.md", false],
    ] as const) {
      expect(isRouteSource(path), path).toBe(expected);
    }
  });
});

describe("the Spring endpoint provider", () => {
  it("scans once per root with method coordinates and rescans for another root", async () => {
    scanSources.mockResolvedValue({ status: "ok", data: [source()] });
    expect(await routeIndex(null)).toEqual([]);
    const first = await routeIndex("/p");
    expect(first).toEqual([
      {
        method: "POST",
        url: "/deeplink/menu/search",
        controller: "DeepLinkController",
        methodName: "searchMenus",
        path: "/p/DeepLinkController.java",
        rel: "DeepLinkController.java",
        line: 4,
        col: 11,
      },
    ]);
    expect(await routeIndex("/p")).toBe(first);
    expect(scanSources).toHaveBeenCalledTimes(1);
    expect(await routeIndex("/other")).toEqual([]);
    expect(scanSources).toHaveBeenCalledTimes(2);
  });

  it("matches sources by path segments, whatever separator the host used", async () => {
    scanSources.mockResolvedValue({
      status: "ok",
      data: [
        { ...source(), path: "D:\\work\\demo\\src\\DeepLinkController.java" },
        { ...source("other"), path: "D:/work/demo2/DeepLinkController.java" },
      ],
    });
    expect(await routeIndex("D:/work/demo")).toMatchObject([
      {
        methodName: "searchMenus",
        path: "D:/work/demo/src/DeepLinkController.java",
        rel: "src/DeepLinkController.java",
      },
    ]);
  });

  it.each(["created", "modified", "removed"] as const)("invalidates on %s mapping sources", async (kind) => {
    scanSources.mockResolvedValue({ status: "ok", data: [source()] });
    dispose = registerSpringEndpointProvider();
    expect((await searchEndpoints("/p"))[0]?.methodName).toBe("searchMenus");
    emitLocalIdeEvent({ type: "fsChanged", created: [], modified: [ipcPath("/p/pom.xml")], removed: [] });
    await searchEndpoints("/p");
    expect(scanSources).toHaveBeenCalledTimes(1);
    scanSources.mockResolvedValue({ status: "ok", data: [source("listMenus", "/menu/list")] });
    emitLocalIdeEvent({ type: "fsChanged", created: [], modified: [], removed: [], [kind]: [ipcPath("/p/A.java")] });
    expect((await searchEndpoints("/p"))[0]?.url).toBe("/deeplink/menu/list");
    expect(scanSources).toHaveBeenCalledTimes(2);
    dispose();
    expect(await searchEndpoints("/p")).toEqual([]);
  });
});

describe("shared model revisions and cancellation", () => {
  const contract = (url: string) => ({
    path: "/p/Api.java",
    text: `interface Api { @RequestMapping(path={"${url}","/second"},method={RequestMethod.POST,RequestMethod.PUT}) String get(); }`,
  });
  const child = { path: "/p/C.java", text: "@RestController class C implements Api { public String get(){} }" };
  it("shares concurrent scans and retries failures", async () => {
    scanSources.mockRejectedValueOnce(new Error("temporary"));
    await expect(routeIndex("/p")).rejects.toThrow("temporary");
    scanSources.mockResolvedValue({ status: "ok", data: [source()] });
    const [a, b] = await Promise.all([routeIndex("/p"), routeIndex("/p")]);
    expect(a).toBe(b);
    expect(scanSources).toHaveBeenCalledTimes(2);
  });
  it("does not install or return an obsolete result after filesystem changes or disposal", async () => {
    dispose = registerSpringEndpointProvider();
    let finish!: (value: { status: "ok"; data: ReturnType<typeof source>[] }) => void;
    scanSources.mockImplementationOnce(
      () =>
        new Promise((resolve) => {
          finish = resolve;
        }),
    );
    const old = routeIndex("/p");
    emitLocalIdeEvent({ type: "fsChanged", created: [], modified: [ipcPath("/p/Api.java")], removed: [] });
    scanSources.mockResolvedValue({ status: "ok", data: [source("newMethod", "/new")] });
    expect((await routeIndex("/p"))[0]?.methodName).toBe("newMethod");
    finish({ status: "ok", data: [source("oldMethod")] });
    expect(await old).toEqual([]);
    expect((await routeIndex("/p"))[0]?.methodName).toBe("newMethod");
    clearRouteIndex();
    scanSources.mockImplementationOnce(
      () =>
        new Promise((resolve) => {
          finish = resolve;
        }),
    );
    const pending = routeIndex("/p");
    dispose();
    finish({ status: "ok", data: [source()] });
    expect(await pending).toEqual([]);
  });
  it("indexes every method/path and keeps unsaved dependencies consistent across consumers", async () => {
    dispose = registerSpringEndpointProvider();
    scanSources.mockResolvedValue({ status: "ok", data: [contract("/old"), child] });
    expect((await routeIndex("/p")).map((r) => [r.method, r.url])).toEqual([
      ["POST", "/old"],
      ["POST", "/second"],
      ["PUT", "/old"],
      ["PUT", "/second"],
    ]);
    updateRouteSource("/p/Api.java", contract("/unsaved").text);
    expect((await routeIndex("/p"))[0]?.url).toBe("/unsaved");
    expect(knownControllers("/p", child.path, child.text)[0]?.endpoints[0]?.paths).toEqual(["/unsaved", "/second"]);
    expect((await controllersForSource("/p", child.path, child.text))[0]?.endpoints[0]?.paths).toEqual([
      "/unsaved",
      "/second",
    ]);
    expect(scanSources).toHaveBeenCalledTimes(1);
    scanSources.mockResolvedValue({ status: "ok", data: [contract("/diskChanged"), child] });
    emitLocalIdeEvent({ type: "fsChanged", created: [], modified: [ipcPath("/p/Api.java")], removed: [] });
    expect((await routeIndex("/p"))[0]?.url).toBe("/unsaved");
    expect(scanSources).toHaveBeenCalledTimes(2);
  });
  it("navigates inherited methods to UTF16 definition columns", async () => {
    const base = {
      path: "/p/Base.java",
      text: '@RequestMapping("/base") abstract class Base { /* 😀 */ @GetMapping("/x") public String get(){} }',
    };
    scanSources.mockResolvedValue({
      status: "ok",
      data: [base, { path: "/p/C.java", text: "@RestController class C extends Base {}" }],
    });
    expect(await routeIndex("/p")).toMatchObject([
      { path: base.path, line: 0, col: base.text.indexOf("get()"), url: "/base/x" },
    ]);
  });

  it("drops an oversized unsaved source instead of returning its old route", async () => {
    dispose = registerSpringEndpointProvider();
    scanSources.mockResolvedValue({ status: "ok", data: [source()] });
    expect(await routeIndex("/p")).toHaveLength(1);
    const oversized = source().text + " ".repeat(2 * 1024 * 1024);
    updateRouteSource(source().path, oversized);
    expect(await routeIndex("/p")).toEqual([]);
    expect(knownControllers("/p", source().path, oversized)).toEqual([]);
    expect(await controllersForSource("/p", source().path, oversized)).toEqual([]);
    updateRouteSource(source().path, source("edited", "/edited").text);
    expect((await routeIndex("/p"))[0]?.methodName).toBe("edited");
    expect(scanSources).toHaveBeenCalledTimes(1);
  });

  it("releases unsaved overlays and pending scans when the project switches", async () => {
    dispose = registerSpringEndpointProvider();
    scanSources.mockResolvedValue({ status: "ok", data: [source()] });
    updateRouteSource(source().path, source("unsaved", "/unsaved").text);
    expect((await routeIndex("/p"))[0]?.methodName).toBe("unsaved");
    useProject.setState({ project: { root: "/other" } as never });
    expect(await routeIndex("/other")).toEqual([]);
    useProject.setState({ project: { root: "/p" } as never });
    expect((await routeIndex("/p"))[0]?.methodName).toBe("searchMenus");
    useProject.setState({ project: null });
  });
});
