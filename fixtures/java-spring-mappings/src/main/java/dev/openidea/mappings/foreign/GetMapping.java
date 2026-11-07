package dev.openidea.mappings.foreign;
import java.lang.annotation.*;
@Target(ElementType.METHOD) @Retention(RetentionPolicy.RUNTIME)
public @interface GetMapping { String value(); }
