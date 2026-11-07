package dev.openidea.refactor;
import org.springframework.beans.factory.annotation.Qualifier;
import org.springframework.beans.factory.annotation.Value;
import org.springframework.stereotype.Component;
@Component
public class Consumer {
    public final String port;
    public final Integer counter;
    @Value("${service.timeout:99}") public int timeout;
    public Consumer(/*😀*/ @Qualifier("port") String port, @Qualifier("counter") Integer counter) { this.port = port; this.counter = counter; }
    public String plainText() { return "port service.timeout find"; }
}
