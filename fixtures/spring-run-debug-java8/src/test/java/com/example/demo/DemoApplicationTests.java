package com.example.demo;

import org.junit.jupiter.api.Test;
import org.springframework.beans.factory.annotation.Autowired;
import org.springframework.boot.test.context.SpringBootTest;
import static org.assertj.core.api.Assertions.assertThat;

@SpringBootTest
class DemoApplicationTests {
    @Autowired private UserMapper userMapper;
    @Autowired private DemoProperties properties;

    @Test
    void xmlStatementsAndConfigurationWork() {
        assertThat(userMapper.findById(1L).getName()).isEqualTo("Alice");
        assertThat(userMapper.findByName("Bo")).extracting(User::getName).containsExactly("Bob");
        assertThat(userMapper.findById(999L)).isNull();
        assertThat(properties.getGreeting()).isEqualTo("Hello OpenIDEA Java 8");
    }

    @Test
    void generatedCountStatementExecutesAgainstH2() {
        assertThat(userMapper.countByName("Alice")).isEqualTo(1L);
        assertThat(userMapper.countByName("Bob")).isEqualTo(1L);
        assertThat(userMapper.countByName("Nobody")).isZero();
    }
}
