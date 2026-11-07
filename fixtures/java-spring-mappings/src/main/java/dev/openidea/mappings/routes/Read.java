package dev.openidea.mappings.routes;
import java.lang.annotation.*;
import org.springframework.core.annotation.AliasFor;
import org.springframework.web.bind.annotation.*;
@Target({ElementType.METHOD, ElementType.ANNOTATION_TYPE}) @Retention(RetentionPolicy.RUNTIME)
@RequestMapping(method = {RequestMethod.POST, RequestMethod.PUT}, consumes = "application/json")
public @interface Read {
    @AliasFor(annotation = RequestMapping.class, attribute = "path") String[] value() default {};
    @AliasFor(annotation = RequestMapping.class, attribute = "path") String[] path() default {};
}
