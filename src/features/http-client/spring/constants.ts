import { type ClassDecl, type Expr, type SourceFile } from "./source";

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

interface ConstantEntry {
  expression: Expr;
  scope: string;
}
const indexes = new WeakMap<
  SourceFile,
  { fields: Map<string, ConstantEntry>; scopes: WeakMap<ClassDecl, string>; kotlin: boolean }
>();

/** Evaluates string-valued annotation arguments within one file. */
export class Constants {
  private readonly fields = new Map<string, ConstantEntry>();
  private readonly scope: string;
  private readonly kotlin: boolean;
  constructor(file: SourceFile, owner?: ClassDecl) {
    const prior = indexes.get(file);
    if (prior) {
      this.fields = prior.fields;
      this.scope = owner ? (prior.scopes.get(owner) ?? "") : "";
      this.kotlin = prior.kotlin;
      return;
    }
    const scopes = new WeakMap<ClassDecl, string>();
    this.kotlin =
      file.fields.some((f) => f.isConstant) || file.classes.some((c) => c.classes.some((n) => n.name === "Companion"));
    const add = (fields: SourceFile["fields"], scope: string) => {
      for (const f of fields)
        if (f.init && f.isConstant) {
          const key = [scope, f.name].filter(Boolean).join(".");
          this.fields.set(key, { expression: f.init, scope });
        }
    };
    const walk = (classes: ClassDecl[], scope: string) => {
      for (const c of classes) {
        const name = [scope, c.name === "Companion" ? "" : c.name].filter(Boolean).join(".");
        scopes.set(c, name);
        add(c.fields, name);
        walk(c.classes, name);
      }
    };
    add(file.fields, "");
    walk(file.classes, "");
    this.scope = owner ? (scopes.get(owner) ?? "") : "";
    // Fully-qualified same-file constants and explicit static imports resolve exactly.
    for (const [key, entry] of [...this.fields])
      if (file.packageName) this.fields.set(`${file.packageName}.${key}`, entry);
    for (const imported of file.imports) {
      const entry = this.fields.get(imported);
      if (entry) this.fields.set(imported.slice(imported.lastIndexOf(".") + 1), entry);
    }
    indexes.set(file, { fields: this.fields, scopes, kotlin: this.kotlin });
  }

  private field(name: string, scope: string) {
    for (;;) {
      const field = this.fields.get([scope, name].filter(Boolean).join("."));
      if (field) return field;
      if (!scope) return undefined;
      scope = scope.includes(".") ? scope.slice(0, scope.lastIndexOf(".")) : "";
    }
  }

  /** Every element of an array (`{a, b}`, `[a, b]`, `arrayOf(a, b)`) or the single value; unknown expressions return no values. */
  strings(expr: Expr | undefined, enums = false): string[] {
    return this.resolvedStrings(expr, enums) ?? [];
  }

  /** null means at least one element is unknown; an empty array is resolved. */
  resolvedStrings(expr: Expr | undefined, enums = false): string[] | null {
    if (!expr?.length) return [];
    const first = expr[0]!;
    let inner: Expr | null = null;
    if (first.kind === "p" && (first.text === "{" || first.text === "[")) inner = expr.slice(1, -1);
    else if (first.kind === "id" && /^(arrayOf|listOf|setOf)$/.test(first.text) && expr[1]?.text === "(")
      inner = expr.slice(2, -1);
    if (!inner) {
      const v = this.value(expr, enums, new Set(), this.scope);
      return v === null ? null : [v];
    }
    const values = splitTop(inner).map((e) => this.value(e, enums, new Set(), this.scope));
    return values.some((v) => v === null) ? null : (values as string[]);
  }

  string(expr: Expr | undefined, enums = false): string | null {
    return this.strings(expr, enums)[0] ?? null;
  }

  private value(expr: Expr, enums: boolean, seen: Set<string>, scope: string): string | null {
    let out = "";
    for (const part of splitPlus(expr)) {
      const v = this.part(part, enums, seen, scope);
      if (v === null) return null;
      out += v;
    }
    return out;
  }

  private part(part: Expr, enums: boolean, seen: Set<string>, scope: string): string | null {
    // Kotlin templates of constants; other `${…}` are property placeholders, kept.
    if (part.length === 1 && part[0]!.kind === "str")
      return !this.kotlin
        ? (part[0]!.value ?? "")
        : (part[0]!.value ?? "").replace(
            /\$\{(\w+)\}|\$([A-Za-z_]\w*)/g,
            (m, a: string | undefined, b: string | undefined) => {
              const name = (a ?? b)!;
              const init = this.field(name, scope),
                key = `${init?.scope}.${name}`;
              return init && !seen.has(key) && seen.size < 64
                ? (this.value(init.expression, false, new Set([...seen, key]), init.scope) ?? m)
                : m;
            },
          );
    if (part.length === 1 && part[0]!.kind === "num") return part[0]!.text;
    if (part[0]?.text === "(" && part[part.length - 1]?.text === ")")
      return this.value(part.slice(1, -1), enums, seen, scope);
    // `NAME`, `Paths.NAME`, `MediaType.APPLICATION_JSON_VALUE`, `RequestMethod.GET`.
    if (!part.every((t, i) => (i % 2 ? t.kind === "p" && t.text === "." : t.kind === "id"))) return null;
    const names = part.filter((t) => t.kind === "id").map((t) => t.text);
    const last = names[names.length - 1]!;
    const qualifier = names[names.length - 2];
    if (qualifier === "MediaType") return MEDIA_TYPES[last.replace(/_VALUE$/, "")] ?? null;
    const init = this.field(names.join("."), scope);
    if (init) {
      const key = `${init.scope}.${last}`;
      return seen.has(key) || seen.size >= 64
        ? null
        : this.value(init.expression, enums, new Set([...seen, key]), init.scope);
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
