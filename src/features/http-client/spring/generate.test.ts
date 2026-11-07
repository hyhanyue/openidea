import { afterEach, describe, expect, it, vi } from "vitest";
import { _addEditor, _removeEditor, type TextEditorHandle } from "@/editor/api";
import { provideEditorService, provideNotificationService } from "@/workbench/contributions";
import { projectControllers, springControllers } from "./endpoints";
import { generateRequests, targetsOf } from "./editor";
import { MappingProject } from "./mappings";
import { parseSource } from "./source";

// `appDataDir()` is a native path: `\`-separated on Windows.
vi.mock("@tauri-apps/api/path", () => ({
  appDataDir: async () => String.raw`C:\Users\me\AppData\Roaming\ai.l0ng.openidea`,
}));

// No backend here: the file commands answer like a project without Spring
// configuration files (empty configs, the files the body lookup probes missing).
vi.mock("@/platform/ipc", async (importOriginal) => {
  const m = await importOriginal<typeof import("@/platform/ipc")>();
  return {
    ...m,
    call: async (p: Promise<unknown>) => await p,
    commands: {
      ...m.commands,
      fsCreateDir: async () => null,
      fsCreateFile: async () => null,
      fsReadText: async (path: string) => sourceFiles.get(path) ?? "",
    },
  };
});

const sourceFiles = new Map<string, string>();

const SCRATCH = "C:/Users/me/AppData/Roaming/ai.l0ng.openidea/scratches/generated-requests.http";

const JAVA = `package com.example.demo.web;

import org.springframework.web.bind.annotation.*;

@RestController
@RequestMapping("/api")
public class DemoController {
    @PostMapping("/h66")
    public String h66() { return "ok"; }
}
`;

const notifications: { level: string; title: string; body?: unknown }[] = [];

function fakeEditor(path: string, dispatched: { changes?: { insert?: string } }[]): TextEditorHandle {
  return {
    view: {
      state: { doc: { toString: () => "" } },
      dispatch: (spec: { changes?: { insert?: string } }) => void dispatched.push(spec),
    },
    ctx: { doc: 1, path, language: "http", uiMode: null, readOnly: false },
  } as unknown as TextEditorHandle;
}

