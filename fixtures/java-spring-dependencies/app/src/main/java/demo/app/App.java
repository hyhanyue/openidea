package demo.app;
import org.springframework.boot.autoconfigure.SpringBootApplication;
import org.springframework.context.annotation.Import;
import org.springframework.beans.factory.annotation.Autowired;
import demo.api.Port;
import demo.api.Token;
@SpringBootApplication @Import({vendor.LibraryConfig.class, demo.api.ModuleConfig.class, other.Token.class})
public class App {
    /* 😀 */ @Autowired Port port;
    @Autowired demo.api.ModuleValue moduleValue;
    @Autowired Token token;
    @Autowired ScannedLibrary scanned;
}