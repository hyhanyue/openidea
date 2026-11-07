import { Constants } from "./constants";
import {
  parseSource,
  type Annotation,
  type ClassDecl,
  type Expr,
  type Method,
  type Param,
  type SourceFile,
} from "./source";

export interface MappingSource {
  path: string;
  text: string;
  file: SourceFile;
}

export interface Declaration {
  source: MappingSource;
  cls: ClassDecl;
  name: string;
  constants: Constants;
}

const WEB = "org.springframework.web.bind.annotation.";
const STEREOTYPE = "org.springframework.stereotype.";
const SHORTCUTS: Record<string, string> = {
  GetMapping: "GET",
  PostMapping: "POST",
  PutMapping: "PUT",
  DeleteMapping: "DELETE",
  PatchMapping: "PATCH",
};
const ATTRIBUTES = new Set(["value", "path", "method", "params", "headers", "consumes", "produces"]);
const JAVA_LANG = new Set([
  "String",
  "Object",
  "Boolean",
  "Byte",
  "Short",
  "Integer",
  "Long",
  "Float",
  "Double",
  "Character",
  "Number",
  "Void",
]);

export interface Mapping {
  paths: string[];
  methods: string[];
  params: string[];
  headers: string[];
  consumes: string[];
  produces: string[];
}

const UNRESOLVED = Symbol("unresolved mapping");
type MappingResult = Mapping | null | typeof UNRESOLVED;

interface Value {
  expression: Expr;
  owner: Declaration;
  explicit: boolean;
  priority?: number;
}
type Values = Map<string, Value>;
type Overrides = Map<string, Values>;
interface Parent {
  owner: Declaration;
  variables: Map<string, string>;
}
export interface MappedMethod {
  method: Method;
  owner: Declaration;
  mapping: Mapping;
  params: (Param & { owner: Declaration; annotationOwners: Map<Annotation, Declaration> })[];
}
export interface MappedController {
  owner: Declaration;
  mapping: Mapping | null;
  methods: MappedMethod[];
}

function declarations(
  source: MappingSource,
  classes = source.file.classes,
  prefix = source.file.packageName,
): Declaration[] {
  return classes.flatMap((cls) => {
    const name = [prefix, cls.name].filter(Boolean).join(".");
    return [
      { source, cls, name, constants: new Constants(source.file, cls) },
      ...declarations(source, cls.classes, name),
    ];
  });
}

function raw(type: string): string {
  return type.replace(/<.*>/g, "").replace(/\[\]|\.\.\./g, "");
}
function substitute(type: string, variables: Map<string, string>): string {
  return type.replace(/\b[A-Za-z_$][\w$]*\b/g, (n) => variables.get(n) ?? n);
}
function argumentsOf(type: string): string[] {
  const inner = type.slice(type.indexOf("<") + 1, type.lastIndexOf(">"));
  if (!type.includes("<")) return [];
  const out: string[] = [];
  let depth = 0;
  let start = 0;
  for (let i = 0; i < inner.length; i++) {
    if (inner[i] === "<") depth++;
    else if (inner[i] === ">") depth--;
    else if (inner[i] === "," && !depth) {
      out.push(inner.slice(start, i));
      start = i + 1;
    }
  }
  out.push(inner.slice(start));
  return out;
}
function valuesEqual(a: Value, b: Value): boolean {
  const left = a.owner.constants.resolvedStrings(a.expression, true),
    right = b.owner.constants.resolvedStrings(b.expression, true);
  return left !== null && right !== null && JSON.stringify(left) === JSON.stringify(right);
}

/** A source-scoped Spring annotation and Java hierarchy model; never loads classes. */
export class MappingProject {
  private readonly types: Map<string, Declaration[]>;
  private readonly byPath = new Map<string, Declaration[]>();
  constructor(readonly sources: MappingSource[]) {
    this.types = new Map();
    for (const source of sources) {
      const declared = declarations(source);
      this.byPath.set(source.path, declared);
      for (const declaration of declared) {
        const group = this.types.get(declaration.name) ?? [];
        group.push(declaration);
        this.types.set(declaration.name, group);
      }
    }
  }

