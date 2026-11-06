package demo;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import org.junit.jupiter.api.Test;

public class SameTest {
    @Test
    void same() {
        assertFalse(Boolean.getBoolean("openideaFail"), "Fails only in beta");
    }

    @Test
    void betaOnly() {
        assertEquals(42, Consumer.answer());
    }
}
