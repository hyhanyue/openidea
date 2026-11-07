package dev.openidea.async;

import java.util.concurrent.CompletableFuture;
import java.util.concurrent.CompletionStage;
import org.springframework.scheduling.annotation.Async;

public class AsyncSamples {
    @Async
    public static class ClassJobs {
        public String text() { return "not delivered"; }
        public int count() { return 7; }
        public CompletionStage<String> stage() { return CompletableFuture.completedFuture("not delivered"); }
        public CompletableFuture<String> future() {
            return CompletableFuture.completedFuture(Thread.currentThread().getName() + ":" + helper());
        }
        private String helper() { return "helper"; }
        public final String local() { return "local"; }
        public static int staticValue() { return 42; }
    }

    public static class MethodJobs {
        @Async public String text() { return "not delivered"; }
        @Async public CompletableFuture<String> future() { return CompletableFuture.completedFuture("method future"); }
        public String normal() { return "normal"; }
    }
}
