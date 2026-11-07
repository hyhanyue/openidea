package vendor;
import org.springframework.context.annotation.*;
@Configuration public class AutoConfig {
    @Bean public String autoMessage() { return "auto"; }
}