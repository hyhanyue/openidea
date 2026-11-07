package com.example.runtime;

import org.springframework.beans.factory.annotation.Value;
import org.springframework.boot.autoconfigure.condition.ConditionalOnProperty;
import org.springframework.context.annotation.Bean;
import org.springframework.context.annotation.Configuration;
import org.springframework.security.config.annotation.web.builders.HttpSecurity;
import org.springframework.security.config.annotation.web.configuration.EnableWebSecurity;
import org.springframework.security.core.userdetails.User;
import org.springframework.security.core.userdetails.UserDetailsService;
import org.springframework.security.provisioning.InMemoryUserDetailsManager;
import org.springframework.security.web.SecurityFilterChain;

@Configuration
@EnableWebSecurity
@ConditionalOnProperty(name = "fixture.secured", havingValue = "true")
public class FixtureSecurity {
    @Bean
    public SecurityFilterChain fixtureChain(HttpSecurity http) throws Exception {
        http.authorizeRequests().antMatchers("/actuator/configprops").denyAll()
            .anyRequest().authenticated().and().httpBasic();
        return http.build();
    }
    @Bean
    public UserDetailsService fixtureUsers(@Value("${fixture.test-user}") String user,
                                          @Value("${fixture.test-password}") String password) {
        return new InMemoryUserDetailsManager(User.withUsername(user).password("{noop}" + password).roles("TEST").build());
    }
}
