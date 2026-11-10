import { describe, expect, it } from "vitest";
import { addFileIconProvider, registerExternalIcons } from "@/ui/icons";
import { presentationIcon } from "./filePresentation";
import type { RootFolder } from "./languageSettings";
import type { JvmPresentation } from "./ipc";

const roots: RootFolder[] = [
  { path: "src/main/java", kind: "source", detectedFrom: "pom.xml" },
  { path: "src/test/java", kind: "test", detectedFrom: "pom.xml" },
  { path: "src/main/resources", kind: "resources", detectedFrom: "pom.xml" },
  { path: "src/test/resources", kind: "testResources", detectedFrom: "pom.xml" },
];
const type = (kind: JvmPresentation["kind"]): JvmPresentation => ({ kind, springBoot: false, runnable: false });
const icon = (path: string, info: JvmPresentation | null = null, spring = false) =>
  presentationIcon(`/p/${path}`, info, roots, "/p", spring);

describe("JVM file presentation", () => {
  it.each([
    ["class", "class"],
    ["abstractClass", "class-abstract"],
    ["interface", "interface"],
    ["enum", "enum"],
    ["record", "record"],
    ["annotation", "annotation"],
    ["exception", "exception"],
    ["object", "kotlin-object"],
  ] as const)("maps %s to %s", (kind, expected) => expect(icon("Foo.java", type(kind))).toBe(expected));
  it.each(["LIMIT.java", "AUDIT.java", "Contest.java"])("keeps the type icon of %s", (path) =>
    expect(icon(path, type("class"))).toBe("class"),
  );
  it.each(["FooTest.java", "FooTests.java", "FooIT.java", "FooTest.kt", "src/test/java/Anything.java"])(
    "marks test classes: %s",
    (path) => expect(icon(path, type("class"))).toBe("jvm-test"),
  );
  it("keeps files without declarations and binaries as file types", () => {
    expect(icon("Foo.java", type(null))).toBe("file-java");
    expect(icon("FooTest.java", type(null))).toBe("file-java");
    expect(icon("Foo.kt")).toBe("file-kotlin");
    expect(icon("Foo.class")).toBe("file-binary");
    expect(icon("Contest.java", type("class"))).toBe("class");
  });
  it.each(["application.yml", "application-dev.yaml", "bootstrap.properties", "bootstrap-local.yml"])(
    "requires Spring or a resource root: %s",
    (name) => {
      expect(icon(`src/main/resources/${name}`)).toBe("spring-config");
      expect(icon(`src/test/resources/${name}`)).toBe("spring-config");
      expect(icon(name, null, true)).toBe("spring-config");
      expect(icon(name)).not.toBe("spring-config");
    },
  );
  it("uses the entire Boot icon on application classes", () => {
    expect(icon("App.java", { ...type("class"), springBoot: true, runnable: true })).toBe("spring-boot");
  });
});

it("keeps plugin Mapper file icons ahead of JVM interface presentation and restores after unload", () => {
  const name = "ext:fileIcon/mybatis/mapper-java";
  const off = registerExternalIcons([[name, '<svg viewBox="0 0 16 16"><path d="M1 1h12v12H1z"/></svg>']]);
  const remove = addFileIconProvider((file) => (file.endsWith("Mapper.java") ? name : null));
  try {
    expect(icon("UserMapper.java", type("interface"))).toBe(name);
    expect(icon("UserService.java", type("class"))).toBe("class");
    expect(icon("Other.java", type("interface"))).toBe("interface");
  } finally {
    remove();
    off();
  }
  expect(icon("UserMapper.java", type("interface"))).toBe("interface");
});

it("passes the full path to explicit icon providers before JVM or Spring inference", () => {
  const remove = addFileIconProvider((_name, path) =>
    path === "/p/src/main/resources/application.yml" ? "file-docker" : null,
  );
  try {
    expect(icon("src/main/resources/application.yml", null, true)).toBe("file-docker");
    expect(icon("src/main/resources/application-other.yml", null, true)).toBe("spring-config");
  } finally {
    remove();
  }
});
