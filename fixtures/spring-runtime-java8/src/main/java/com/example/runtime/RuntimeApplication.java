package com.example.runtime;

import org.springframework.boot.SpringApplication;
import org.springframework.boot.autoconfigure.SpringBootApplication;
import org.springframework.boot.autoconfigure.condition.ConditionalOnProperty;
import org.springframework.boot.context.properties.ConfigurationProperties;
import org.springframework.boot.context.properties.EnableConfigurationProperties;
import org.springframework.context.annotation.Bean;
import org.springframework.web.bind.annotation.GetMapping;
import org.springframework.web.bind.annotation.RestController;

@SpringBootApplication(exclude = {
    org.springframework.boot.autoconfigure.security.servlet.SecurityAutoConfiguration.class,
    org.springframework.boot.autoconfigure.security.servlet.UserDetailsServiceAutoConfiguration.class,
    org.springframework.boot.actuate.autoconfigure.security.servlet.ManagementWebSecurityAutoConfiguration.class
})
@EnableConfigurationProperties(RuntimeApplication.DemoProperties.class)
@RestController
public class RuntimeApplication {
    public static void main(String[] args) { SpringApplication.run(RuntimeApplication.class, args); }
    @GetMapping("/runtime-fixture")
    public String fixture() { return "fixture"; }
    @Bean
    @ConditionalOnProperty(name = "fixture.enabled", havingValue = "true")
    public String enabledFixture() { return "fixture"; }
    @Bean
    @ConditionalOnProperty(name = "fixture.absent", havingValue = "true")
    public Integer absentFixture() { return 1; }

    @ConfigurationProperties(prefix = "fixture.demo")
    public static class DemoProperties {
        private String value;
        public String getValue() { return value; }
        public void setValue(String value) { this.value = value; }
    }
}
