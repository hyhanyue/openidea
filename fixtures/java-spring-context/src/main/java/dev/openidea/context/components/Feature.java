package dev.openidea.context.components;
import org.springframework.stereotype.Component;
import org.springframework.boot.autoconfigure.condition.ConditionalOnProperty;
@Component @ConditionalOnProperty(prefix="feature",name="enabled",havingValue="yes")
public class Feature {}
