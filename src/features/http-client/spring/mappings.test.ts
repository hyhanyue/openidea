import { describe, expect, it } from "vitest";
import { projectControllers, springControllers } from "./endpoints";
import { MappingProject } from "./mappings";
import { parseSource } from "./source";

const composed = `
import org.springframework.web.bind.annotation.*;
import org.springframework.core.annotation.AliasFor;
@RequestMapping(method = RequestMethod.GET, produces = "application/json")
@interface JsonRead {
  @AliasFor(annotation = RequestMapping.class, attribute = "path")
  String[] value() default {};
}
@RestController @RequestMapping("/api")
class Catalog {
  @JsonRead({"/items", "/products"})
  java.lang.String list(@RequestParam("q") String query) { return ""; }
}`;

function project(files: Record<string, string>) {
  return new MappingProject(Object.entries(files).map(([path, text]) => ({ path, text, file: parseSource(text) })));
}

describe("Spring composed mappings and controller contracts", () => {
  it("merges composed annotations and explicit AliasFor overrides", () => {
    expect(springControllers(composed)[0]?.endpoints).toMatchObject([
      { methodName: "list", paths: ["/api/items", "/api/products"], httpMethods: ["GET"], produces: "application/json", query: [{ name: "q", value: null }] },
    ]);
  });

  it("recognizes composed controller stereotypes and nested aliases", () => {
    const text = composed.replace('@RestController @RequestMapping("/api")\nclass Catalog', `
@JsonRead @interface PublicRead {
  @AliasFor(annotation = JsonRead.class, attribute = "value") String[] route() default {};
}
@RestController @RequestMapping("/api") @interface Api {}
@Api class Catalog`).replace('@JsonRead({"/items", "/products"})', '@PublicRead(route="/items")');
    expect(springControllers(text).map((c) => [c.name, c.endpoints.map((e) => e.paths)])).toEqual([["Catalog", [["/api/items"]]]]);
  });

  it("expands local aliases and rejects conflicting alias values", () => {
    const text = composed.replace('@AliasFor(annotation = RequestMapping.class, attribute = "path")\n  String[] value() default {};', `@AliasFor("value") String[] path() default {};
  @AliasFor("path") String[] value() default {};`).replace('@JsonRead({"/items", "/products"})', '@JsonRead(value="/items")');
    expect(springControllers(text)[0]?.endpoints[0]?.paths).toEqual(["/api/items"]);
    expect(springControllers(text.replace('value="/items"', 'value="/items", path="/other"'))[0]?.endpoints).toEqual([]);
    expect(springControllers(text.replace('@AliasFor("value")', '@AliasFor(annotation=RequestMapping.class,attribute="path")'))[0]?.endpoints).toEqual([]);
  });

  it("applies alias defaults, including an empty path overriding a fixed meta-path", () => {
    const text = composed.replace('method = RequestMethod.GET,', 'path = "/fixed", method = RequestMethod.GET,')
      .replace('@JsonRead({"/items", "/products"})', '@JsonRead');
    expect(springControllers(text)[0]?.endpoints[0]?.paths).toEqual(["/api"]);
    expect(springControllers(text.replace('String[] value() default {};', ''))[0]?.endpoints[0]?.paths).toEqual(["/api/fixed"]);
    expect(springControllers(text.replace('String[] value() default {};', 'String[] value() default {"/default"};'))[0]?.endpoints[0]?.paths).toEqual(["/api/default"]);
  });

  it("does not invent a root route for unresolved paths or non-Spring annotations", () => {
    expect(springControllers('@RestController class C { @GetMapping(UNKNOWN) String list() {} }')[0]?.endpoints).toEqual([]);
    expect(springControllers('import other.GetMapping;\n@RestController class C { @GetMapping("/x") String list() {} }')[0]?.endpoints).toEqual([]);
    expect(springControllers('@other.RestController class C { @GetMapping("/x") String list() {} }')).toEqual([]);
    expect(springControllers('@RequestMapping @interface A {}\n@A @interface B {}\n@B @interface A2 {}')).toEqual([]);
  });

  it("retains every declared HTTP method and Spring's class/method union", () => {
    const [c] = springControllers('@RestController @RequestMapping(method=RequestMethod.GET) class C { @RequestMapping(path="/x",method={RequestMethod.POST, RequestMethod.PUT}) void list(){} @RequestMapping("/all") void all(){} }');
    expect(c?.endpoints.map((e) => e.httpMethods)).toEqual([["GET", "POST", "PUT"], ["GET"]]);
    expect(springControllers('@RestController class C { @RequestMapping("/all") void all(){} }')[0]?.endpoints[0]?.httpMethods).toEqual([]);
  });

  it("resolves inherited generic contracts, parameter annotations and overloads across files", () => {
    const sources = {
      "/p/Api.java": `package example; import org.springframework.web.bind.annotation.*;
        @RequestMapping("/api") interface Api<T> { @GetMapping("/items") String list(@RequestParam("q") T query); }`,
      "/p/Middle.java": "package example; interface Middle<T> extends Api<T> {}",
      "/p/Catalog.java": `package example; import org.springframework.web.bind.annotation.RestController;
        @RestController class Catalog implements Middle<String> {
          public String list(String q) { return q; }
          public String list(Integer other) { return ""; }
        }`,
    };
    const [controller] = projectControllers(project(sources), "/p/Catalog.java");
    expect(controller?.endpoints).toMatchObject([{ methodName: "list", paths: ["/api/items"], query: [{ name: "q", value: null }] }]);
    expect(controller?.endpoints).toHaveLength(1);
    expect(sources["/p/Catalog.java"].slice(controller!.endpoints[0]!.offset)).toMatch(/^list\(String/);
  });

  it("provides parent definition coordinates for inherited methods without fake caret targets", () => {
    const p = project({
      "/p/Base.java": '@RequestMapping("/base") abstract class Base { @PostMapping("/send") public void send(@RequestBody String text) {} }',
      "/p/Child.java": "@RestController class Child extends Base {}",
    });
    const [controller] = projectControllers(p, "/p/Child.java");
    expect(controller?.endpoints[0]).toMatchObject({ paths: ["/base/send"], bodyType: "String", range: { from: -1, to: -1 }, origin: { path: "/p/Base.java" } });
    expect(projectControllers(p, "/p/Base.java")).toEqual([]);
  });

  it("resolves imports rather than unrelated same-named contracts and annotations", () => {
    const p = project({
      "/p/a/Route.java": 'package a; @RequestMapping(method=RequestMethod.POST) @interface Route { @AliasFor(annotation=RequestMapping.class,attribute="path") String[] value() default {}; }',
      "/p/b/Route.java": 'package b; @RequestMapping(method=RequestMethod.DELETE) @interface Route { @AliasFor(annotation=RequestMapping.class,attribute="path") String[] value() default {}; }',
      "/p/a/Api.java": 'package a; interface Api { @GetMapping("/a") String get(); }',
      "/p/b/Api.java": 'package b; interface Api { @GetMapping("/b") String get(); }',
      "/p/Controller.java": 'package c; import a.Api; import b.Route; @RestController class C implements Api { public String get(){} @Route("/route") String post(){} }',
    });
    expect(projectControllers(p, "/p/Controller.java")[0]?.endpoints.map((e) => [e.httpMethod, e.paths])).toEqual([["GET", ["/a"]], ["DELETE", ["/route"]]]);
  });

  it("updates an unsaved controller against the same project and handles cycles", () => {
    const p = project({
      "/p/Api.java": 'interface Api { @GetMapping("/old") String get(); }',
      "/p/C.java": "@RestController class C implements Api { public String get(){} }",
      "/p/Bad.java": "interface A extends B {} interface B extends A {}",
    });
    expect(projectControllers(p, "/p/C.java", '@RestController class C implements Api { @PostMapping("/new") public String get(){} }')[0]?.endpoints[0]).toMatchObject({ httpMethod: "POST", paths: ["/new"] });
    expect(projectControllers(p, "/p/C.java")[0]?.endpoints[0]?.paths).toEqual(["/old"]);
  });
});
