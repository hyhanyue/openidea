# Java8 annotated Spring mapping contracts

The fixture uses Boot 2.7.18 dependency management (Spring 5.3.31), JDK8 and no listening server. It reads the real MVC and WebFlux `RequestMappingHandlerMapping` registries; MVC cases also exercise MockMvc request dispatch. Negative cases assert invalid aliases prevent handler registration.

Regenerate the checked-in evidence from the repository root:

```sh
JAVA_HOME=/Library/Java/JavaVirtualMachines/jdk-1.8.jdk/Contents/Home \
  '/Applications/IntelliJ IDEA.app/Contents/plugins/maven/lib/maven3/bin/mvn' \
  -f fixtures/java-spring-mappings/pom.xml test \
  -Dmapping.evidence="$PWD/fixtures/java-spring-mappings/ground-truth.json"
node_modules/.bin/vitest run src/features/http-client/spring/groundTruth.test.ts
```

Add `-Dmaven.repo.local=/your/isolated/cache` for a separate dependency cache and `-o` after dependencies have been cached. The Maven `target` stays under this fixture. No application, external port or original demo project is needed.

The TypeScript contract test compares every static handler (including all methods/paths and media conditions) to both stacks. Request parameter inheritance, chosen variants and body generation are additionally covered by Spring module tests.

Scope: Java/Kotlin annotated controllers with source-visible metadata. Functional RouterFunction DSLs, programmatic registrations, dependency JAR metadata, runtime property/SpEL paths, application bean activation/conditional registration and runtime proxies require runtime evidence. Unresolved path expressions are omitted, never replaced with a root route; omitted results do not prove a server has no endpoint.
