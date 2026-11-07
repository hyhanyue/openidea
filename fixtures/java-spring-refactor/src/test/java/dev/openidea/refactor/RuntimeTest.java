package dev.openidea.refactor;
import org.junit.jupiter.api.Test;
import org.springframework.boot.WebApplicationType;
import org.springframework.boot.builder.SpringApplicationBuilder;
import org.springframework.context.ConfigurableApplicationContext;
import static org.junit.jupiter.api.Assertions.*;
class RuntimeTest {
    @Test void startsBindsInjectsAndQueries() {
        assertEquals("1.8", System.getProperty("java.specification.version"));
        try (ConfigurableApplicationContext ctx = new SpringApplicationBuilder(App.class).web(WebApplicationType.NONE).run()) {
            Consumer consumer = ctx.getBean(Consumer.class);
            assertEquals("port", consumer.port);
            assertEquals("port service.timeout find", consumer.plainText());
            assertEquals(11, consumer.timeout);
            assertEquals("ready", ctx.getBean(Settings.class).getLabel());
            UserMapper mapper = ctx.getBean(UserMapper.class);
            assertEquals(Integer.valueOf(7), mapper.find(1));
            assertEquals(Integer.valueOf(7), mapper.nested().getValue());
            assertEquals(Integer.valueOf(19), ctx.getBean(OtherMapper.class).find());
            assertEquals(Integer.valueOf(5), consumer.counter);
        }
    }
}
