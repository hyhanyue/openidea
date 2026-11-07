package dev.openidea.refactor;
import org.springframework.beans.factory.annotation.Value;
import org.springframework.stereotype.Component;
@Component
public class PropertyClient {
    private final Settings settings;
    @Value("${sample.display-name:fallback}") public String canonical;
    @Value("${sample.displayName:fallback}") public String camel;
    @Value("${sample.display_name:fallback}") public String snake;
    public PropertyClient(Settings settings) { this.settings = settings; }
    public String roundTrip() {
        String current = settings.getDisplayName();
        settings.setDisplayName(current);
        return current;
    }
    public String plainText() { return "sample.display-name"; }
}
