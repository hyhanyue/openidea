package dev.openidea.mappings.routes;
import java.lang.annotation.*;
import org.springframework.core.annotation.AliasFor;
@Target({ElementType.METHOD, ElementType.ANNOTATION_TYPE}) @Retention(RetentionPolicy.RUNTIME)
@Read("/meta")
public @interface Layer {
    @AliasFor(annotation = Read.class, attribute = "value") String[] routes() default {"/save", "/write"};
}