describe("generate requests from a mapping", () => {
  afterEach(() => {
    notifications.length = 0;
    sourceFiles.clear();
  });

  it("opens the scratch at its UI path and appends the request", async () => {
    const opened: string[] = [];
    const dispatched: { changes?: { insert?: string } }[] = [];
    provideEditorService({ openFile: async (path: string) => void opened.push(path) } as never);
    provideNotificationService({
      notify: (n: { level: string; title: string; body?: unknown }) => void notifications.push(n),
    } as never);
    const editor = fakeEditor(SCRATCH, dispatched);
    _addEditor(editor);
    try {
      const controller = springControllers(JAVA)[0]!;
      await generateRequests("D:/p/src/main/java/com/example/demo/web/DemoController.java", JAVA, [
        { endpoint: controller.endpoints[0]! },
      ]);
    } finally {
      _removeEditor(editor);
    }

    expect(notifications).toEqual([]);
    // The scratch is opened — and found again by the editor registry — as the
    // `C:/…` path every other path in the UI uses, not as a native `C:\…` one.
    expect(opened).toEqual([SCRATCH]);
    expect(dispatched[0]?.changes?.insert).toContain("### DemoController.h66");
    expect(dispatched[0]?.changes?.insert).toContain("POST http://localhost:8080/api/h66");
  });

  it("generates all method/path variants and honors an individual menu choice", async () => {
    const text =
      '@RestController class C { @RequestMapping(path={"/a","/b"},method={RequestMethod.POST,RequestMethod.PUT}) void send(){} }';
    const endpoint = springControllers(text)[0]!.endpoints[0]!;
    const dispatched: { changes?: { insert?: string } }[] = [];
    provideEditorService({ openFile: async () => {} } as never);
    provideNotificationService({ notify: (n: (typeof notifications)[number]) => void notifications.push(n) } as never);
    const editor = fakeEditor(SCRATCH, dispatched);
    _addEditor(editor);
    try {
      await generateRequests("/p/C.java", text, targetsOf(endpoint));
      await generateRequests("/p/C.java", text, [targetsOf(endpoint)[3]!]);
    } finally {
      _removeEditor(editor);
    }
    const all = dispatched[0]?.changes?.insert ?? "";
    expect(all.match(/(?:POST|PUT) http:\/\/localhost:8080\/[ab]/g)).toEqual([
      "POST http://localhost:8080/a",
      "POST http://localhost:8080/b",
      "PUT http://localhost:8080/a",
      "PUT http://localhost:8080/b",
    ]);
    const chosen = dispatched[1]?.changes?.insert ?? "";
    expect(chosen).toContain("PUT http://localhost:8080/b");
    expect(chosen).not.toContain("POST ");
    expect(notifications).toEqual([]);
  });

  it("uses the declaring source imports for an inherited request body", async () => {
    const basePath = "/p/src/main/java/p/web/Base.java",
      childPath = "/p/src/main/java/p/web/C.java";
    const base =
      'package p.web; import p.dto.Body; abstract class Base { @PostMapping("/body") public void send(@RequestBody Body value){} }';
    const child = "package p.web; @RestController class C extends Base {}";
    sourceFiles.set(basePath, base);
    sourceFiles.set("/p/src/main/java/p/dto/Body.java", "package p.dto; public class Body { public String name; }");
    const project = new MappingProject([
      { path: basePath, text: base, file: parseSource(base) },
      { path: childPath, text: child, file: parseSource(child) },
    ]);
    const endpoint = projectControllers(project, childPath)[0]!.endpoints[0]!;
    const dispatched: { changes?: { insert?: string } }[] = [];
    provideEditorService({ openFile: async () => {} } as never);
    provideNotificationService({ notify: (n: (typeof notifications)[number]) => void notifications.push(n) } as never);
    const editor = fakeEditor(SCRATCH, dispatched);
    _addEditor(editor);
    try {
      await generateRequests(childPath, child, targetsOf(endpoint));
    } finally {
      _removeEditor(editor);
    }
    expect(dispatched[0]?.changes?.insert).toContain('"name": ""');
    expect(notifications).toEqual([]);
  });

  it("resolves the concrete DTO of a generic inherited body", async () => {
    const basePath = "/p/src/main/java/p/api/Base.java",
      childPath = "/p/src/main/java/p/web/C.java";
    const base =
      'package p.api; abstract class Base<T> { @PostMapping("/generic") public void send(@RequestBody T body){} }';
    const child = "package p.web; import p.api.Base; import p.dto.Body; @RestController class C extends Base<Body> {}";
    sourceFiles.set(basePath, base);
    sourceFiles.set("/p/src/main/java/p/dto/Body.java", "package p.dto; public class Body { public String concrete; }");
    const project = new MappingProject([
      { path: basePath, text: base, file: parseSource(base) },
      { path: childPath, text: child, file: parseSource(child) },
    ]);
    const endpoint = projectControllers(project, childPath)[0]!.endpoints[0]!;
    expect(endpoint.bodyType).toBe("p.dto.Body");
    const dispatched: { changes?: { insert?: string } }[] = [];
    provideEditorService({ openFile: async () => {} } as never);
    provideNotificationService({ notify: (n: (typeof notifications)[number]) => void notifications.push(n) } as never);
    const editor = fakeEditor(SCRATCH, dispatched);
    _addEditor(editor);
    try {
      await generateRequests(childPath, child, targetsOf(endpoint));
    } finally {
      _removeEditor(editor);
    }
    expect(dispatched[0]?.changes?.insert).toContain('"concrete": ""');
    expect(notifications).toEqual([]);
  });
});
