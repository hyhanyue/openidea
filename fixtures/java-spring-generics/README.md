# Java 8 Spring generic qualifier fixture

Uses Spring Boot 2.7.18 dependency management, Spring Framework 5.3.31 and JUnit 5.
No servlet server, port or external database is needed.

```sh
JAVA_HOME=/path/to/jdk8 mvn -f fixtures/java-spring-generics/pom.xml test
```

Eight real-context tests check generic selection before primary/name matching,
collection/map/array/provider injection, multilevel nested generic substitution,
factory method signatures, explicit qualifiers, missing/ambiguous candidates and
the raw-signature fallback boundary. Expected context failures are asserted by
the tests; their Spring warning logs are intentional.

The source plugin deliberately retains uncertain raw candidates. The runtime
can select a single concrete candidate using its strict/fallback passes. This
fixture records that boundary without claiming complete container equivalence.
