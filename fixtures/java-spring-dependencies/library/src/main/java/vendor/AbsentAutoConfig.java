package vendor;
import org.springframework.context.annotation.*;
import org.springframework.boot.autoconfigure.condition.*;
@Configuration @ConditionalOnClass(name="missing.NeverInstalled")
public class AbsentAutoConfig { @Bean public Double absentNumber() { return 1.0; } }