package com.example.runtime;

import java.util.Collections;
import org.springframework.context.ApplicationContextInitializer;
import org.springframework.context.ConfigurableApplicationContext;
import org.springframework.core.env.MapPropertySource;

public class FixtureSources implements ApplicationContextInitializer<ConfigurableApplicationContext> {
    @Override
    public void initialize(ConfigurableApplicationContext context) {
        String marker = "SYNTHETIC_SOURCE_AUTH_MUST_NOT_CROSS_IPC";
        context.getEnvironment().getPropertySources().addLast(new MapPropertySource(
            "configserver:https://fixture:" + marker + "@config.example.test/app?token=" + marker,
            Collections.<String, Object>singletonMap("fixture.external-origin", marker)));
        context.getEnvironment().getPropertySources().addLast(new MapPropertySource(
            "programmatic value=" + marker,
            Collections.<String, Object>singletonMap("fixture.programmatic-origin", marker)));
    }
}
