package dev.openidea.configuration;

import org.springframework.boot.context.properties.ConfigurationProperties;

@ConfigurationProperties(prefix = "demo")
public class DemoProperties {
    private String name;
    private final Nested nested = new Nested();
    public String getName() { return name; }
    public void setName(String name) { this.name = name; }
    public Nested getNested() { return nested; }
    public static class Nested {
        private String label;
        public String getLabel() { return label; }
        public void setLabel(String label) { this.label = label; }
    }
}
