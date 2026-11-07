package dev.openidea.configuration;

import java.util.Collections;
import java.util.LinkedHashMap;
import java.util.Map;
import dev.openidea.configuration.scanned.ScannedProperties;
import org.junit.jupiter.api.Test;
import org.springframework.boot.Banner;
import org.springframework.boot.WebApplicationType;
import org.springframework.boot.builder.SpringApplicationBuilder;
import org.springframework.context.ConfigurableApplicationContext;
import org.springframework.core.env.MapPropertySource;
import org.springframework.core.env.StandardEnvironment;
import org.springframework.core.env.SystemEnvironmentPropertySource;
import static org.junit.jupiter.api.Assertions.*;

class ConfigurationEnvironmentTest {
    private ConfigurableApplicationContext start(Map<String,Object> env, Map<String,Object> system, String... args) {
        StandardEnvironment environment = new StandardEnvironment();
        environment.getPropertySources().remove(StandardEnvironment.SYSTEM_PROPERTIES_PROPERTY_SOURCE_NAME);
        environment.getPropertySources().remove(StandardEnvironment.SYSTEM_ENVIRONMENT_PROPERTY_SOURCE_NAME);
        environment.getPropertySources().addLast(new MapPropertySource("systemProperties", system));
        environment.getPropertySources().addLast(new SystemEnvironmentPropertySource("systemEnvironment", env));
        return new SpringApplicationBuilder(App.class).environment(environment).web(WebApplicationType.NONE)
            .bannerMode(Banner.Mode.OFF).logStartupInfo(false).registerShutdownHook(false).run(args);
    }
    private ConfigurableApplicationContext start(String... args) {
        return start(Collections.emptyMap(), Collections.emptyMap(), args);
    }
    @Test void includesGroupsProfileImportsAndBindings() {
        try (ConfigurableApplicationContext context = start("--spring.profiles.active=production")) {
            assertArrayEquals(new String[]{"common","production","proddb","prodmq"},context.getEnvironment().getActiveProfiles());
            assertEquals("imported-production", context.getBean(DemoProperties.class).getName());
            assertEquals("nested-yaml", context.getBean(DemoProperties.class).getNested().getLabel());
            assertEquals("hello imported-production", context.getEnvironment().getProperty("demo.greeting"));
            assertEquals("production", context.getEnvironment().getProperty("import.profile"));
            assertEquals("last-document", context.getEnvironment().getProperty("demo.document"));
            assertEquals("scanned", context.getBean(ScannedProperties.class).getMode());
            assertSame(context.getBean(DemoProperties.class),context.getBean(Plain.class).getProperties());
            assertTrue(context.containsBean("demo-dev.openidea.configuration.DemoProperties"));
            assertTrue(context.containsBean("dev.openidea.configuration.Plain"));
            assertNotNull(context.getBean(EnabledProfileProperties.class));
        }
    }
    @Test void profileSwitchAndDefaultPlaceholders() {
        try (ConfigurableApplicationContext context = start("--spring.profiles.active=dev")) {
            assertArrayEquals(new String[]{"common","dev"},context.getEnvironment().getActiveProfiles());
            assertEquals("dev",context.getBean(DemoProperties.class).getName());
            assertEquals("external",context.getEnvironment().getProperty("external.priority"));
            assertEquals("root-profile",context.getEnvironment().getProperty("location.flag"));
            assertEquals("yes",context.getEnvironment().getProperty("wildcard.flag"));
            assertEquals("dev",context.getEnvironment().getProperty("demo.nested-default"));
            assertEquals("默认",context.getEnvironment().getProperty("demo.unicode"));
            assertNull(context.getEnvironment().getProperty("demo.document"));
            assertThrows(IllegalArgumentException.class,()->context.getEnvironment().getProperty("demo.a"));
        }
    }
    @Test void environmentSystemJsonAndCommandLinePrecedence() {
        Map<String,Object> env = new LinkedHashMap<>();
        env.put("DEMO_NAME","environment");
        env.put("DEMO_ITEMS_0_NAME","nested-env");
        Map<String,Object> system = new LinkedHashMap<>();
        system.put("demo.name","system");
        try(ConfigurableApplicationContext context = start(env,system,"--spring.profiles.active=dev")) {
            assertEquals("system",context.getBean(DemoProperties.class).getName());
            assertEquals("nested-env",context.getEnvironment().getProperty("demo.items[0].name"));
        }
        env.put("SPRING_APPLICATION_JSON","{\"demo\":{\"name\":\"json\"}}");
        try(ConfigurableApplicationContext context = start(env,system,"--spring.profiles.active=dev")) {
            assertEquals("json",context.getBean(DemoProperties.class).getName());
        }
        try(ConfigurableApplicationContext context = start(env,system,"--spring.profiles.active=dev","--demo.name=cli")) {
            assertEquals("cli",context.getBean(DemoProperties.class).getName());
        }
    }
    @Test void localFileAndRelativeImportsAndRepeatedCycleLoadOnce() {
        try(ConfigurableApplicationContext context = start("--spring.profiles.active=dev","--spring.config.import=file:./testdata/outer.properties")) {
            assertEquals("nested",context.getEnvironment().getProperty("external"));
            assertEquals("classpath",context.getEnvironment().getProperty("imported"));
        }
    }
    @Test void laterDocumentsOverrideEarlierImports() {
        try(ConfigurableApplicationContext context = start("--spring.profiles.active=dev","--spring.config.location=classpath:/cases/multidoc.properties")) {
            assertEquals("last-document",context.getEnvironment().getProperty("flag"));
        }
    }
    @Test void optionalMissingImportsPermitStartup() {
        try(ConfigurableApplicationContext context = start("--spring.config.location=optional:classpath:/cases/optional.properties")) {
            assertEquals("optional-survives",context.getEnvironment().getProperty("flag"));
        }
    }
    @Test void requiredMissingImportsAndProfileSpecificIncludesFail() {
        assertThrows(org.springframework.boot.context.config.ConfigDataResourceNotFoundException.class,()->start("--spring.config.location=classpath:/cases/missing.properties"));
        assertThrows(org.springframework.boot.context.config.InvalidConfigDataPropertyException.class,()->start("--spring.profiles.active=dev","--spring.config.location=classpath:/cases/invalid.properties"));
    }
    @Test void importedProfileVariantsArePromotedAndOrdinaryProfileFilesOverrideActivatedDocuments() {
        try(ConfigurableApplicationContext context = start("--spring.profiles.active=dev","--spring.config.location=classpath:/cases/order.properties")) {
            assertEquals("import-profile",context.getEnvironment().getProperty("flag"));
        }
        try(ConfigurableApplicationContext context = start("--spring.profiles.active=dev","--spring.config.location=classpath:/cases/group/")) {
            assertEquals("profile-file",context.getEnvironment().getProperty("flag"));
        }
    }

}