  private find(name: string, owner: Declaration): Declaration | null {
    const unique = (n: string) => {
      const ds = this.types.get(n);
      return ds?.length === 1 ? ds[0]! : null;
    };
    if (name.includes(".")) {
      const exact = unique(name);
      if (exact) return exact;
    }
    const first = name.split(".")[0]!;
    const imported = owner.source.file.imports.filter((i) => i.endsWith(`.${first}`));
    if (imported.length) return imported.length === 1 ? unique(imported[0]! + name.slice(first.length)) : null;
    let scope = owner.name;
    while (scope) {
      const local = unique(`${scope}.${name}`);
      if (local) return local;
      scope = scope.includes(".") ? scope.slice(0, scope.lastIndexOf(".")) : "";
    }
    const local = unique([owner.source.file.packageName, name].filter(Boolean).join("."));
    if (local) return local;
    const wildcard = owner.source.file.imports
      .filter((i) => i.endsWith(".*"))
      .map((i) => unique(i.slice(0, -1) + name))
      .filter((d): d is Declaration => !!d);
    return wildcard.length === 1 ? wildcard[0]! : null;
  }

  private framework(name: string, qualified: string, owner: Declaration, prefix = WEB): boolean {
    if (qualified.includes(".")) return qualified === prefix + name;
    if (this.find(name, owner)) return false;
    const explicit = owner.source.file.imports.filter((i) => i.endsWith(`.${name}`));
    if (explicit.length) return explicit.length === 1 && explicit[0] === prefix + name;
    if (this.types.has([owner.source.file.packageName, name].filter(Boolean).join("."))) return false;
    const wildcards = owner.source.file.imports.filter((i) => i.endsWith(".*"));
    if (wildcards.some((i) => this.types.has(i.slice(0, -1) + name))) return false;
    return !wildcards.length || wildcards.includes(prefix + "*");
  }

  private key(name: string, owner: Declaration): string {
    return (
      this.find(name, owner)?.name ??
      (name.includes(".") ? name : (owner.source.file.imports.find((i) => i.endsWith(`.${name}`)) ?? WEB + name))
    );
  }

  private metaPresent(owner: Declaration, target: string, seen = new Set<string>()): boolean {
    if (seen.has(owner.name) || seen.size > 32) return false;
    return owner.cls.annotations.some((a) => {
      const key = this.key(a.qualifiedName, owner);
      if (key === target) return true;
      const declaration = this.find(a.qualifiedName, owner);
      return !!declaration && this.metaPresent(declaration, target, new Set([...seen, owner.name]));
    });
  }

  private aliasTarget(owner: Declaration, attribute: string, seen = new Set<string>()): string | null {
    const key = `${owner.name}#${attribute}`;
    if (seen.has(key) || seen.size > 32) return null;
    const method = owner.cls.methods.find((m) => m.name === attribute);
    if (!method) return null;
    const alias = method.annotations.find(
      (a) =>
        a.name === "AliasFor" && this.framework(a.name, a.qualifiedName, owner, "org.springframework.core.annotation."),
    );
    const targetType = alias?.args
      .get("annotation")
      ?.map((t) => t.text)
      .join("")
      .replace(/\.class$/, "");
    const target = targetType ? this.key(targetType, owner) : owner.name;
    const attr = (alias && owner.constants.string(alias.args.get("attribute") ?? alias.args.get("value"))) || attribute;
    if (alias && target !== owner.name) {
      if (target === WEB + "RequestMapping") return `${target}#${attr === "value" ? "path" : attr}`;
      const declaration = this.find(target, owner);
      return declaration ? this.aliasTarget(declaration, attr, new Set([...seen, key])) : null;
    }
    if (ATTRIBUTES.has(attribute) && attribute !== "value") return `${WEB}RequestMapping#${attribute}`;
    return alias ? this.aliasTarget(owner, attr, new Set([...seen, key])) : null;
  }

  private attributes(ann: Annotation, owner: Declaration, incoming: Values | undefined): Values {
    const values: Values = new Map();
    for (const [name, expression] of ann.args) values.set(name, { expression, owner, explicit: true });
    for (const [name, value] of incoming ?? [])
      values.set(name, { ...value, priority: (value.priority ?? (value.explicit ? 1 : 0)) + 2 });
    return values;
  }

