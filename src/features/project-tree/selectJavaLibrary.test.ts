import { afterEach, expect, it, vi } from "vitest";
import { executeAction } from "@/platform/actions";
import { setContext } from "@/platform/context";
import { provideEditorService, registerProjectViewRoot, type EditorService } from "@/workbench/contributions";
import { registerProjectActions } from "./actions";
import { MemoryFs, setProjectFsForTests } from "./fs";
import { libKey } from "./model";
import { initProjectTree, setViewMode, useProjectTree } from "./store";

const PROJECT = "/project";
const JDK = "/Library/Java/JavaVirtualMachines/jdk-21.jdk/Contents/Home";
const MAVEN = "/home/me/.m2/repository/org/junit/jupiter/junit-jupiter-api/5.10.2";
const GRADLE = "/home/me/.gradle/caches/modules-2/files-2.1/demo/artifact/1";
const disposers: Array<() => void> = [];

function openLibrary(uri: string, roots: string[], entries: string[]) {
  const fs = new MemoryFs(Object.fromEntries(entries.map((path) => [path, ""])), PROJECT);
  setProjectFsForTests(fs, PROJECT);
  initProjectTree(PROJECT, "demo");
  setViewMode("project");
  disposers.push(
    registerProjectViewRoot({
      id: "externalLibraries",
      label: "External Libraries",
      icon: "library",
      readOnly: true,
      children: [
        {
          label: "Java",
          icon: "file-java",
          children: roots.map((path) => ({ label: path.split("/").at(-1)!, path, icon: "file-archive" })),
        },
      ],
    }),
    registerProjectActions(),
  );
  setContext("projectOpen", true);
  provideEditorService({ activePath: () => uri } as EditorService);
  return fs;
}

afterEach(() => {
  disposers.splice(0).forEach((dispose) => dispose());
  setViewMode("project");
  initProjectTree(null);
  setProjectFsForTests(null);
  setContext("projectOpen", false);
});

function jdt(archive: string, path: string, handle: string) {
  const name = encodeURIComponent(archive.split("/").at(-1)!);
  const escaped = encodeURIComponent(archive.replaceAll("/", "\\/"));
  return `jdt://contents/${name}/${path}?=demo/${escaped}${handle}`;
}

