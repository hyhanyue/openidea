package dev.openidea.mappings.web;
import dev.openidea.mappings.api.Middle;
import dev.openidea.mappings.model.Order;
import dev.openidea.mappings.routes.Api;
@Api public class CrossController implements Middle<Order> {
    public String save(Order body) { return body.name; }
    public String save(Integer unrelated) { return unrelated.toString(); }
}
