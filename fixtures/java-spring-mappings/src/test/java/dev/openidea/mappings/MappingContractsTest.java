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
    private static final List<Map<String, Object>> identities = new ArrayList<>();
    @Configuration @EnableWebMvc static class Mvc {}

    private GenericWebApplicationContext context(Class<?> controller) {
        GenericWebApplicationContext context = new GenericWebApplicationContext();
        context.setServletContext(new MockServletContext());
        new AnnotatedBeanDefinitionReader(context).register(Mvc.class, controller);
        context.refresh();
        RequestMappingHandlerMapping mappings = context.getBean(RequestMappingHandlerMapping.class);
        mappings.getHandlerMethods().forEach((info, handler) -> {
            Map<String, Object> row = new LinkedHashMap<>();
            row.put("stack", "mvc");
            row.put("controller", controller.getSimpleName());
            row.put("methodName", handler.getMethod().getName());
            row.put("paths", new TreeSet<>(info.getPatternsCondition().getPatterns()));
            row.put("methods", info.getMethodsCondition().getMethods().stream().map(Object::toString).sorted().collect(Collectors.toList()));
            row.put("consumes", info.getConsumesCondition().getConsumableMediaTypes().stream().map(Object::toString).sorted().collect(Collectors.toList()));
            row.put("produces", info.getProducesCondition().getProducibleMediaTypes().stream().map(Object::toString).sorted().collect(Collectors.toList()));
            evidence.add(row);
            saveIdentity("mvc", handler, row);
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

    @Test void singleSidedAliasesWorkInSpring53() throws Exception {
        try (GenericWebApplicationContext context = context(MappingSamples.SingleAliasController.class)) {
            assertEquals(Collections.singleton("/single"), mapping(context, "single").getPatternsCondition().getPatterns());
        }
    }

    @Test void invalidAliasesRejectTheController() {
        assertThrows(Exception.class, () -> context(MappingSamples.InvalidController.class));
        assertThrows(Exception.class, () -> context(MappingSamples.ConflictingController.class));
        assertThrows(Exception.class, () -> context(MappingSamples.TransitiveConflictController.class));
    }

    @Test void crossFileComposedGenericContractsAndForeignNames() throws Exception {
        try (GenericWebApplicationContext context = context(dev.openidea.mappings.web.CrossController.class)) {
            assertEquals(new TreeSet<>(Arrays.asList("/cross/save", "/cross/write", "/v2/save", "/v2/write")),
                mapping(context, "save").getPatternsCondition().getPatterns());
            MockMvcBuilders.webAppContextSetup(context).build().perform(post("/cross/save").contentType("application/json").content("{\"name\":\"typed\"}"))
                .andExpect(status().isOk()).andExpect(content().string("typed"));
        }
        try (GenericWebApplicationContext context = context(dev.openidea.mappings.web.ForeignController.class)) {
            assertTrue(context.getBean(RequestMappingHandlerMapping.class).getHandlerMethods().isEmpty());
        }
    }

    @Test void reactiveAnnotatedMappingsHaveTheSameContracts() {
        Class<?>[] controllers = { MappingSamples.TransitiveController.class, dev.openidea.mappings.web.InheritedController.class, MappingSamples.Catalog.class, MappingSamples.ContractController.class,
            MappingSamples.Child.class, MappingSamples.Verbs.class, MappingSamples.AnyMethod.class,
            MappingSamples.SingleAliasController.class, dev.openidea.mappings.web.CrossController.class,
            dev.openidea.mappings.web.ForeignController.class };
        for (Class<?> controller : controllers) {
            try (org.springframework.context.annotation.AnnotationConfigApplicationContext context =
                    new org.springframework.context.annotation.AnnotationConfigApplicationContext(controller)) {
                org.springframework.web.reactive.result.method.annotation.RequestMappingHandlerMapping mappings =
                    new org.springframework.web.reactive.result.method.annotation.RequestMappingHandlerMapping();
                mappings.setApplicationContext(context);
                mappings.afterPropertiesSet();
                mappings.getHandlerMethods().forEach((info, handler) -> {
                    Map<String, Object> row = new LinkedHashMap<>();
                    row.put("controller", controller.getSimpleName());
                    row.put("methodName", handler.getMethod().getName());
                    row.put("paths", new TreeSet<>(info.getPatternsCondition().getPatterns().stream().map(Object::toString).collect(Collectors.toList())));
                    row.put("methods", info.getMethodsCondition().getMethods().stream().map(Object::toString).sorted().collect(Collectors.toList()));
                    row.put("consumes", info.getConsumesCondition().getConsumableMediaTypes().stream().map(Object::toString).sorted().collect(Collectors.toList()));
                    row.put("produces", info.getProducesCondition().getProducibleMediaTypes().stream().map(Object::toString).sorted().collect(Collectors.toList()));
                    row.put("stack", "webflux");
                    evidence.add(row);
                    saveIdentity("webflux", handler, row);
                });
            }
        }
    }

    @Test void transitiveAliasesAndCrossFileGenericParents() {
        try (GenericWebApplicationContext context = context(MappingSamples.TransitiveController.class)) {
            assertEquals(Collections.singleton("/transitive"), mapping(context, "transitive").getPatternsCondition().getPatterns());
        }
        try (GenericWebApplicationContext context = context(dev.openidea.mappings.web.InheritedController.class)) {
            assertEquals(Collections.singleton("/parent/send"), mapping(context, "send").getPatternsCondition().getPatterns());
            HandlerMethod method = context.getBean(RequestMappingHandlerMapping.class).getHandlerMethods().values().iterator().next();
            assertEquals(dev.openidea.mappings.api.GenericBase.class, method.getMethod().getDeclaringClass());
        }
    }

    private static void saveIdentity(String stack, HandlerMethod handler, Map<String, Object> mapping) {
        Map<String, Object> row = new LinkedHashMap<>();
        row.put("stack", stack);
        row.put("qualifiedController", handler.getBeanType().getName());
        row.put("declaringClass", handler.getMethod().getDeclaringClass().getName());
        row.put("methodName", handler.getMethod().getName());
        row.put("paths", mapping.get("paths"));
        row.put("methods", mapping.get("methods"));
        identities.add(row);
    }

    @AfterAll static void saveEvidence() throws Exception {
        evidence.sort(Comparator.comparing(row -> row.get("controller") + "." + row.get("methodName") + "." + row.get("stack")));
        identities.sort(Comparator.comparing(row -> row.get("qualifiedController") + "." + row.get("methodName") + "." + row.get("stack")));
        String identityOutput = System.getProperty("mapping.identityEvidence");
        if (identityOutput != null) new ObjectMapper().writerWithDefaultPrettyPrinter().writeValue(Paths.get(identityOutput).toFile(), identities);
        String output = System.getProperty("mapping.evidence");
        if (output != null) new ObjectMapper().writerWithDefaultPrettyPrinter().writeValue(Paths.get(output).toFile(), evidence);
    }
}
