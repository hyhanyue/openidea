package dev.openidea.refactor;
import org.springframework.boot.context.properties.ConfigurationProperties;
@ConfigurationProperties(prefix = "sample")
public class Settings {
    private String label;
    public String getLabel() { return label; }
    public void setLabel(String label) { this.label = label; }
}
