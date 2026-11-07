package dev.openidea.context.components;
import dev.openidea.context.Port;
import org.springframework.stereotype.Component;
import org.springframework.context.annotation.Profile;
import org.springframework.context.annotation.Primary;
@Component @Primary @Profile("prod")
public class ProdPort implements Port {}
