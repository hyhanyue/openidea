/** Spring request-path index, cached per project until a mapping source changes. */
import { call, commands, onIdeEvent } from "@/platform/ipc";
import { ipcPath, isSameOrInside } from "@/platform/paths";
import { registerEndpointProvider, type EndpointEntry } from "@/workbench/contributions";
import { projectControllers, springControllers, type Controller } from "./endpoints";
import { MappingProject, type MappingSource } from "./mappings";
import { parseSource } from "./source";

export function registerSpringEndpointProvider(): () => void {
  clearRouteIndex();
  const unregister = registerEndpointProvider({ id: "http.spring", endpoints: routeIndex });
  const unsubscribe = onIdeEvent("fsChanged", (e) => {
    if ([...e.created, ...e.modified, ...e.removed].some((p) => isRouteSource(String(p)))) clearRouteIndex();
  });
  return () => {
    unsubscribe();
    unregister();
    clearRouteIndex();
  };
}

interface Index { project: MappingProject; routes: EndpointEntry[] }
let cached: { root: string; promise: Promise<Index>; index?: Index } | null = null;

/** Whether `path` is a source a mapping can live in (a Java or Kotlin file). */
export function isRouteSource(path: string): boolean {
  return /\.(java|kt|kts)$/i.test(path);
}

/** Forgets the index, so the next search rescans the project. */
export function clearRouteIndex() {
  cached = null;
}

/** The project's routes, scanned once per project and kept in memory. */
export async function routeIndex(root: string | null): Promise<EndpointEntry[]> {
  return root ? (await load(root)).routes : [];
}

export function knownControllers(root: string | null, path: string, text: string): Controller[] {
  const index = root && cached?.root === root ? cached.index : undefined;
  return index ? projectControllers(index.project, ipcPath(path), text) : springControllers(text);
}

export async function controllersForSource(root: string | null, path: string, text: string): Promise<Controller[]> {
  if (!root) return springControllers(text);
  try { return projectControllers((await load(root)).project, ipcPath(path), text); }
  catch { return springControllers(text); }
}

function load(root: string): Promise<Index> {
  if (cached?.root === root) return cached.promise;
  const entry: NonNullable<typeof cached> = { root, promise: scan(root) };
  cached = entry;
  void entry.promise.then((index) => { if (cached === entry) entry.index = index; }, () => { if (cached === entry) cached = null; });
  return entry.promise;
}

async function scan(root: string): Promise<Index> {
  const raw = await call(commands.springRouteSources());
  const sources: MappingSource[] = [];
  const base = ipcPath(root).replace(/\/$/, "");
  for (const source of raw) {
    const path = ipcPath(source.path);
    if (!isSameOrInside(path, base)) continue;
    sources.push({ path, text: source.text, file: parseSource(source.text) });
    // Parsing large source sets yields to editor input between batches.
    if (sources.length % 16 === 0) await new Promise<void>((resolve) => setTimeout(resolve, 0));
  }
  const project = new MappingProject(sources), routes: EndpointEntry[] = [];
  const texts = new Map(sources.map((s) => [s.path, s.text]));
  const seen = new Set<string>();
  for (let i = 0; i < sources.length; i++) {
    const source = sources[i]!;
    for (const controller of projectControllers(project, source.path)) for (const endpoint of controller.endpoints) {
      const path = endpoint.origin?.path || source.path;
      const at = lineCol(texts.get(path) ?? source.text, endpoint.origin?.offset ?? endpoint.offset);
      for (const method of endpoint.httpMethods.length ? endpoint.httpMethods : ["*"]) for (const url of endpoint.paths) {
        const key = JSON.stringify([controller.name, endpoint.methodName, method, url, path, at.line, at.col]);
        if (seen.has(key)) continue;
        seen.add(key);
        routes.push({ method, url, controller: controller.name, methodName: endpoint.methodName, path,
          rel: path.slice(base.length + 1), line: at.line, col: at.col });
      }
    }
    if (i % 16 === 15) await new Promise<void>((resolve) => setTimeout(resolve, 0));
  }
  return { project, routes };
}

function lineCol(text: string, offset: number): { line: number; col: number } {
  let line = 0;
  let col = 0;
  for (let i = 0; i < offset && i < text.length; i++) {
    if (text.charCodeAt(i) === 10 /* \n */) {
      line++;
      col = 0;
    } else {
      col++;
    }
  }
  return { line, col };
}
