package dev.openidea.configuration;

import org.springframework.boot.context.properties.ConfigurationProperties;
import org.springframework.context.annotation.Profile;

@ConfigurationProperties("enabled-profile")
@Profile("never-selected")
public class EnabledProfileProperties {}
