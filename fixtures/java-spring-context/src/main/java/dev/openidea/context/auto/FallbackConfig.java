package dev.openidea.context.auto;
import dev.openidea.context.Port;
import org.springframework.boot.autoconfigure.AutoConfiguration;
import org.springframework.boot.autoconfigure.condition.ConditionalOnMissingBean;
import org.springframework.context.annotation.Bean;
@AutoConfiguration
public class FallbackConfig {
    @Bean @ConditionalOnMissingBean(Port.class)
    public Port fallback() { return new Port() {}; }
}
