/**
 * Virtual documents: read-only contents a language server provides for a
 * non-`file:` URI (library classes from jdtls' `jdt://contents/…`, decompiled
 * code). Locations into them carry the URI as their `path`; the editor opens
 * them as `virtual` editor inputs instead of files.
 */
import { isClassFile } from "./classFiles";
import { call, commands, type VirtualDocument } from "./ipc";
import { ipcPath } from "./paths";

/** Server document URIs, including decompiled disk classes; drive letters remain paths. */
export function isVirtualUri(s: string | null | undefined): boolean {
  const m = /^([a-zA-Z][a-zA-Z0-9+.-]+):/.exec(s ?? "");
  return !!m && (m[1]!.toLowerCase() !== "file" || isClassFile(s!));
}

function decodeUriPart(part: string): string | null {
  try {
    return decodeURIComponent(part);
  } catch {
    return null;
  }
}

/** Last path segment of a URI (without query / fragment), e.g. `SpringApplication.class`. */
export function virtualName(uri: string): string {
  const rest = uri.replace(/^[^:]+:(\/\/)?/, "").split(/[?#]/)[0]!;
  const seg = rest.split("/").filter(Boolean).pop() ?? uri;
  return decodeUriPart(seg) ?? seg;
}

/** The URI's path without its last segment, e.g. `jdt://contents/x.jar/org.example`. */
export function virtualParent(uri: string): string {
  const base = uri.split(/[?#]/)[0]!;
  const i = base.lastIndexOf("/");
  return i > base.indexOf("://") + 2 ? base.slice(0, i) : base;
}

/**
 * Short human location of a virtual document, e.g. `spring-boot-4.1.1.jar › org.springframework.boot`
 * for `jdt://contents/spring-boot-4.1.1.jar/org.springframework.boot/SpringApplication.class?=…`.
 */
export function virtualContainer(uri: string): string {
  const scheme = uri.slice(0, uri.indexOf(":"));
  // `jar:///…/spring-boot-3.5.16.jar!/org/springframework/boot/X.class` → `spring-boot-3.5.16.jar › org.springframework.boot`
  const jar = scheme === "jar" ? /^jar:(?:file:)?(?:\/\/)?(.*?)!\/(.*)$/.exec(uri.split(/[?#]/)[0]!) : null;
  if (jar) {
    const archive = jar[1]!.split("/").pop() ?? jar[1]!;
    const pkg = jar[2]!
      .split("/")
      .slice(0, -1)
      .map((part) => decodeUriPart(part) ?? part)
      .join(".");
    const name = decodeUriPart(archive) ?? archive;
    return pkg ? `${name} › ${pkg}` : name;
  }
  const rest = uri.replace(/^[^:]+:(\/\/)?/, "").split(/[?#]/)[0]!;
  const segs = rest
    .split("/")
    .filter(Boolean)
    .map((part) => decodeUriPart(part) ?? part);
  segs.pop();
  if (scheme === "jdt" && segs[0] === "contents") segs.shift();
  return segs.join(" › ");
}

/** Archive identity and class/source entries from a JDT LS class-file URI. */
export function jdtClassLocation(
  uri: string,
): { archive: string; module: string | null; classEntry: string; sourceEntry: string } | null {
  const base = uri.split("#", 1)[0]!;
  const query = base.indexOf("?");
  if (query < 0) return null;
  const parts = /^jdt:\/\/contents\/([^/]+)\/(?:([^/]*)\/)?([^/]+)$/.exec(base.slice(0, query));
  if (!parts) return null;
  const container = decodeUriPart(parts[1]!);
  const pkg = decodeUriPart(parts[2] ?? "");
  const name = decodeUriPart(parts[3]!);
  if (pkg === null || !name) return null;

  // The query is an Eclipse Java element handle. Its root path follows the project
  // name. A backslash escapes handle delimiters, including path separators.
  const handle = decodeUriPart(base.slice(query + 1));
  if (!handle) return null;
  const root = /^=[^/]+\/((?:\\.|[^\\<(`=])+)(?:`([^=<(/]+))?(?=[=<(])/.exec(handle);
  if (!root) return null;
  const archive = ipcPath(root[1]!.replace(/\\(.)/g, "$1")).replace(/^\/(?=[A-Za-z]:\/)/, "");
  if (!/^(?:\/|[A-Za-z]:\/)/.test(archive) || !/\.(?:jar|jmod)$/i.test(archive) || /\0/.test(archive)) return null;
  const module =
    root[2] ?? (archive.endsWith("/jrt-fs.jar") ? container : /\/jmods\/([^/]+)\.jmod$/.exec(archive)?.[1]);

  // The URI path can name the original source file (Outer.java), while the handle
  // still names the binary file (Outer$Inner.class).
  const className =
    /^(?:\\.|[^\\<])*<(?:\\.|[^\\('])*[(']([^/\\[&]+\.class)(?=$|[[&])/.exec(handle.slice(root[0].length))?.[1] ??
    (/\.(?:java|class)$/.test(name) ? name.replace(/\.java$/, ".class") : null);
  const segments = pkg ? pkg.split(".") : [];
  if (
    !className ||
    [...segments, name, className].some((part) => !part || part === "." || part === ".." || /[\\/\0]/.test(part))
  )
    return null;
  const prefix = segments.length ? `${segments.join("/")}/` : "";
  return {
    archive,
    module: module && /^[\w$]+(?:\.[\w$]+)*$/.test(module) ? module : null,
    classEntry: prefix + className,
    sourceEntry: prefix + name.replace(/\.class$/, ".java"),
  };
}

const cache = new Map<string, Promise<VirtualDocument>>();

export function invalidateVirtualDocument(uri: string): void {
  cache.delete(uri);
}

/** Contents of a virtual document (cached; failures are retried on the next call). */
export function virtualDocument(uri: string): Promise<VirtualDocument> {
  let p = cache.get(uri);
  if (!p) {
    p = call(commands.lspVirtualDocument(uri));
    p.catch(() => cache.delete(uri));
    cache.set(uri, p);
  }
  return p;
}

const decode = (s: string) => {
  try {
    return decodeURIComponent(s);
  } catch {
    return s;
  }
};

/** `/C:/x` (a drive path inside a URI) → `C:/x`. */
const uriFsPath = (p: string) => (/^\/[A-Za-z]:\//.test(p) ? p.slice(1) : p);

const parentDir = (p: string, levels = 1) => p.split("/").slice(0, -levels).join("/");

/** `(jar, entry without extension)` → the class in its jar (the binary jar for a `-sources.jar`). */
const jarCandidates = (jar: string, stem: string, archives: readonly string[]) => {
  const binary = jar.replace(/-sources\.jar$/, ".jar");
  const candidates = [`${binary}!/${stem}.class`];
  // Gradle caches binary and sources jars in separate hash directories of one version.
  const gradle = /^(.*\/files-2\.1\/[^/]+\/[^/]+\/[^/]+)\/[^/]+\/[^/]+$/.exec(jar);
  if (jar !== binary && gradle && !archives.includes(`${binary}!`)) {
    const name = binary.slice(binary.lastIndexOf("/") + 1);
    for (const root of archives)
      if (root.endsWith(`/${name}!`) && parentDir(root, 2) === gradle[1]) candidates.push(`${root}/${stem}.class`);
  }
  // JDK 8: `<home>/jre/lib/rt.jar` with its sources in `<home>/src.zip`.
  if (jar.endsWith("/rt.jar")) candidates.push(`${parentDir(jar, 3)}/src.zip!/${stem}.java`);
  return candidates;
};

/**
 * Where a library document URI lies in the Project view's External Libraries,
 * as `<archive>!/<entry>` paths, most specific first: jdtls' `jdt://contents/…`
 * classes (dependency jars, the JDK's modules in `lib/src.zip`), `jar:` entries
 * (classes and sources jars) and decompiled `file:` classes. Empty for anything
 * else; a missing entry still leads to its package folder.
 */
export function libraryEntryPaths(uri: string, archives: readonly string[] = []): string[] {
  const base = uri.split(/[?#]/)[0]!;
  const scheme = base.slice(0, Math.max(base.indexOf(":"), 0)).toLowerCase();
  if (scheme === "file") return isClassFile(base) ? [uriFsPath(decode(base.replace(/^file:(\/\/)?/, "")))] : [];
  if (scheme === "jar") {
    const m = /^jar:(?:file:)?(?:\/\/)?(.*?)!\/(.*)$/.exec(base);
    if (!m) return [];
    const jar = uriFsPath(decode(m[1]!));
    const entry = decode(m[2]!);
    const exact = `${jar}!/${entry}`;
    const stem = entry.replace(/\.[^./]*$/, "");
    return [exact, ...jarCandidates(jar, stem, archives).filter((p) => p !== exact)];
  }
  if (scheme !== "jdt") return [];
  // jdt://contents/<jar or module>/<package>/<Class.class>?=<project>/<escaped jar path><package(Class.class
  const segs = base
    .replace(/^jdt:\/\/contents\//, "")
    .split("/")
    .map(decode);
  if (segs.length !== 3) return [];
  const [module, pkg, file] = segs as [string, string, string];
  const stem = [...(pkg ? pkg.split(".") : []), file.replace(/\.(class|java)$/, "")].join("/");
  const query = decode(uri.split("?")[1] ?? "");
  const rest = query.slice(query.indexOf("/") + 1);
  const jar = query.includes("/") ? uriFsPath(rest.replace(/\\/g, "").split(/[<(`=]/)[0]!) : "";
  if (!jar) return [];
  // JDK 9+: `<home>/lib/jrt-fs.jar`, sources per module in `<home>/lib/src.zip`.
  if (jar.endsWith("/jrt-fs.jar")) return [`${parentDir(jar)}/src.zip!/${module}/${stem}.java`];
  return jarCandidates(jar, stem, archives);
}
