import type { EndpointEntry, OpenOptions } from "@/workbench/contributions";

export const endpoints = ["beans", "conditions", "env", "configprops", "mappings"] as const;
export type Endpoint = (typeof endpoints)[number];
export interface RuntimeRow {
  name: string;
  context: string;
  detail: string;
  className?: string;
  methodName?: string;
  descriptor?: string;
  dependencies?: string[];
  methods?: string[];
  paths?: string[];
  outcome?: "matched" | "notMatched" | "unconditional";
  masked?: boolean;
}
export type RuntimeState =
  | "ok"
  | "unauthorized"
  | "forbidden"
  | "disabled"
  | "redirect"
  | "httpError"
  | "timeout"
  | "unreachable"
  | "tooLarge"
  | "invalidResponse"
  | "incompatible"
  | "cancelled";
export interface RuntimeData {
  state: RuntimeState;
  rows: RuntimeRow[];
  limited?: boolean;
  status?: number;
}
export type Snapshot = Record<Endpoint, RuntimeData>;
export interface SourceTarget {
  path: string;
  selection: OpenOptions["selection"];
}

/** The source provider only proves mapping identity when class, method, verb and path agree.
 * A second overload, package candidate or source location makes the result unknown. */
export function mappingCandidates(row: RuntimeRow, routes: EndpointEntry[]): EndpointEntry[] {
  if (!row.className || !row.methodName || !row.methods?.length || !row.paths?.length) return [];
  const simple = row.className.split(".").at(-1);
  const candidates = routes.filter(
    (r) =>
      (r.controller === row.className || r.controller === simple) &&
      r.methodName === row.methodName &&
      row.methods!.includes(r.method) &&
      row.paths!.includes(r.url),
  );
  const unique = new Map(candidates.map((r) => [`${r.path}:${r.line}:${r.col}`, r]));
  return [...unique.values()];
}

export function visibleRows(data: RuntimeData | undefined, query: string): RuntimeRow[] {
  const q = query.trim().toLowerCase();
  return (data?.rows ?? []).filter(
    (r) =>
      !q ||
      [r.name, r.context, r.detail, r.className, r.methodName, r.outcome, ...(r.dependencies ?? [])]
        .join(" ")
        .toLowerCase()
        .includes(q),
  );
}
