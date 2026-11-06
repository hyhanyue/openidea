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
