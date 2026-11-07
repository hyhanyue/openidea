package dev.openidea.refactor;
import java.beans.Introspector;
import java.beans.PropertyDescriptor;
import java.util.Collections;
import org.junit.jupiter.api.Test;
import org.springframework.boot.context.properties.bind.Bindable;
import org.springframework.boot.context.properties.bind.Binder;
import org.springframework.boot.context.properties.source.MapConfigurationPropertySource;
import static org.junit.jupiter.api.Assertions.*;
class BindingAliasesTest {
    @Test void eachResourceAliasBindsTheActualJavaBeanProperty() throws Exception {
        for (PropertyDescriptor property : Introspector.getBeanInfo(Settings.class).getPropertyDescriptors()) {
            if (property.getPropertyType() != String.class || property.getName().equals("label")) continue;
            String camel = property.getName();
            String kebab = camel.replaceAll("([a-z])([A-Z])", "$1-$2").toLowerCase(java.util.Locale.ROOT);
            String snake = kebab.replace('-', '_');
            for (String alias : new String[] { camel, kebab, snake }) {
                MapConfigurationPropertySource source = new MapConfigurationPropertySource(Collections.singletonMap("sample." + alias, "alias-value"));
                Settings bound = new Binder(source).bind("sample", Bindable.of(Settings.class)).get();
                assertEquals("alias-value", property.getReadMethod().invoke(bound));
            }
        }
    }
}
