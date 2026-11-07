/**
 * Spring Web request mappings of a Java / Kotlin source: `@RequestMapping` /
 * `@GetMapping`… methods of `@RestController` / `@Controller` classes, with
 * the class-level mapping applied. Constants are resolved within the file.
 */
import { MappingProject, type MappedController, type MappedMethod } from "./mappings";
import { parseSource, type Annotation, type SourceFile } from "./source";

export interface QueryParam {
  name: string;
  /** `defaultValue`, if any. */
  value: string | null;
}

export interface Endpoint {
  qualifiedController?: string;
  declaringClass?: string;
  className: string;
  methodName: string;
  /** Offset of the method name (gutter line). */
  offset: number;
  /** The method declaration, annotations included. */
  range: { from: number; to: number };
  /** Offset of the controller class name. */
  classOffset: number;
  /** A convenient method for request generation; GET when the mapping is unrestricted. */
  httpMethod: string;
  /** Explicit accepted methods; empty means unrestricted, not GET-only. */
  httpMethods: string[];
  origin?: { path: string; offset: number };
  /** Class × method paths (`/` joined, `{var}` kept), first one first. */
  paths: string[];
  query: QueryParam[];
  headers: { name: string; value: string }[];
  /** `consumes` (first). */
  consumes: string | null;
  /** `produces` (first). */
  produces: string | null;
  /** `@RequestBody` parameter type as written, if any. */
  bodyType: string | null;
}

export interface Controller {
  name: string;
  offset: number;
  /** The class declaration, annotations included. */
  range: { from: number; to: number };
  endpoints: Endpoint[];
}

export function joinPaths(base: string, path: string): string {
  const joined = `/${base}/${path}`.replace(/\/{2,}/g, "/");
  return joined.length > 1 ? joined.replace(/\/$/, "") : "/";
}

/** Map-typed parameters collect all entries: nothing to generate. */
const isMap = (type: string) =>
  /^(java\.util\.)?(Map|MultiValueMap|HttpHeaders|LinkedMultiValueMap|HashMap)\b/.test(type);

function endpoint(controller: MappedController, mapped: MappedMethod): Endpoint {
  const cls = controller.owner.cls,
    method = mapped.method;
  const base = controller.mapping,
    own = mapped.mapping;
  const paths: string[] = [];
  for (const b of base?.paths.length ? base.paths : [""])
    for (const p of own.paths.length ? own.paths : [""]) {
      if (paths.length >= 1024) break;
      paths.push(joinPaths(b, p));
    }
  const httpMethods = [...new Set([...(base?.methods ?? []), ...own.methods])];
  const query: QueryParam[] = [],
    headers: { name: string; value: string }[] = [];
  let bodyType: string | null = null;
  for (const p of mapped.params) {
    const annotation = (name: string) =>
      p.annotations.find(
        (a) =>
          a.name === name &&
          (!a.qualifiedName.includes(".") || a.qualifiedName === `org.springframework.web.bind.annotation.${name}`),
      );
    const rp = annotation("RequestParam"),
      rh = annotation("RequestHeader");
    const value = (a: Annotation, key: string) =>
      (p.annotationOwners.get(a) ?? p.owner).constants.string(a.args.get(key));
    if (rp && !isMap(p.type))
      query.push({ name: value(rp, "value") || value(rp, "name") || p.name, value: value(rp, "defaultValue") });
    else if (rh && !isMap(p.type)) {
      const name = value(rh, "value") || value(rh, "name") || p.name;
      headers.push({ name, value: value(rh, "defaultValue") ?? `{{${name}}}` });
    } else if (annotation("RequestBody")) bodyType = p.type || "Object";
  }
  for (const condition of [...(base?.params ?? []), ...own.params]) {
    if (condition.startsWith("!") || condition.includes("!=")) continue;
    const eq = condition.indexOf("="),
      name = (eq < 0 ? condition : condition.slice(0, eq)).trim();
    const value = eq < 0 ? null : condition.slice(eq + 1).trim();
    const existing = query.find((q) => q.name === name);
    if (name && existing && value !== null) existing.value = value;
    else if (name && !existing) query.push({ name, value });
  }
  for (const condition of [...(base?.headers ?? []), ...own.headers]) {
    const eq = condition.indexOf("=");
    if (eq < 0 || condition.startsWith("!") || condition.includes("!=")) continue;
    const name = condition.slice(0, eq).trim(),
      value = condition.slice(eq + 1).trim();
    const existing = headers.find((h) => h.name.toLowerCase() === name.toLowerCase());
    if (existing) existing.value = value;
    else if (name) headers.push({ name, value });
  }
  const inherited = mapped.owner.cls !== cls;
  const qualifiedController = controller.owner.qualifiedName,
    declaringClass = mapped.owner.qualifiedName;
  return {
    ...(qualifiedController ? { qualifiedController } : {}),
    ...(declaringClass ? { declaringClass } : {}),
    className: cls.name,
    methodName: method.name,
    offset: inherited ? cls.nameStart : method.nameStart,
    range: inherited ? { from: -1, to: -1 } : { from: method.start, to: method.end },
    ...(inherited ? { origin: { path: mapped.owner.source.path, offset: method.nameStart } } : {}),
    classOffset: cls.nameStart,
    httpMethod: own.methods[0] ?? base?.methods[0] ?? "GET",
    httpMethods,
    paths: [...new Set(paths)],
    query,
    headers,
    consumes: own.consumes[0] ?? base?.consumes[0] ?? null,
    produces: own.produces[0] ?? base?.produces[0] ?? null,
    bodyType,
  };
}

export function projectControllers(project: MappingProject, path: string, text?: string): Controller[] {
  return project.controllers(path, text).map((controller) => {
    const cls = controller.owner.cls;
    return {
      name: cls.name,
      offset: cls.nameStart,
      range: { from: cls.start, to: cls.end },
      endpoints: controller.methods.map((m) => endpoint(controller, m)),
    };
  });
}

export function controllersOf(file: SourceFile): Controller[] {
  return projectControllers(new MappingProject([{ path: "", text: "", file }]), "");
}

/** Controllers of one source; project indexing additionally resolves external declarations. */
export function springControllers(text: string): Controller[] {
  if (!text.includes("@")) return [];
  return controllersOf(parseSource(text));
}

/** The controller and endpoint whose declaration holds `offset`. */
export function springAt(
  controllers: Controller[],
  offset: number,
): { controller: Controller | null; endpoint: Endpoint | null } {
  // Innermost controller (nested classes come after their outer one).
  const controller = controllers.filter((c) => c.range.from <= offset && offset <= c.range.to).pop() ?? null;
  const endpoint = controller?.endpoints.find((e) => e.range.from <= offset && offset <= e.range.to) ?? null;
  return { controller, endpoint };
}
