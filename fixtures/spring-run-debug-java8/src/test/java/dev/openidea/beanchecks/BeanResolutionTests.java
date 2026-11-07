package dev.openidea.beanchecks;

import org.junit.jupiter.api.Test;
import org.springframework.context.annotation.AnnotationConfigApplicationContext;
import org.springframework.beans.factory.UnsatisfiedDependencyException;
import static org.junit.jupiter.api.Assertions.*;

class BeanResolutionTests {
    @Test void primaryAndQualifierMatchNavigation() {
        try (AnnotationConfigApplicationContext ctx = new AnnotationConfigApplicationContext(GreetingConfiguration.class, BeanNavigationSample.class)) {
            BeanNavigationSample bean = ctx.getBean(BeanNavigationSample.class);
            assertEquals("primary", bean.primary.name());
            assertEquals("backup", bean.backup.name());
        }
    }
    @Test void missingBeanFailsActualSpringInjection() {
        assertThrows(UnsatisfiedDependencyException.class, () -> {
            try (AnnotationConfigApplicationContext ctx = new AnnotationConfigApplicationContext()) {
                ctx.register(MissingBeanSample.class);
                ctx.refresh();
            }
        });
    }
    @Test void ambiguousBeansFailActualSpringInjection() {
        assertThrows(UnsatisfiedDependencyException.class, () -> {
            try (AnnotationConfigApplicationContext ctx = new AnnotationConfigApplicationContext()) {
                ctx.register(EmailChannel.class, SmsChannel.class, AmbiguousBeanSample.class);
                ctx.refresh();
            }
        });
    }
}
