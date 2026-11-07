package dev.openidea.mappings;

import com.fasterxml.jackson.databind.ObjectMapper;
import java.nio.file.*;
import java.util.*;
import java.util.stream.Collectors;
import org.junit.jupiter.api.*;
import org.springframework.context.annotation.*;
import org.springframework.mock.web.MockServletContext;
import org.springframework.web.context.support.GenericWebApplicationContext;
import org.springframework.web.method.HandlerMethod;
import org.springframework.web.servlet.config.annotation.EnableWebMvc;
import org.springframework.web.servlet.mvc.method.RequestMappingInfo;
import org.springframework.web.servlet.mvc.method.annotation.RequestMappingHandlerMapping;
import org.springframework.test.web.servlet.MockMvc;
import org.springframework.test.web.servlet.setup.MockMvcBuilders;
import static org.junit.jupiter.api.Assertions.*;
import static org.springframework.test.web.servlet.request.MockMvcRequestBuilders.*;
import static org.springframework.test.web.servlet.result.MockMvcResultMatchers.*;

class MappingContractsTest {
    private static final List<Map<String, Object>> evidence = new ArrayList<>();
    @Configuration @EnableWebMvc static class Mvc {}

    private GenericWebApplicationContext context(Class<?> controller) {
        GenericWebApplicationContext context = new GenericWebApplicationContext();
        context.setServletContext(new MockServletContext());
        new AnnotatedBeanDefinitionReader(context).register(Mvc.class, controller);
        context.refresh();
        RequestMappingHandlerMapping mappings = context.getBean(RequestMappingHandlerMapping.class);
        mappings.getHandlerMethods().forEach((info, handler) -> {
            Map<String, Object> row = new LinkedHashMap<>();
            row.put("controller", controller.getSimpleName());
            row.put("methodName", handler.getMethod().getName());
            row.put("paths", new TreeSet<>(info.getPatternsCondition().getPatterns()));
            row.put("methods", info.getMethodsCondition().getMethods().stream().map(Object::toString).sorted().collect(Collectors.toList()));
            row.put("consumes", info.getConsumesCondition().getConsumableMediaTypes().stream().map(Object::toString).sorted().collect(Collectors.toList()));
            row.put("produces", info.getProducesCondition().getProducibleMediaTypes().stream().map(Object::toString).sorted().collect(Collectors.toList()));
            evidence.add(row);
        });
        return context;
    }

    private RequestMappingInfo mapping(GenericWebApplicationContext context, String name) {
        return context.getBean(RequestMappingHandlerMapping.class).getHandlerMethods().entrySet().stream()
            .filter(e -> e.getValue().getMethod().getName().equals(name)).map(Map.Entry::getKey).findFirst().get();
    }

    @Test void composedAndAliasesAreActualRoutes() throws Exception {
        try (GenericWebApplicationContext context = context(MappingSamples.Catalog.class)) {
            MockMvc mvc = MockMvcBuilders.webAppContextSetup(context).build();
            mvc.perform(get("/api/items").param("q", "java8")).andExpect(status().isOk()).andExpect(content().string("java8"));
            mvc.perform(get("/api/products").param("q", "alias")).andExpect(status().isOk());
            mvc.perform(get("/api/public")).andExpect(status().isOk());
            mvc.perform(get("/api/alias")).andExpect(status().isOk());
            assertEquals(Collections.singleton("/api"), mapping(context, "fixed").getPatternsCondition().getPatterns());
            assertEquals(Collections.singleton("/api/fixed"), mapping(context, "staticFixed").getPatternsCondition().getPatterns());
            assertEquals(Collections.singleton("/api/default"), mapping(context, "defaultPath").getPatternsCondition().getPatterns());
        }
    }

    @Test void genericInterfaceMappingAndParametersApplyToTheStringOverride() throws Exception {
        try (GenericWebApplicationContext context = context(MappingSamples.ContractController.class)) {
            RequestMappingHandlerMapping mappings = context.getBean(RequestMappingHandlerMapping.class);
            HandlerMethod method = mappings.getHandlerMethods().values().iterator().next();
            assertArrayEquals(new Class<?>[]{String.class}, method.getMethod().getParameterTypes());
            MockMvcBuilders.webAppContextSetup(context).build().perform(get("/contract/search").param("q", "typed"))
                .andExpect(status().isOk()).andExpect(content().string("typed"));
        }
    }

    @Test void inheritedMethodsKeepTheirActualDeclaration() throws Exception {
        try (GenericWebApplicationContext context = context(MappingSamples.Child.class)) {
            HandlerMethod method = context.getBean(RequestMappingHandlerMapping.class).getHandlerMethods().values().iterator().next();
            assertEquals(MappingSamples.Base.class, method.getMethod().getDeclaringClass());
            MockMvcBuilders.webAppContextSetup(context).build().perform(post("/base/send").content("inherited").contentType("text/plain"))
                .andExpect(status().isOk()).andExpect(content().string("inherited"));
        }
    }

    @Test void classAndMethodVerbsAreAUnion() throws Exception {
        try (GenericWebApplicationContext context = context(MappingSamples.Verbs.class)) {
            assertEquals(3, mapping(context, "union").getMethodsCondition().getMethods().size());
            MockMvc mvc = MockMvcBuilders.webAppContextSetup(context).build();
            mvc.perform(get("/verbs/union")).andExpect(status().isOk());
            mvc.perform(post("/verbs/union")).andExpect(status().isOk());
            mvc.perform(put("/verbs/union")).andExpect(status().isOk());
            mvc.perform(delete("/verbs/union")).andExpect(status().isMethodNotAllowed());
        }
    }

    @Test void anUnrestrictedMappingIsNotGetOnly() throws Exception {
        try (GenericWebApplicationContext context = context(MappingSamples.AnyMethod.class)) {
            assertTrue(mapping(context, "any").getMethodsCondition().getMethods().isEmpty());
            MockMvc mvc = MockMvcBuilders.webAppContextSetup(context).build();
            mvc.perform(get("/any")).andExpect(status().isOk());
            mvc.perform(post("/any")).andExpect(status().isOk());
        }
    }

    @AfterAll static void saveEvidence() throws Exception {
        evidence.sort(Comparator.comparing(row -> row.get("controller") + "." + row.get("methodName")));
        String output = System.getProperty("mapping.evidence");
        if (output != null) new ObjectMapper().writerWithDefaultPrettyPrinter().writeValue(Paths.get(output).toFile(), evidence);
    }
}
