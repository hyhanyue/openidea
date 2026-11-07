package dev.openidea.async;

import java.util.concurrent.TimeUnit;
import org.junit.jupiter.api.Test;
import org.springframework.aop.AopInvocationException;
import org.springframework.context.annotation.*;
import org.springframework.scheduling.annotation.EnableAsync;
import org.springframework.scheduling.concurrent.ThreadPoolTaskExecutor;
import static org.junit.jupiter.api.Assertions.*;

class AsyncContractsTest {
    @Configuration @EnableAsync(proxyTargetClass = true)
    static class Config {
        @Bean AsyncSamples.ClassJobs classJobs() { return new AsyncSamples.ClassJobs(); }
        @Bean AsyncSamples.MethodJobs methodJobs() { return new AsyncSamples.MethodJobs(); }
        @Bean ThreadPoolTaskExecutor taskExecutor() {
            ThreadPoolTaskExecutor executor = new ThreadPoolTaskExecutor();
            executor.setCorePoolSize(1);
            executor.setMaxPoolSize(1);
            executor.setThreadNamePrefix("async-contract-");
            return executor;
        }
    }

    @Test void typeLevelAnnotationDiscardsAnUnsupportedReferenceResult() {
        try (AnnotationConfigApplicationContext context = new AnnotationConfigApplicationContext(Config.class)) {
            assertNull(context.getBean(AsyncSamples.ClassJobs.class).text());
        }
    }

    @Test void primitiveResultCannotBeReturnedThroughTheProxy() {
        try (AnnotationConfigApplicationContext context = new AnnotationConfigApplicationContext(Config.class)) {
            assertThrows(AopInvocationException.class, () -> context.getBean(AsyncSamples.ClassJobs.class).count());
        }
    }

    @Test void completionStageDeclarationIsNotADeclaredFuture() {
        try (AnnotationConfigApplicationContext context = new AnnotationConfigApplicationContext(Config.class)) {
            assertNull(context.getBean(AsyncSamples.ClassJobs.class).stage());
        }
    }

    @Test void aCompletableFutureDeliversItsResultAndHelpersRemainSynchronous() throws Exception {
        try (AnnotationConfigApplicationContext context = new AnnotationConfigApplicationContext(Config.class)) {
            AsyncSamples.ClassJobs jobs = context.getBean(AsyncSamples.ClassJobs.class);
            assertTrue(jobs.future().get(5, TimeUnit.SECONDS).startsWith("async-contract-"));
            assertTrue(jobs.future().get(5, TimeUnit.SECONDS).endsWith(":helper"));
            assertEquals("local", jobs.local());
            assertEquals(42, AsyncSamples.ClassJobs.staticValue());
        }
    }

    @Test void directMethodAnnotationsFollowTheSameContract() throws Exception {
        try (AnnotationConfigApplicationContext context = new AnnotationConfigApplicationContext(Config.class)) {
            AsyncSamples.MethodJobs jobs = context.getBean(AsyncSamples.MethodJobs.class);
            assertNull(jobs.text());
            assertEquals("method future", jobs.future().get(5, TimeUnit.SECONDS));
            assertEquals("normal", jobs.normal());
        }
    }
}
