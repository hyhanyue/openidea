package demo;

import static org.junit.jupiter.api.Assertions.assertEquals;
import org.junit.jupiter.api.Test;

public class WorkflowIT {
    @Test
    void integration() {
        assertEquals("configured", System.getProperty("fixture.it"));
        assertEquals(42, Consumer.answer());
    }

    @Test
    void otherIntegration() {}
}
