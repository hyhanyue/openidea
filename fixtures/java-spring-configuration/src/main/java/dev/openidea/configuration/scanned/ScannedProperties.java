package dev.openidea.configuration.scanned;

import org.springframework.boot.context.properties.ConfigurationProperties;

@ConfigurationProperties("scan")
public class ScannedProperties {
    private String mode;
    public String getMode() { return mode; }
    public void setMode(String mode) { this.mode = mode; }
}
