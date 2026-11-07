package dev.openidea.beanchecks;

import org.springframework.stereotype.Service;

interface MissingGateway {}

// Inspection sample: register a Bean implementing MissingGateway to clear the warning.
@Service
class MissingBeanSample {
    MissingBeanSample(MissingGateway gateway) {}
}
