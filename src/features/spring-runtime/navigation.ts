import { call, commands, nextRequestId } from "@/platform/ipc";
import { isSameOrInside } from "@/platform/paths";
import { searchEndpoints } from "@/workbench/contributions";
import type { WorkspaceSymbol } from "@/bindings";
import { proxyClass, sameClassName } from "./classIdentity";
import { mappingCandidates, type RuntimeRow, type SourceTarget } from "./model";

export function exactClass(symbols: WorkspaceSymbol[], className: string, root: string): WorkspaceSymbol | null {
  if (proxyClass(className)) return null;
  const eligible = symbols.filter(
    (s) =>
      ["class", "interface", "enum"].includes(s.kind) &&
      s.location.path.endsWith(".java") &&
      isSameOrInside(s.location.path, root),
  );
  const name = (s: WorkspaceSymbol) =>
    s.container && !s.name.startsWith(`${s.container}.`) ? `${s.container}.${s.name}` : s.name;
  const raw = eligible.filter((s) => name(s) === className);
  const matches = raw.length ? raw : eligible.filter((s) => sameClassName(name(s), className));
  const unique = new Map(
    matches.map((s) => [`${s.location.path}:${s.location.range.start.line}:${s.location.range.start.col}`, s]),
  );
  return unique.size === 1 ? [...unique.values()][0]! : null;
}

export async function sourceTarget(row: RuntimeRow, root: string): Promise<SourceTarget | null> {
  if (!row.className || proxyClass(row.className)) return null;
  const types = new Map<string, Promise<WorkspaceSymbol | null>>();
  const resolveClass = (name: string) => {
    if (proxyClass(name)) return Promise.resolve(null);
    let result = types.get(name);
    if (!result) {
      result = call(commands.lspWorkspaceSymbols(nextRequestId(), name.split(/[.$]/).at(-1)!)).then((symbols) =>
        exactClass(symbols, name, root),
      );
      types.set(name, result);
    }
    return result;
  };
  const type = await resolveClass(row.className);
  if (!type) return null;
  if (!row.methodName) return { path: type.location.path, selection: type.location.range.start };
  const candidates = mappingCandidates(row, await searchEndpoints(root));
  if (candidates.length !== 1) return null;
  const target = candidates[0]!;
  if (target.qualifiedController && target.declaringClass) {
    const [controller, declaring] = await Promise.all([
      resolveClass(target.qualifiedController),
      resolveClass(target.declaringClass),
    ]);
    if (!controller || !declaring || target.path !== declaring.location.path) return null;
    const sameLocation = (symbol: WorkspaceSymbol) =>
      symbol.location.path === type.location.path &&
      symbol.location.range.start.line === type.location.range.start.line &&
      symbol.location.range.start.col === type.location.range.start.col;
    if (
      !(sameClassName(row.className, target.qualifiedController) && sameLocation(controller)) &&
      !(sameClassName(row.className, target.declaringClass) && sameLocation(declaring))
    )
      return null;
  } else if (target.path !== type.location.path) {
    return null;
  }
  return { path: target.path, selection: { line: target.line, col: target.col } };
}