it.each([
  {
    name: "JDK 21 MappedByteBuffer on macOS",
    uri: "jdt://contents/java.base/java.nio/MappedByteBuffer.java?=library-reveal/%5C/Library%5C/Java%5C/JavaVirtualMachines%5C/jdk-21.jdk%5C/Contents%5C/Home%5C/lib%5C/jrt-fs.jar%60java.base=/javadoc_location=/https:%5C/%5C/docs.oracle.com%5C/en%5C/java%5C/javase%5C/21%5C/docs%5C/api%5C/=/=/maven.pomderived=/true=/%3Cjava.nio%28MappedByteBuffer.class",
    root: `${JDK}/lib/src.zip!`,
    entry: "java.base/java/nio/MappedByteBuffer.java",
  },
  {
    name: "unmodular JDK sources",
    uri: jdt("/sdk/jdk8/jre/lib/rt.jar", "java.lang/String.class", "%3Cjava.lang%28String.class"),
    root: "/sdk/jdk8/src.zip!",
    entry: "java/lang/String.java",
  },
  {
    name: "a flat JDK sources ZIP with a modular handle",
    uri: jdt("/sdk/flat/lib/jrt-fs.jar", "java.lang/String.java", "%60java.base%3Cjava.lang%28String.class"),
    root: "/sdk/flat/lib/src.zip!",
    entry: "java/lang/String.java",
    other: "/other/sdk/lib/src.zip!",
  },
  {
    name: "JUnit in the binary Maven JAR",
    uri: jdt(
      `${MAVEN}/junit-jupiter-api-5.10.2.jar`,
      "org.junit.jupiter.api/Test.java",
      "=/test=/true=/%3Corg.junit.jupiter.api%28Test.class",
    ),
    root: `${MAVEN}/junit-jupiter-api-5.10.2.jar!`,
    entry: "org/junit/jupiter/api/Test.class",
    other: "/different/repository/junit-jupiter-api-5.10.2.jar!",
  },
  {
    name: "the nested class in the binary JAR",
    uri: jdt("/m2/lib.jar", "example/Outer.java", "%3Cexample%28Outer%24Inner.class"),
    root: "/m2/lib.jar!",
    entry: "example/Outer$Inner.class",
  },
  {
    name: "the binary class behind a Kotlin source filename",
    uri: jdt("/m2/lib.jar", "example/Facade.kt", "%3Cexample%28RenamedFacade.class"),
    root: "/m2/lib.jar!",
    entry: "example/RenamedFacade.class",
  },
  {
    name: "a Java source in the sole Maven sources JAR",
    uri: jdt("/m2/lib.jar", "example/Outer%24Inner.class", "%3Cexample%28Outer%24Inner.class"),
    root: "/m2/lib-sources.jar!",
    entry: "example/Outer.java",
  },
  {
    name: "a Java source in a sibling Gradle hash",
    uri: jdt(`${GRADLE}/binary/artifact-1.jar`, "example/Type.java", "%3Cexample%28Type.class"),
    root: `${GRADLE}/source/artifact-1-sources.jar!`,
    entry: "example/Type.java",
    other: `${GRADLE.replace("/demo/", "/other/")}/source/artifact-1-sources.jar!`,
  },
  {
    name: "a Windows archive and class in the default package",
    uri: jdt("C:/Maven 测试 (1)/lib.jar".replace("(", "\\("), "Root.java", "%3C%28Root.class"),
    root: "C:/Maven 测试 (1)/lib.jar!",
    entry: "Root.class",
  },
])("Select Opened File reveals $name under External Libraries", async ({ uri, root, entry, other }) => {
  const target = `${root}/${entry}`;
  openLibrary(uri, other ? [other, root] : [root], other ? [`${other}/${entry}`, target] : [target]);
  setViewMode("projectFiles");
  expect(await executeAction("navigate.selectInProjectView")).toBe(true);
  expect(useProjectTree.getState().mode).toBe("project");
  expect(useProjectTree.getState().revealRequest).toMatchObject({
    path: target,
    focus: true,
    key: libKey("externalLibraries/Java", target),
  });
});

it.each([
  jdt("/remote/lib.jar", "example/Type.java", "%3Cexample%28Type.class"),
  "jdt://contents/lib.jar/example/Type.class",
  jdt("/local/lib.jar", "%2e%2e/Type.java", "%3Cexample%28Type.class"),
  jdt("/local/lib.jar", "example/Bad%2FType.java", "%3Cexample%28Type.class"),
])("leaves the selection alone and does not read the filesystem for %s", async (uri) => {
  const fs = openLibrary(uri, ["/local/lib.jar!"], ["/local/lib.jar!/example/Type.class"]);
  const read = vi.spyOn(fs, "readDir");
  await executeAction("navigate.selectInProjectView");
  expect(useProjectTree.getState().revealRequest).toBeNull();
  expect(read).not.toHaveBeenCalled();
});

it("leaves the selection alone when a registered archive lacks the class", async () => {
  const root = "/local/lib.jar!";
  const fs = openLibrary(
    jdt("/local/lib.jar", "example/Missing.java", "%3Cexample%28Missing.class"),
    [root],
    [`${root}/example/Other.class`],
  );
  const read = vi.spyOn(fs, "readDir");
  await executeAction("navigate.selectInProjectView");
  expect(useProjectTree.getState().revealRequest).toBeNull();
  expect(read.mock.calls).toEqual([[`${root}/example`]]);
});

it("keeps a newer selection when an archive listing is still pending", async () => {
  const root = "/local/lib.jar!";
  const fs = openLibrary(
    jdt("/local/lib.jar", "example/Type.java", "%3Cexample%28Type.class"),
    [root],
    [`${root}/example/Type.class`, "/project/Other.java"],
  );
  const entries = await fs.readDir(`${root}/example`);
  let resolveListing!: (result: typeof entries) => void;
  const listing = new Promise<typeof entries>((resolve) => (resolveListing = resolve));
  vi.spyOn(fs, "readDir").mockImplementationOnce(() => listing);
  const pending = executeAction("navigate.selectInProjectView");
  await executeAction("navigate.selectInProjectView", "/project/Other.java");
  resolveListing(entries);
  await pending;
  expect(useProjectTree.getState().revealRequest?.path).toBe("/project/Other.java");
});
