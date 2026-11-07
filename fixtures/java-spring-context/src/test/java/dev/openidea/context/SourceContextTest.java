package dev.openidea.context;

import java.util.Collections;
import org.junit.jupiter.api.Test;
import org.springframework.context.annotation.AnnotationConfigApplicationContext;
import org.springframework.core.env.MapPropertySource;
import dev.openidea.context.components.Feature;
import static org.junit.jupiter.api.Assertions.*;

class SourceContextTest {
    private AnnotationConfigApplicationContext open(String profile, String feature) {
        AnnotationConfigApplicationContext context = new AnnotationConfigApplicationContext();
        if (!profile.isEmpty()) context.getEnvironment().setActiveProfiles(profile.split(","));
        context.getEnvironment().getPropertySources().addFirst(new MapPropertySource("test",
                Collections.<String,Object>singletonMap("feature.enabled", feature)));
        context.register(App.class);
        context.refresh();
        return context;
    }
    @Test void devProfileUsesScannedBeanAndImportsConfigurationOutsideScan() {
        try (AnnotationConfigApplicationContext context = open("dev", "yes")) {
            assertEquals(Collections.singleton("devPort"), context.getBeansOfType(Port.class).keySet());
            assertNotNull(context.getBean(java.time.Clock.class));
            assertNotNull(context.getBean(Feature.class));
            assertFalse(context.containsBean("outsidePort"));
        }
    }
    @Test void prodProfileChangesCandidatesAndKnownPropertyDisablesFeature() {
        try (AnnotationConfigApplicationContext context = open("prod", "false")) {
            assertEquals(Collections.singleton("prodPort"), context.getBeansOfType(Port.class).keySet());
            assertTrue(context.getBeansOfType(Feature.class).isEmpty());
        }
    }
    @Test void noMatchingUserBeanAllowsRuntimeFallback() {
        try (AnnotationConfigApplicationContext context = open("", "yes")) {
            assertEquals(Collections.singleton("fallback"), context.getBeansOfType(Port.class).keySet());
        }
    }
    @Test void negatedProfileExpressionExcludesDevInCloud() {
        try (AnnotationConfigApplicationContext context = open("dev,cloud", "yes")) {
            assertEquals(Collections.singleton("fallback"), context.getBeansOfType(Port.class).keySet());
        }
    }
}
