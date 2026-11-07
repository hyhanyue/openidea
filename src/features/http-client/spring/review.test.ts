import { describe, expect, it } from "vitest";
import { springControllers } from "./endpoints";

describe("reviewed unresolved and alias boundaries", () => {
  it("never drops unknown array members or falls back past unresolved class mappings", () => {
    for (const text of [
      '@RestController @RequestMapping(UNKNOWN) class C { @GetMapping("/x") void x(){} }',
      '@RestController class C { @GetMapping({"/x", UNKNOWN}) void x(){} }',
      '@RestController class C { @GetMapping("${route}") void x(){} }',
      '@RestController class C { @GetMapping("#{route}") void x(){} }',
    ])
      expect(
        springControllers(text).flatMap((c) => c.endpoints),
        text,
      ).toEqual([]);
  });
  it("rejects unequal defaults in explicit and implicit alias sets", () => {
    const base =
      "@RequestMapping(method=RequestMethod.GET) @interface Read { DECL } @RestController class C { @Read void x(){} }";
    for (const declaration of [
      '@AliasFor("path") String[] value() default {"/a"}; @AliasFor("value") String[] path() default {"/b"};',
      '@AliasFor(annotation=RequestMapping.class, attribute="path") String[] value() default {"/a"}; @AliasFor(annotation=RequestMapping.class, attribute="path") String[] path() default {"/b"};',
    ])
      expect(springControllers(base.replace("DECL", declaration))[0]?.endpoints).toEqual([]);
  });
  it("supports single sided local aliases since Spring 5.2.1", () => {
    const source =
      '@RequestMapping(method=RequestMethod.GET) @interface Read { @AliasFor("path") String[] value() default {}; String[] path() default {}; } @RestController class C { @Read("/single") void x(){} }';
    expect(springControllers(source)[0]?.endpoints[0]?.paths).toEqual(["/single"]);
  });
  it("does not inherit a known route past a dynamic override", () => {
    const source =
      'interface Api { @GetMapping("/base") String get(); } @RestController class C implements Api { @GetMapping(UNKNOWN) public String get(){} }';
    expect(springControllers(source)[0]?.endpoints).toEqual([]);
  });
  it("does not apply imported non Spring parameter annotations", () => {
    expect(
      springControllers(
        'import other.RequestParam; @RestController class C { @GetMapping("/x") void x(@RequestParam("bad") String q){} }',
      )[0]?.endpoints[0]?.query,
    ).toEqual([]);
  });
  it("resolves constants in their lexical scope and leaves dynamic or qualified foreign values unknown", () => {
    const text =
      'class A { static final String PATH="/a"; } @RestController class C { static final String PATH="/c"; @GetMapping(PATH) void own(){} @GetMapping(A.PATH) void qualified(){} @GetMapping(Missing.PATH) void missing(){} }';
    expect(springControllers(text)[0]?.endpoints.map((e) => e.paths)).toEqual([["/c"], ["/a"]]);
    expect(
      springControllers('@RestController class C { static String PATH="/dynamic"; @GetMapping(PATH) void x(){} }')[0]
        ?.endpoints,
    ).toEqual([]);
  });
  it("rejects aliases to absent meta annotations or missing attributes", () => {
    for (const alias of [
      '@AliasFor(annotation=Other.class, attribute="path")',
      '@AliasFor(annotation=RequestMapping.class, attribute="missing")',
    ]) {
      const text = `@RequestMapping("/fixed") @interface Read { ${alias} String[] route() default {}; } @RestController class C { @Read(route="/x") void x(){} }`;
      expect(springControllers(text)[0]?.endpoints).toEqual([]);
    }
  });
  it("does not guess that external wildcard annotations are Spring", () => {
    expect(springControllers('import foreign.*; @RestController class C { @GetMapping("/x") void x(){} }')).toEqual([]);
  });
  it("reuses constants across concatenation branches without confusing them with a cycle", () => {
    const source =
      'class A { static final String BASE="/a"; } class B { static final String BASE="/b"; } @RestController class C { static final String BASE="/c"; @GetMapping(BASE+BASE) void repeated(){} @GetMapping(A.BASE+B.BASE) void qualified(){} }';
    expect(springControllers(source)[0]?.endpoints.map((e) => e.paths)).toEqual([["/c/c"], ["/a/b"]]);
    expect(
      springControllers(
        "@RestController class C { static final String A=B; static final String B=A; @GetMapping(A) void cycle(){} }",
      )[0]?.endpoints,
    ).toEqual([]);
  });
  it("treats an explicitly written alias default as the default, not a conflict", () => {
    const source =
      '@RequestMapping(method=RequestMethod.GET) @interface Read { @AliasFor(annotation=RequestMapping.class,attribute="path") String[] value() default {}; @AliasFor(annotation=RequestMapping.class,attribute="path") String[] path() default {}; } @RestController class C { @Read(value="/x",path={}) void read(){} }';
    expect(springControllers(source)[0]?.endpoints[0]?.paths).toEqual(["/x"]);
  });
});
