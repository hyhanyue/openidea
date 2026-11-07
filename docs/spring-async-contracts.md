# Spring asynchronous return contracts

The Java Spring inspection now applies a class-level `@Async` to its overridable
instance methods. Private, static and final helper methods are not treated as
intercepted calls solely because the class is annotated. Direct method annotations
retain the existing return inspection and proxyability checks.

Known non-Future return types are checked by their written qualification and
explicit imports, rather than by simple name alone. A foreign or locally declared
`String` is not assumed to be `java.lang.String`. Arrays cannot carry an async
result; an explicitly resolved `java.util.concurrent.CompletionStage` declaration
is also incompatible, unlike a declared `CompletableFuture`. Unresolved custom
return hierarchies remain conservative.

The diagnostic describes an unsupported result contract rather than promising
one version-specific failure. The Java 8 fixture uses Boot 2.7.18 / Spring 5.3.31
and actual asynchronous proxies: unsupported reference results are discarded,
primitive results fail at the proxy boundary, and a declared CompletableFuture
delivers its result from the configured executor. Later frameworks may reject
unsupported declarations differently.

## Verification

- Three new baseline regressions failed before the fix: class-level coverage,
  return-type identity and arrays / explicitly imported CompletionStage.
- All 199 Spring Beans plugin tests pass after the fix.
- `fixtures/java-spring-async` has five real Java 8 proxy tests, all passing.
- Plugin clippy and the rebuilt WASM source fingerprint are checked alongside
  the change. A combined Mac editor acceptance remains required before release.

```sh
cargo test --manifest-path plugins/official/spring-beans/Cargo.toml
cargo clippy --manifest-path plugins/official/spring-beans/Cargo.toml --all-targets -- -D warnings
plugins/build.sh --check official/spring-beans
JAVA_HOME=/path/to/jdk8 mvn -f fixtures/java-spring-async/pom.xml test
```

Reference: [Spring 5.3.31 async execution support](https://github.com/spring-projects/spring-framework/blob/v5.3.31/spring-aop/src/main/java/org/springframework/aop/interceptor/AsyncExecutionAspectSupport.java).
