package dev.openidea.mappings.web;
import dev.openidea.mappings.foreign.GetMapping;
import org.springframework.web.bind.annotation.RestController;
@RestController public class ForeignController {
    @GetMapping("/false") public String unrelated() { return "no"; }
}
