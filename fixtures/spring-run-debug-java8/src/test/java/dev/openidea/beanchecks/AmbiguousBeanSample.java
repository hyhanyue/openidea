package dev.openidea.beanchecks;

import org.springframework.stereotype.Component;
import org.springframework.stereotype.Service;

interface DeliveryChannel {}
@Component class EmailChannel implements DeliveryChannel {}
@Component class SmsChannel implements DeliveryChannel {}

// Inspection sample: add @Qualifier("emailChannel") to select one implementation.
@Service
    class AmbiguousBeanSample {
        AmbiguousBeanSample(DeliveryChannel channel) {}
    }
