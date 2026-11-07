package dev.openidea.mappings.routes;
import java.lang.annotation.*;
import org.springframework.stereotype.Controller;
import org.springframework.web.bind.annotation.ResponseBody;
@Target(ElementType.TYPE) @Retention(RetentionPolicy.RUNTIME)
@Controller @ResponseBody
public @interface Api {}
