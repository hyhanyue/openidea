import { call, commands, nextRequestId } from "@/platform/ipc";
import { isSameOrInside } from "@/platform/paths";
import { searchEndpoints } from "@/workbench/contributions";
import type { WorkspaceSymbol } from "@/bindings";
import { mappingCandidates, type RuntimeRow, type SourceTarget } from "./model";

export function exactClass(symbols: WorkspaceSymbol[], className: string, root: string): WorkspaceSymbol | null {
  const matches = symbols.filter(
    (s) =>
      ["class", "interface", "enum"].includes(s.kind) &&
      s.location.path.endsWith(".java") &&
      isSameOrInside(s.location.path, root) &&
      (s.name === className || `${s.container}.${s.name}` === className),
  );
  const unique = new Map(
    matches.map((s) => [`${s.location.path}:${s.location.range.start.line}:${s.location.range.start.col}`, s]),
  );
  return unique.size === 1 ? [...unique.values()][0]! : null;
}

export async function sourceTarget(row: RuntimeRow, root: string): Promise<SourceTarget | null> {
  if (!row.className || row.className.includes("$$")) return null;
  const symbols = await call(commands.lspWorkspaceSymbols(nextRequestId(), row.className.split(".").at(-1)!));
  const type = exactClass(symbols, row.className, root);
  if (!type) return null;
  if (!row.methodName) return { path: type.location.path, selection: type.location.range.start };
  const candidates = mappingCandidates(row, await searchEndpoints(root));
  if (candidates.length !== 1 || candidates[0]!.path !== type.location.path) return null;
  const target = candidates[0]!;
  return { path: target.path, selection: { line: target.line, col: target.col } };
}
