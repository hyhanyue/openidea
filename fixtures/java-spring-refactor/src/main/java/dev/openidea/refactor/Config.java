package dev.openidea.refactor;
import org.springframework.context.annotation.Bean;
import org.springframework.context.annotation.Configuration;
@Configuration
public class Config {
    @Bean("port") public String buildPort() { return "port"; }
    @Bean public Integer counter() { return 5; }
}
