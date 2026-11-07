import { allClasses, type Expr, type SourceFile } from "./source";

const MEDIA_TYPES: Record<string, string> = {
  ALL: "*/*",
  APPLICATION_JSON: "application/json",
  APPLICATION_JSON_UTF8: "application/json;charset=UTF-8",
  APPLICATION_XML: "application/xml",
  APPLICATION_ATOM_XML: "application/atom+xml",
  APPLICATION_FORM_URLENCODED: "application/x-www-form-urlencoded",
  APPLICATION_OCTET_STREAM: "application/octet-stream",
  APPLICATION_PDF: "application/pdf",
  APPLICATION_PROBLEM_JSON: "application/problem+json",
  APPLICATION_PROBLEM_XML: "application/problem+xml",
  APPLICATION_NDJSON: "application/x-ndjson",
  APPLICATION_STREAM_JSON: "application/stream+json",
  APPLICATION_CBOR: "application/cbor",
  APPLICATION_GRAPHQL_RESPONSE: "application/graphql-response+json",
  IMAGE_GIF: "image/gif",
  IMAGE_JPEG: "image/jpeg",
  IMAGE_PNG: "image/png",
  MULTIPART_FORM_DATA: "multipart/form-data",
  MULTIPART_MIXED: "multipart/mixed",
  TEXT_EVENT_STREAM: "text/event-stream",
  TEXT_HTML: "text/html",
  TEXT_MARKDOWN: "text/markdown",
  TEXT_PLAIN: "text/plain",
  TEXT_XML: "text/xml",
};

/** Evaluates string-valued annotation arguments within one file. */
export class Constants {
  private readonly fields = new Map<string, Expr>();
  constructor(file: SourceFile) {
    for (const f of [...file.fields, ...allClasses(file).flatMap((c) => c.fields)])
      if (f.init && !this.fields.has(f.name)) this.fields.set(f.name, f.init);
  }

  /** Every element of an array (`{a, b}`, `[a, b]`, `arrayOf(a, b)`) or the single value; unresolvable ones dropped. */
  strings(expr: Expr | undefined, enums = false): string[] {
    if (!expr?.length) return [];
    const first = expr[0]!;
    let inner: Expr | null = null;
    if (first.kind === "p" && (first.text === "{" || first.text === "[")) inner = expr.slice(1, -1);
    else if (first.kind === "id" && /^(arrayOf|listOf|setOf)$/.test(first.text) && expr[1]?.text === "(")
      inner = expr.slice(2, -1);
    if (!inner) {
      const v = this.value(expr, enums, new Set());
      return v === null ? [] : [v];
    }
    return splitTop(inner)
      .map((e) => this.value(e, enums, new Set()))
      .filter((v): v is string => v !== null);
  }

  string(expr: Expr | undefined, enums = false): string | null {
    return this.strings(expr, enums)[0] ?? null;
  }

  private value(expr: Expr, enums: boolean, seen: Set<string>): string | null {
    let out = "";
    for (const part of splitPlus(expr)) {
      const v = this.part(part, enums, seen);
      if (v === null) return null;
      out += v;
    }
    return out;
  }

  private part(part: Expr, enums: boolean, seen: Set<string>): string | null {
    // Kotlin templates of constants; other `${…}` are property placeholders, kept.
    if (part.length === 1 && part[0]!.kind === "str")
      return (part[0]!.value ?? "").replace(
        /\$\{(\w+)\}|\$([A-Za-z_]\w*)/g,
        (m, a: string | undefined, b: string | undefined) => {
          const name = (a ?? b)!;
          const init = seen.has(name) ? undefined : this.fields.get(name);
          return init ? (this.value(init, false, new Set([...seen, name])) ?? m) : m;
        },
      );
    if (part.length === 1 && part[0]!.kind === "num") return part[0]!.text;
    if (part[0]?.text === "(" && part[part.length - 1]?.text === ")") return this.value(part.slice(1, -1), enums, seen);
    // `NAME`, `Paths.NAME`, `MediaType.APPLICATION_JSON_VALUE`, `RequestMethod.GET`.
    if (!part.every((t, i) => (i % 2 ? t.kind === "p" && t.text === "." : t.kind === "id"))) return null;
    const names = part.filter((t) => t.kind === "id").map((t) => t.text);
    const last = names[names.length - 1]!;
    const qualifier = names[names.length - 2];
    if (qualifier === "MediaType") return MEDIA_TYPES[last.replace(/_VALUE$/, "")] ?? null;
    const init = this.fields.get(last);
    if (init && !seen.has(last)) {
      seen.add(last);
      return this.value(init, enums, seen);
    }
    return enums ? last : null;
  }
}

function splitTop(expr: Expr): Expr[] {
  const parts: Expr[] = [];
  let cur: Expr = [];
  let depth = 0;
  for (const t of expr) {
    if (t.kind === "p" && "([{".includes(t.text)) depth++;
    if (t.kind === "p" && ")]}".includes(t.text)) depth--;
    if (depth === 0 && t.kind === "p" && t.text === ",") {
      parts.push(cur);
      cur = [];
    } else cur.push(t);
  }
  if (cur.length) parts.push(cur);
  return parts;
}

function splitPlus(expr: Expr): Expr[] {
  const parts: Expr[] = [];
  let cur: Expr = [];
  let depth = 0;
  for (const t of expr) {
    if (t.kind === "p" && "([{".includes(t.text)) depth++;
    if (t.kind === "p" && ")]}".includes(t.text)) depth--;
    if (depth === 0 && t.kind === "p" && t.text === "+") {
      parts.push(cur);
      cur = [];
    } else cur.push(t);
  }
  parts.push(cur);
  return parts;
}