  private mapping(
    ann: Annotation,
    owner: Declaration,
    seen = new Set<string>(),
    overrides: Overrides = new Map(),
  ): MappingResult {
    const declaration = this.find(ann.qualifiedName, owner);
    const name = declaration?.name ?? this.key(ann.qualifiedName, owner);
    if (seen.has(name) || seen.size > 32) return null;
    const visited = new Set([...seen, name]);
    if (
      !declaration &&
      (ann.name === "RequestMapping" || SHORTCUTS[ann.name]) &&
      this.framework(ann.name, ann.qualifiedName, owner)
    ) {
      const values = this.attributes(ann, owner, undefined);
      const localIncoming = overrides.get(name);
      if (localIncoming?.has("path") && !localIncoming.has("value")) values.delete("value");
      if (localIncoming?.has("value") && !localIncoming.has("path")) values.delete("path");
      for (const [key, value] of localIncoming ?? []) values.set(key, value);
      const incoming = overrides.get(WEB + "RequestMapping");
      if (incoming?.has("path") && !incoming.has("value")) values.delete("value");
      if (incoming?.has("value") && !incoming.has("path")) values.delete("path");
      for (const [key, value] of incoming ?? []) values.set(key, value);
      const strings = (key: string): string[] => {
        const v = values.get(key);
        return v ? v.owner.constants.strings(v.expression, key === "method") : [];
      };
      const path = strings("path"),
        value = strings("value");
      if (path.length && value.length && JSON.stringify(path) !== JSON.stringify(value)) return UNRESOLVED;
      for (const key of ATTRIBUTES) {
        const v = values.get(key);
        if (v && v.owner.constants.resolvedStrings(v.expression, key === "method") === null) return UNRESOLVED;
      }
      if ([...path, ...value].some((p) => /[$#]\{/.test(p))) return UNRESOLVED;
      if (strings("method").some((m) => !/^(GET|HEAD|POST|PUT|PATCH|DELETE|OPTIONS|TRACE)$/.test(m))) return UNRESOLVED;
      return {
        paths: path.length ? path : value,
        methods: SHORTCUTS[ann.name] ? [SHORTCUTS[ann.name]!] : strings("method"),
        params: strings("params"),
        headers: strings("headers"),
        consumes: strings("consumes"),
        produces: strings("produces"),
      };
    }
    if (declaration?.cls.kind !== "annotation") return null;
    const values: Values = new Map();
    for (const method of declaration.cls.methods)
      if (method.defaultValue)
        values.set(method.name, { expression: method.defaultValue, owner: declaration, explicit: false });
    for (const [key, value] of this.attributes(ann, owner, overrides.get(name))) values.set(key, value);
    for (const method of declaration.cls.methods) {
      const value = values.get(method.name);
      if (
        value &&
        value.priority === undefined &&
        method.defaultValue &&
        valuesEqual(value, { expression: method.defaultValue, owner: declaration, explicit: false })
      ) {
        values.set(method.name, { ...value, explicit: false });
      }
    }
    const forwarded: Overrides = new Map([...overrides].map(([key, value]) => [key, new Map(value)]));
    const aliases = declaration.cls.methods.map((method) => {
      const alias = method.annotations.find(
        (a) =>
          a.name === "AliasFor" &&
          this.framework(a.name, a.qualifiedName, declaration, "org.springframework.core.annotation."),
      );
      const target = alias?.args
        .get("annotation")
        ?.map((t) => t.text)
        .join("")
        .replace(/\.class$/, "");
      const attr = alias && declaration.constants.string(alias.args.get("attribute") ?? alias.args.get("value"));
      return {
        method: method.name,
        target: target ? this.key(target, declaration) : name,
        attr: attr || method.name,
        alias: !!alias,
      };
    });
    if (aliases.some((a) => a.alias && a.target === name && a.attr === a.method)) return UNRESOLVED;
    for (const a of aliases)
      if (a.alias && a.target === name && a.attr !== a.method) {
        const partner = aliases.find((b) => b.method === a.attr);
        if (!partner || (partner.alias && (partner.target !== name || partner.attr !== a.method))) return UNRESOLVED;
      }
    for (const a of aliases.filter((a) => a.alias && a.target !== name)) {
      if (!this.metaPresent(declaration, a.target)) return UNRESOLVED;
      const target = this.find(a.target, declaration);
      if (
        target
          ? !target.cls.methods.some((m) => m.name === a.attr)
          : a.target !== WEB + "RequestMapping" || (!ATTRIBUTES.has(a.attr) && a.attr !== "name")
      )
        return UNRESOLVED;
    }
    // Explicit and transitive implicit aliases form one set at their final target.
    const sets = new Map<string, typeof aliases>();
    for (const a of aliases) {
      const target = this.aliasTarget(declaration, a.method);
      if (!target) continue;
      const group = sets.get(target) ?? [];
      group.push(a);
      sets.set(target, group);
    }
    for (const group of sets.values())
      if (group.length > 1) {
        const first = declaration.cls.methods.find((m) => m.name === group[0]!.method)!;
        const candidates = group.map((a) => values.get(a.method)).filter((v): v is Value => !!v);
        const priority = Math.max(...candidates.map((v) => v.priority ?? (v.explicit ? 1 : 0)));
        const preferred = candidates.filter((v) => (v.priority ?? (v.explicit ? 1 : 0)) === priority);
        if (priority > 0 && preferred.some((v) => !valuesEqual(v, preferred[0]!))) return UNRESOLVED;
        for (const a of group) {
          const method = declaration.cls.methods.find((m) => m.name === a.method)!;
          if (
            first.returnType !== method.returnType ||
            !first.defaultValue ||
            !method.defaultValue ||
            !valuesEqual(
              { expression: first.defaultValue, owner: declaration, explicit: false },
              { expression: method.defaultValue, owner: declaration, explicit: false },
            )
          )
            return UNRESOLVED;
          if (priority > 0 && preferred[0]) values.set(a.method, preferred[0]);
        }
      }
    for (let pass = 0; pass < aliases.length; pass++)
      for (const a of aliases) {
        if (!a.alias || a.target !== name || a.method === a.attr) continue;
        const from = values.get(a.method),
          to = values.get(a.attr);
        if (from?.explicit && to?.explicit && !valuesEqual(from, to)) return UNRESOLVED;
        if (from?.explicit && !to?.explicit) values.set(a.attr, from);
        else if (to?.explicit && !from?.explicit) values.set(a.method, to);
      }
    for (const a of aliases) {
      const value = values.get(a.method);
      if (!value) continue;
      const local = !a.alias || a.target === name;
      const target = local ? WEB + "RequestMapping" : a.target;
      if (local && (!ATTRIBUTES.has(a.method) || a.method === "value")) continue;
      const targetValues = forwarded.get(target) ?? new Map<string, Value>();
      const prior = targetValues.get(a.attr);
      const rank = (v: Value) => v.priority ?? (v.explicit ? 1 : 0);
      if (prior && rank(prior) === rank(value) && !valuesEqual(prior, value)) return UNRESOLVED;
      if (!prior || rank(value) >= rank(prior)) targetValues.set(a.attr, value);
      forwarded.set(target, targetValues);
    }
    return this.firstMapping(declaration.cls.annotations, declaration, visited, forwarded);
  }

  private firstMapping(
    annotations: Annotation[],
    owner: Declaration,
    seen = new Set<string>(),
    overrides: Overrides = new Map(),
  ): MappingResult {
    for (const ann of annotations) {
      const mapping = this.mapping(ann, owner, seen, overrides);
      if (mapping !== null) return mapping;
    }
    return null;
  }

  private parents(parent: Parent): Parent[] {
    return parent.owner.cls.supers
      .flatMap((type) => {
        const owner = this.find(raw(type), parent.owner);
        if (!owner) return [];
        const args = argumentsOf(type).map((a) =>
          substitute(a, parent.variables).replace(
            /\b[A-Za-z_$][\w$]*(?:\.[A-Za-z_$][\w$]*)*/g,
            (n) =>
              this.find(n, parent.owner)?.name ??
              parent.owner.source.file.imports.find((i) => i.endsWith(`.${n}`)) ??
              (JAVA_LANG.has(n) ? `java.lang.${n}` : n),
          ),
        );
        return [{ owner, variables: new Map(owner.cls.typeParams.map((p, i) => [p, args[i] ?? p])) }];
      })
      .sort((a, b) => Number(b.owner.cls.kind === "interface") - Number(a.owner.cls.kind === "interface"));
  }

  private hierarchy(owner: Declaration): Parent[] {
    const out: Parent[] = [];
    const visited = new Set<string>();
    const walk = (parent: Parent) => {
      if (visited.has(parent.owner.name) || visited.size >= 128) return;
      visited.add(parent.owner.name);
      out.push(parent);
      for (const p of this.parents(parent)) walk(p);
    };
    walk({ owner, variables: new Map() });
    return out;
  }

  private controller(annotations: Annotation[], owner: Declaration, seen = new Set<string>()): boolean {
    return annotations.some((a) => {
      if (
        (a.name === "RestController" && this.framework(a.name, a.qualifiedName, owner)) ||
        (a.name === "Controller" && this.framework(a.name, a.qualifiedName, owner, STEREOTYPE))
      )
        return true;
      const declaration = this.find(a.qualifiedName, owner);
      return (
        declaration?.cls.kind === "annotation" &&
        !seen.has(declaration.name) &&
        this.controller(declaration.cls.annotations, declaration, new Set([...seen, declaration.name]))
      );
    });
  }

  private signature(method: Method, parent: Parent): string {
    return (
      method.name +
      "(" +
      method.params
        .map((p) => {
          const type = substitute(p.type, parent.variables),
            name = raw(type);
          const dimensions = (type.match(/\[\]|\.\.\./g) ?? []).length;
          const imported = parent.owner.source.file.imports.find((i) => i.endsWith(`.${name}`));
          const canonical =
            this.find(name, parent.owner)?.name ?? imported ?? (JAVA_LANG.has(name) ? `java.lang.${name}` : name);
          return canonical + "[]".repeat(dimensions);
        })
        .join(",") +
      ")"
    );
  }

  controllers(path: string, text?: string): MappedController[] {
    const source = this.sources.find((s) => s.path === path);
    if (!source)
      return text === undefined
        ? []
        : new MappingProject([...this.sources, { path, text, file: parseSource(text) }]).controllers(path);
    if (text !== undefined && text !== source.text)
      return new MappingProject(
        this.sources.map((s) => (s === source ? { path, text, file: parseSource(text) } : s)),
      ).controllers(path);
    const out: MappedController[] = [];
    for (const owner of this.byPath.get(path) ?? []) {
      if (["interface", "annotation", "enum"].includes(owner.cls.kind) || owner.cls.abstract) continue;
      const hierarchy = this.hierarchy(owner);
      if (!hierarchy.some((p) => this.controller(p.owner.cls.annotations, p.owner))) continue;
      const mapping =
        hierarchy.map((p) => this.firstMapping(p.owner.cls.annotations, p.owner)).find((m) => m !== null) ?? null;
      if (mapping === UNRESOLVED) continue;
      const groups = new Map<string, { parent: Parent; method: Method }[]>();
      for (const parent of hierarchy)
        for (const method of parent.owner.cls.methods) {
          if (method.name === parent.owner.cls.name) continue;
          const signature = this.signature(method, parent),
            group = groups.get(signature) ?? [];
          group.push({ parent, method });
          groups.set(signature, group);
        }
      const methods: MappedMethod[] = [];
      for (const group of groups.values()) {
        const first = group.find((g) => g.parent.owner.cls.kind !== "interface") ?? group[0]!;
        const ordered = [first, ...group.filter((g) => g !== first)];
        const methodMapping = ordered
          .map((g) => this.firstMapping(g.method.annotations, g.parent.owner))
          .find((m) => m !== null);
        if (!methodMapping || methodMapping === UNRESOLVED) continue;
        const params = first.method.params.map((p, i) => {
          const annotations: Annotation[] = [],
            annotationOwners = new Map<Annotation, Declaration>();
          for (const g of ordered)
            for (const a of g.method.params[i]?.annotations ?? [])
              if (
                this.framework(a.name, a.qualifiedName, g.parent.owner) &&
                !annotations.some((prior) => prior.name === a.name)
              ) {
                annotations.push(a);
                annotationOwners.set(a, g.parent.owner);
              }
          return {
            ...p,
            type: substitute(p.type, first.parent.variables),
            annotations,
            owner: first.parent.owner,
            annotationOwners,
          };
        });
        methods.push({ method: first.method, owner: first.parent.owner, mapping: methodMapping, params });
      }
      out.push({ owner, mapping, methods });
    }
    return out;
  }
}
