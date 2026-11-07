package dev.openidea.beanchecks;

import org.springframework.context.annotation.Bean;
import org.springframework.context.annotation.Configuration;
import org.springframework.context.annotation.Primary;
import org.springframework.beans.factory.annotation.Qualifier;
import org.springframework.stereotype.Service;

interface GreetingChannel { String name(); }
class PrimaryGreetingChannel implements GreetingChannel { public String name() { return "primary"; } }
class BackupGreetingChannel implements GreetingChannel { public String name() { return "backup"; } }

@Configuration
class GreetingConfiguration {
    @Bean @Primary GreetingChannel primaryGreetingChannel() { return new PrimaryGreetingChannel(); }
    @Bean GreetingChannel backupGreetingChannel() { return new BackupGreetingChannel(); }
}

// Click the green Spring injection icon, or Go to Definition on GreetingChannel.
@Service
class BeanNavigationSample {
    final GreetingChannel primary;
    final GreetingChannel backup;

    BeanNavigationSample(GreetingChannel primary, @Qualifier("backupGreetingChannel") GreetingChannel backup) {
        this.primary = primary;
        this.backup = backup;
    }
}
