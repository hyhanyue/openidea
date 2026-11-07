package vendor;
import org.springframework.context.annotation.*;
@Configuration public class LegacyAutoConfig { @Bean public Long legacyNumber() { return 7L; } }