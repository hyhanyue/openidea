# Spring Runtime Java 8 fixture

Boot 2.7.18, Java 8. Independent application, loopback only, ephemeral port by default.
Only the five tested management endpoints are exposed. This configuration is for this fixture only.
Never copy it into a user's application automatically. The marker is synthetic test data.

Build with JDK 8 and Maven: `mvn -o -DskipTests package`.
Run `java -jar target/spring-runtime-java8-1.0.0.jar --server.port=<unused-local-port>`.
Run the ignored `real_java8_fixture` Rust test with `SPRING_RUNTIME_FIXTURE_URL=http://127.0.0.1:<port>/actuator`.
No runtime response bodies are written to disk. Stop only this Java process afterwards.

`JAVA_HOME=<jdk8> CARGO=<cargo-path> python3 verify.py` runs both the open and secured fixture,
chooses unused loopback ports, creates ephemeral credentials only in child-process environments,
verifies real Spring Security 401/403 and a disabled env endpoint (404), and stops its children.
Fixture security is opt-in (`fixture.secured=true`); default security auto-configuration is excluded
so no generated passwords are logged. No real credentials belong in this fixture.


The open fixture also verifies inherited and overridden handler methods, a nested static
controller (including an inherited method), and nested Beans. Boot 2.7.18 reports the
method declaration class for an inherited mapping, dotted canonical names for nested
mapping classes, and dollar binary names for nested Bean types. The verifier asserts the
actual strings. Synthetic config-server/programmatic source names exercise origin-label
redaction; they do not connect to an external configuration service.
