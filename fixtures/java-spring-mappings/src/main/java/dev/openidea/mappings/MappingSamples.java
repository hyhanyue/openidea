package dev.openidea.mappings;

import java.lang.annotation.*;
import org.springframework.core.annotation.AliasFor;
import org.springframework.web.bind.annotation.*;

public class MappingSamples {
    @Target({ElementType.METHOD, ElementType.ANNOTATION_TYPE})
    @Retention(RetentionPolicy.RUNTIME)
    @RequestMapping(method = RequestMethod.GET, produces = "application/json")
    public @interface JsonRead {
        @AliasFor(annotation = RequestMapping.class, attribute = "path")
        String[] value() default {};
    }

    @Target(ElementType.METHOD)
    @Retention(RetentionPolicy.RUNTIME)
    @JsonRead
    public @interface PublicRead {
        @AliasFor(annotation = JsonRead.class, attribute = "value")
        String[] route() default {};
    }

    @Target(ElementType.METHOD)
    @Retention(RetentionPolicy.RUNTIME)
    @RequestMapping(method = RequestMethod.GET)
    public @interface AliasRead {
        @AliasFor(annotation = RequestMapping.class, attribute = "path") String[] value() default {};
        @AliasFor(annotation = RequestMapping.class, attribute = "path") String[] path() default {};
    }

    @Target(ElementType.METHOD)
    @Retention(RetentionPolicy.RUNTIME)
    @RequestMapping(path = "/fixed", method = RequestMethod.GET)
    public @interface FixedRead {
        @AliasFor(annotation = RequestMapping.class, attribute = "path") String[] value() default {};
    }

    @Target(ElementType.METHOD)
    @Retention(RetentionPolicy.RUNTIME)
    @RequestMapping(path = "/fixed", method = RequestMethod.GET)
    public @interface StaticRead {}

    @Target(ElementType.METHOD)
    @Retention(RetentionPolicy.RUNTIME)
    @RequestMapping(path = "/meta", method = RequestMethod.GET)
    public @interface DefaultRead {
        @AliasFor(annotation = RequestMapping.class, attribute = "path") String[] value() default {"/default"};
    }

    @Target(ElementType.TYPE)
    @Retention(RetentionPolicy.RUNTIME)
    @RestController
    @RequestMapping("/api")
    public @interface Api {}

    @Api
    public static class Catalog {
        @JsonRead({"/items", "/products"})
        public java.lang.String list(@RequestParam("q") String query) { return query; }
        @PublicRead(route = "/public") public String published() { return "public"; }
        @AliasRead(value = "/alias") public String alias() { return "alias"; }
        @FixedRead public String fixed() { return "fixed"; }
        @StaticRead public String staticFixed() { return "static"; }
        @DefaultRead public String defaultPath() { return "default"; }
    }

    @RequestMapping("/contract")
    public interface Contract<T> {
        @GetMapping("/search") String search(@RequestParam("q") T query);
    }
    public interface Middle<T> extends Contract<T> {}

    @RestController
    public static class ContractController implements Middle<String> {
        public String search(String query) { return query; }
        public String search(Integer unrelated) { return unrelated.toString(); }
    }

    @RequestMapping("/base")
    public static abstract class Base {
        @PostMapping("/send") public String send(@RequestBody String text) { return text; }
    }
    @RestController public static class Child extends Base {}

    @RestController
    @RequestMapping(path = "/verbs", method = RequestMethod.GET)
    public static class Verbs {
        @RequestMapping(path = "/union", method = {RequestMethod.POST, RequestMethod.PUT})
        public String union() { return "ok"; }
    }

    @RestController public static class AnyMethod {
        @RequestMapping("/any") public String any() { return "ok"; }
    }

    @Target({ElementType.METHOD, ElementType.ANNOTATION_TYPE}) @Retention(RetentionPolicy.RUNTIME)
    @RequestMapping(method = RequestMethod.GET)
    public @interface SingleAlias {
        @AliasFor("path") String[] value() default {};
        String[] path() default {};
    }
    @RestController public static class SingleAliasController {
        @SingleAlias("/single") public String single() { return "single"; }
    }
    @Target(ElementType.METHOD) @Retention(RetentionPolicy.RUNTIME)
    @RequestMapping(method = RequestMethod.GET)
    public @interface BadDefaults {
        @AliasFor("path") String[] value() default {"/a"};
        @AliasFor("value") String[] path() default {"/b"};
    }
    @RestController public static class InvalidController {
        @BadDefaults public String invalid() { return "invalid"; }
    }
    @RestController public static class ConflictingController {
        @AliasRead(value = "/a", path = "/b") public String conflict() { return "conflict"; }
    }

    @Target(ElementType.METHOD) @Retention(RetentionPolicy.RUNTIME)
    @JsonRead
    public @interface TransitiveRead {
        @AliasFor(annotation = JsonRead.class, attribute = "value") String[] first() default {};
        @AliasFor(annotation = RequestMapping.class, attribute = "path") String[] second() default {};
    }
    @RestController public static class TransitiveController {
        @TransitiveRead(second = "/transitive") public String transitive() { return "transitive"; }
    }
    @RestController public static class TransitiveConflictController {
        @TransitiveRead(first = "/a", second = "/b") public String conflict() { return "conflict"; }
    }
}
