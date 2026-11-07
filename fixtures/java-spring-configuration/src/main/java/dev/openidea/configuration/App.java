package dev.openidea.configuration;

import org.springframework.boot.autoconfigure.SpringBootApplication;
import org.springframework.boot.context.properties.ConfigurationPropertiesScan;
import org.springframework.boot.context.properties.EnableConfigurationProperties;
import org.springframework.context.annotation.Import;

@SpringBootApplication
@EnableConfigurationProperties({DemoProperties.class, EnabledProfileProperties.class})
@ConfigurationPropertiesScan("dev.openidea.configuration.scanned")
@Import(Plain.class)
public class App {}
