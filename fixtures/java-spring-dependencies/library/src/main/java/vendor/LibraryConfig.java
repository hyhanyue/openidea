package vendor;
import org.springframework.context.annotation.*;
import demo.api.Port;
@Configuration public class LibraryConfig {
    static { if (System.getProperty("openidea.model.only") != null) throw new IllegalStateException("Application code was executed by the indexer"); }
    @Bean(name={"libraryPort","portAlias"}) public Port libraryPort(demo.api.Token token) { return () -> "jar"; }
    @Bean public demo.api.Token token() { return new demo.api.Token(); }
}