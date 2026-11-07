package dev.openidea.context.components;
import dev.openidea.context.Port;
import org.springframework.stereotype.Component;
import org.springframework.context.annotation.Profile;
@Component @Profile("dev & !cloud")
public class DevPort implements Port {}
