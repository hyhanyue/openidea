package dev.openidea.mappings.api;
import org.springframework.web.bind.annotation.*;
@RequestMapping("/parent")
public abstract class GenericBase<T> {
    @PostMapping("/send") public String send(@RequestBody T body) { return "inherited"; }
}
