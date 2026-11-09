/** Source-scoped Spring model shared by search, editor menus and request generation. */
import { onEditorEvent } from "@/editor/api";
import { call, commands, onIdeEvent } from "@/platform/ipc";
import { ipcPath, isSameOrInside } from "@/platform/paths";
import { projectFolder, relativePath, useProject } from "@/platform/project";
import { registerEndpointProvider, type EndpointEntry } from "@/workbench/contributions";
import { projectControllers, springControllers, type Controller } from "./endpoints";
import { MappingProject, type MappingSource } from "./mappings";
import { parseSource } from "./source";

interface Index {
  project: MappingProject;
  routes: EndpointEntry[];
}
interface Pending<T> {
  root: string;
  key: string;
  abort: AbortController;
  promise: Promise<T>;
}
let sources: Pending<MappingSource[]> | null = null;
let cached: (Pending<Index> & { index?: Index }) | null = null;
const overlays = new Map<string, { path: string; text: string | null }>();
const listeners = new Set<() => void>();
const MAX_FILES = 1024;
const MAX_SOURCE_TEXT = 2 * 1024 * 1024;
const MAX_TEXT = 24 * 1024 * 1024; // UTF16 units, bounded like the host's byte budget.

export function onRouteIndexChanged(listener: () => void): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

function invalidateModel() {
  cached?.abort.abort();
  cached = null;
  for (const listener of listeners) listener();
}

/** Keep unsaved contracts/annotations visible across editors without rereading the project. */
export function updateRouteSource(path: string, text: string) {
  path = ipcPath(path);
  if (!isRouteSource(path) || overlays.get(path)?.text === text) return;
  if (overlays.size >= MAX_FILES && !overlays.has(path)) return;
  let size = text.length;
  for (const [key, value] of overlays) if (key !== path) size += value.text?.length ?? 0;
  overlays.set(path, { path, text: size > MAX_TEXT || text.length > MAX_SOURCE_TEXT ? null : text });
  invalidateModel();
}

export function registerSpringEndpointProvider(): () => void {
  clearRouteIndex();
  const unregister = registerEndpointProvider({ id: "http.spring", endpoints: routeIndex });
  const unsubscribe = onIdeEvent("fsChanged", (e) => {
    if ([...e.created, ...e.modified, ...e.removed].some((p) => isRouteSource(String(p)))) clearRouteIndex();
  });
  const unproject = useProject.subscribe((state, prior) => {
    if (
      state.project?.root !== prior.project?.root ||
      JSON.stringify(state.project?.workspace?.projects) !== JSON.stringify(prior.project?.workspace?.projects)
    ) {
      overlays.clear();
      clearRouteIndex();
    }
  });
  const uneditor = onEditorEvent((e) => {
    if ((e.type === "docClosed" || e.type === "docSaved") && isRouteSource(e.path)) {
      overlays.delete(ipcPath(e.path));
      clearRouteIndex();
    }
  });
  return () => {
    unsubscribe();
    unproject();
    uneditor();
    unregister();
    overlays.clear();
    clearRouteIndex();
  };
}

export function isRouteSource(path: string): boolean {
  return /\.(java|kt|kts)$/i.test(path);
}

/** Cancel obsolete parsing; failed scans are never retained and can be retried. */
export function clearRouteIndex() {
  sources?.abort.abort();
  sources = null;
  invalidateModel();
}

function cancelled(error: unknown): boolean {
  return error instanceof DOMException && error.name === "AbortError";
}
function check(signal: AbortSignal) {
  if (signal.aborted) throw new DOMException("Obsolete Spring scan", "AbortError");
}
const yieldInput = () => new Promise<void>((resolve) => setTimeout(resolve, 0));

export async function routeIndex(root: string | null): Promise<EndpointEntry[]> {
  if (!root) return [];
  try {
    return (await load(root)).routes;
  } catch (error) {
    if (cancelled(error)) return [];
    throw error;
  }
}

export function knownControllers(root: string | null, path: string, text: string): Controller[] {
  if (text.length > MAX_SOURCE_TEXT) return [];
  const index = root && cached?.root === ipcPath(root) ? cached.index : undefined;
  return index ? projectControllers(index.project, ipcPath(path), text) : springControllers(text);
}

export async function controllersForSource(root: string | null, path: string, text: string): Promise<Controller[]> {
  if (text.length > MAX_SOURCE_TEXT) return [];
  if (!root) return springControllers(text);
  try {
    return projectControllers((await load(root)).project, ipcPath(path), text);
  } catch (error) {
    return cancelled(error) ? [] : springControllers(text);
  }
}

