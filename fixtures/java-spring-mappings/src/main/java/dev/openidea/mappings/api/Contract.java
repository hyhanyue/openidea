package dev.openidea.mappings.api;
import dev.openidea.mappings.routes.Layer;
import org.springframework.web.bind.annotation.*;
@RequestMapping({"/cross", "/v2"})
public interface Contract<T> {
    @Layer String save(@RequestBody T body);
}
