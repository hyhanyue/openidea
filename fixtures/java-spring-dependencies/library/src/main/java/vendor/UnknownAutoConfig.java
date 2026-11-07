package vendor;
import org.springframework.context.annotation.*;
import org.springframework.boot.autoconfigure.condition.*;
@Configuration @ConditionalOnProperty(name="feature.enabled",havingValue="true")
public class UnknownAutoConfig { @Bean public Integer conditionalNumber() { return 42; } }