function scopeKey(root: string): string {
  const current = useProject.getState().project;
  return JSON.stringify([root, current?.root === root ? current.workspace?.projects : null]);
}

function load(root: string): Promise<Index> {
  root = ipcPath(root).replace(/\/$/, "");
  const key = scopeKey(root);
  if (cached?.key === key) return cached.promise;
  cached?.abort.abort();
  const abort = new AbortController();
  const entry: NonNullable<typeof cached> = { root, key, abort, promise: scan(root, abort.signal) };
  cached = entry;
  void entry.promise.then(
    (index) => {
      if (cached === entry) entry.index = index;
    },
    () => {
      if (cached === entry) cached = null;
    },
  );
  return entry.promise;
}

function loadSources(root: string): Promise<MappingSource[]> {
  const key = scopeKey(root);
  if (sources?.key === key) return sources.promise;
  sources?.abort.abort();
  const abort = new AbortController();
  const entry = { root, key, abort, promise: readSources(root, abort.signal) };
  sources = entry;
  void entry.promise.catch(() => {
    if (sources === entry) sources = null;
  });
  return entry.promise;
}

function sourceRoot(root: string, path: string): string | null {
  const current = useProject.getState().project;
  if (current && ipcPath(current.root).replace(/\/$/, "") === root) {
    const folder = projectFolder(current, path)?.root;
    return folder && isSameOrInside(path, ipcPath(folder)) ? folder : null;
  }
  return isSameOrInside(path, root) ? root : null;
}

async function readSources(root: string, signal: AbortSignal): Promise<MappingSource[]> {
  const raw = await call(commands.springRouteSources());
  const found: MappingSource[] = [];
  let size = 0;
  for (const source of raw) {
    check(signal);
    const path = ipcPath(source.path);
    if (!sourceRoot(root, path) || !isRouteSource(path) || source.text.length > MAX_SOURCE_TEXT) continue;
    if (found.length >= MAX_FILES || size + source.text.length > MAX_TEXT) break;
    size += source.text.length;
    found.push({ path, text: source.text, file: parseSource(source.text) });
    if (found.length % 16 === 0) await yieldInput();
  }
  check(signal);
  return found;
}

async function scan(root: string, signal: AbortSignal): Promise<Index> {
  const disk = await loadSources(root);
  check(signal);
  const files = new Map(disk.map((s) => [s.path, s]));
  for (const [path, source] of overlays)
    if (sourceRoot(root, path)) {
      if (source.text === null) files.delete(path);
      else files.set(path, { ...source, text: source.text, file: parseSource(source.text) });
    }
  const all = [...files.values()];
  const project = new MappingProject(all),
    routes: EndpointEntry[] = [];
  const seen = new Set<string>();
  for (let i = 0; i < all.length; i++) {
    check(signal);
    const source = all[i]!;
    for (const controller of projectControllers(project, source.path))
      for (const endpoint of controller.endpoints) {
        const path = endpoint.origin?.path || source.path;
        const at = lineCol(files.get(path)?.text ?? source.text, endpoint.origin?.offset ?? endpoint.offset);
        for (const method of endpoint.httpMethods.length ? endpoint.httpMethods : ["*"])
          for (const url of endpoint.paths) {
            const key = JSON.stringify([
              source.path,
              endpoint.qualifiedController,
              controller.name,
              endpoint.methodName,
              method,
              url,
              path,
              at.line,
              at.col,
            ]);
            if (seen.has(key)) continue;
            seen.add(key);
            if (routes.length >= 20000) return { project, routes };
            routes.push({
              ...(endpoint.qualifiedController ? { qualifiedController: endpoint.qualifiedController } : {}),
              ...(endpoint.declaringClass ? { declaringClass: endpoint.declaringClass } : {}),
              method,
              url,
              controller: controller.name,
              methodName: endpoint.methodName,
              path,
              rel: relativePath(sourceRoot(root, path) ?? root, path),
              line: at.line,
              col: at.col,
            });
          }
      }
    if (i % 16 === 15) await yieldInput();
  }
  check(signal);
  return { project, routes };
}

function lineCol(text: string, offset: number): { line: number; col: number } {
  const prefix = text.slice(0, offset);
  // JS string offsets and lengths count UTF16 code units, as required by IPC.
  return { line: prefix.split("\n").length - 1, col: offset - prefix.lastIndexOf("\n") - 1 };
}
