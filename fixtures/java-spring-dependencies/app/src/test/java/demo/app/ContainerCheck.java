package demo.app;
import org.springframework.boot.SpringApplication;
import org.springframework.context.ConfigurableApplicationContext;
public class ContainerCheck {
    public static void main(String[] args) {
        SpringApplication app = new SpringApplication(App.class);
        app.setWebApplicationType(org.springframework.boot.WebApplicationType.NONE);
        try (ConfigurableApplicationContext c = app.run("--spring.main.banner-mode=off", "--feature.enabled=false")) {
            if (!"jar".equals(c.getBean(demo.api.Port.class).value())) throw new AssertionError();
            for (String bean : new String[]{"libraryClock","secondPort","other.Token","moduleValue","libraryPort","portAlias","token","scannedLibrary","autoMessage","legacyNumber"}) if (!c.containsBean(bean)) throw new AssertionError(bean);
            for (String bean : new String[]{"unscanned","conditionalNumber","absentNumber"}) if (c.containsBean(bean)) throw new AssertionError(bean);
            System.out.println("JAVA8_CONTAINER_OK libraryPort portAlias token scannedLibrary autoMessage legacyNumber");
        }
    }
}