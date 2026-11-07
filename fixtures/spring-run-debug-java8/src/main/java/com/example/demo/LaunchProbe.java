package com.example.demo;

import java.util.LinkedHashMap;
import java.util.Map;
import org.springframework.boot.context.properties.ConfigurationProperties;
import org.springframework.context.annotation.Bean;
import org.springframework.context.annotation.Configuration;
import org.springframework.context.annotation.Profile;
import org.springframework.core.env.Environment;
import org.springframework.web.bind.annotation.GetMapping;
import org.springframework.web.bind.annotation.RestController;

@RestController
public class LaunchProbe {
    private final Environment environment;
    public LaunchProbe(Environment environment) { this.environment = environment; }

    @GetMapping("/launch-probe")
    public Map<String, Object> probe() {
        Map<String, Object> result = new LinkedHashMap<String, Object>();
        result.put("profiles", environment.getActiveProfiles());
        result.put("env", System.getenv("OPENIDEA_PROBE"));
        result.put("file", System.getenv("FILE_PROBE"));
        result.put("vm", System.getProperty("probe.vm"));
        result.put("arg", environment.getProperty("probe.arg"));
        result.put("java", System.getProperty("java.specification.version"));
        result.put("cwd", System.getProperty("user.dir"));
        result.put("inheritedHome", System.getenv("HOME") != null);
        return result;
    }

    interface Missing {}
    interface Choice {}

    @Configuration @Profile("missing")
    static class MissingConfig { @Bean Object broken(Missing required) { return required; } }

    @Configuration @Profile("ambiguous")
    static class AmbiguousConfig {
        @Bean Choice first() { return new Choice() {}; }
        @Bean Choice second() { return new Choice() {}; }
        @Bean Object broken(Choice choice) { return choice; }
    }

    @Configuration @Profile("binding")
    static class BindingConfig {
        @Bean @ConfigurationProperties("probe") ProbeProperties probeProperties() { return new ProbeProperties(); }
    }
    static class ProbeProperties {
        private int number;
        public int getNumber() { return number; }
        public void setNumber(int number) { this.number = number; }
    }
}
