package dev.openidea.context.imported;
import org.springframework.context.annotation.Configuration;
import org.springframework.context.annotation.Bean;
@Configuration
public class ImportedConfig {
    @Bean public java.time.Clock clock() { return java.time.Clock.systemUTC(); }
}
