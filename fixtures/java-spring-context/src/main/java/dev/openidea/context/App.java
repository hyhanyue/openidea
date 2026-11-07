package dev.openidea.context;
import org.springframework.boot.autoconfigure.SpringBootApplication;
import org.springframework.context.annotation.Import;
import dev.openidea.context.imported.ImportedConfig;
import dev.openidea.context.auto.FallbackConfig;
@SpringBootApplication(scanBasePackages="dev.openidea.context.components")
@Import({ImportedConfig.class, FallbackConfig.class})
public class App {}
