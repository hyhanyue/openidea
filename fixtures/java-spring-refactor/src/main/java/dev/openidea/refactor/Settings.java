package dev.openidea.refactor;
import org.springframework.boot.context.properties.ConfigurationProperties;
@ConfigurationProperties(prefix = "sample")
public class Settings {
    private String label;
    private String displayName;
    public String getLabel() { return label; }
    public void setLabel(String label) { this.label = label; }
    public String getDisplayName() { return displayName; }
    public void setDisplayName(String displayName) { this.displayName = displayName; }
}
