# Spring Boot daily development parity: macOS / Java 8

Assessment date: 2026-11-07. Baseline: personal fork `review/mac-local` at
`4a1de687`, app version 0.1.40. This evaluates the free JDT + official Spring
Tools route. It does not imply that installing IntelliJ's optional language
server provides all IDEA Ultimate features.

## Evidence and priority

| Workflow                                    | Existing OpenIDEA evidence                                                                                                | Remaining gap / priority                                                                                                                                      |
| ------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Java completion, imports, run, tests, debug | JDT integration and native Maven/test/debug operations; prior Java 8 acceptance in `java-framework-services.md`           | Library decompilation, rename/refactor breadth and complex multi-module imports need separate acceptance. P1                                                  |
| Boot properties and YAML                    | Official Spring Tools 2.3.0, JDT Java/classpath bridge, project metadata completion and definition already verified       | Active profile precedence, external config imports, placeholder chains and version-specific fixes need end-to-end coverage. P1                                |
| Bean injection                              | Bundled Spring Beans plugin: constructors, fields, setters, qualifiers, primary, collections and Mapper beans             | **Reproduced wrong navigation:** a primary `Store<Integer>` is selected for `Store<String>`. Concrete Java generics are this change's P0                      |
| Application context                         | Source-index Bean list/graph, condition/profile guards for diagnostics                                                    | Not a full runtime context: library/auto-config Beans, scan boundaries, conditions, programmatic registrations. Runtime context inspection is P1              |
| Endpoints / HTTP                            | `src/features/http-client/spring`: route index, request generation, source navigation, body skeletons, host configuration | Composed/inherited mappings, runtime mappings and profile-aware host selection need comparison. No need to duplicate the existing static endpoint UI. P1      |
| Boot lifecycle                              | Existing Spring Boot run configurations, Java SDK/Maven settings                                                          | Profile/config editing and build/devtools update policies are less integrated than IDEA. P2                                                                   |
| Data / persistence                          | MyBatis Mapper/XML navigation, statement generation and SQL completion already bundled                                    | Database-backed schema insight, result maps and cross-file refactoring need separate work. JPA/Spring Data semantic tooling is not equivalent to Ultimate. P2 |
| Project creation                            | Existing project/import workflow                                                                                          | IDEA's Initializr/add-starters workflow is a useful later addition, lower value than fixing incorrect navigation in an existing application. P2               |

The official current IDEA help describes runtime Bean/Health/Mappings/Environment
views, starter management and application update policies. Official Spring Tools
also supports live information using Actuator/JMX and client-specific commands.
A standard LSP bridge alone is not evidence that OpenIDEA implements all of those
client integrations. Existing successful checks are recorded as prior evidence,
not silently rerun against the installed application during this task.

## Implemented: concrete Java generic qualifiers

Before this change, the source model erased generic arguments. Both
`TextStore implements Store<String>` and `NumberStore implements Store<Integer>`
matched `Store<String>`, allowing an unrelated primary Bean or parameter name to
win. Generic matching now precedes primary/name selection and is shared by
injection definitions, gutter targets, reverse Bean references, dependency graph
candidates and project-local resolution warnings.

Covered:

- Concrete nested arguments, array arguments and multiple parameter positions.
- Project-defined generic interfaces and multiple superclass levels, substituting
  type variables (`TextStore extends Middle<String>` → `Base<String>` →
  `Store<List<String>>`).
- `@Bean` return signatures, constructor/field/setter/factory parameter injection.
- Collection/map/array element filtering and optional/provider wrappers.
- Explicitly imported and fully qualified argument names; common `java.lang`
  types and `java.util.*` collection names.
- Qualifiers still narrow compatible candidates, then primary/name selection.
- Diagnostic messages retain the requested parameterized type.

Raw signatures, wildcards, unresolved class/method variables, duplicate simple
class declarations and unavailable library hierarchy remain conservative:
unknown candidates stay in the navigation list, are not collapsed by primary or
parameter name, and suppress resolution warnings. Spring's runtime strict/raw
fallback passes may select one Bean where this plugin lists several. Generic
argument subtype bounds, arbitrary wildcard imports, relative nested type names,
Kotlin/XML generic inference and library reflection are not a complete Java type
system here. The existing raw hierarchy still matches by simple name. Component
scan/condition/profile/programmatic registration limitations remain.

## Reproducible validation

`fixtures/java-spring-generics` is a standalone Maven fixture pinned to Boot
2.7.18 / Spring 5.3.31 / Java 8. It uses real application contexts, starts no web
server and needs no management port. Its tests cover primary/name precedence,
collections/maps/arrays/providers, nested inheritance, generic factory methods,
qualifiers, missing/ambiguous Beans and the raw-signature boundary.

```sh
cargo test --manifest-path plugins/official/spring-beans/Cargo.toml
cargo clippy --manifest-path plugins/official/spring-beans/Cargo.toml --all-targets -- -D warnings
plugins/build.sh official/spring-beans
plugins/build.sh --check official/spring-beans
JAVA_HOME=/path/to/jdk8 mvn -f fixtures/java-spring-generics/pom.xml test
```

The same eight real-context tests were also copied into an isolated copy of the
user's existing Boot/MyBatis Java 8 demo. All 13 tests passed, including its five
existing tests. The original demo and port 18088 were untouched. App-level
verification details and final test counts are recorded in the task's delivery
report after runtime acceptance.

## Official references

- [IDEA Spring Boot features](https://www.jetbrains.com/help/idea/spring-boot.html)
- [IDEA Spring Boot run configuration](https://www.jetbrains.com/help/idea/run-debug-configuration-spring-boot.html)
- [Spring Tools capabilities](https://spring.io/tools/)
- [Official Spring Tools client features](https://marketplace.visualstudio.com/items?itemName=vmware.vscode-spring-boot)
- [Spring generic qualifiers](https://docs.spring.io/spring-framework/reference/core/beans/annotation-config/generics-as-qualifiers.html)
- [Spring Boot Actuator endpoints](https://docs.spring.io/spring-boot/reference/actuator/endpoints.html)

## Independent desktop acceptance

A fresh debug build from this clone used its own worktree-derived application
identifier and data directory. An isolated four-file Java project loaded the
baseline checked-in WASM first, then the rebuilt WASM through plugin reload.
The test did not use the installed app or any existing Java process.

- Baseline `Store<String> numberStore` gutter opened `NumberStore.java` whose
  signature is `Store<Integer>` and which is marked primary.
- Rebuilt plugin's same gutter opened `TextStore.java` (`Store<String>`).
- Adding a second string Store produced an `ambiguous-bean` warning containing
  `Store<String>` and exactly the two string Bean names; its gutter had no target.
- Editing the injection to `Store<Long>` produced `missing-bean`, with no gutter.
- Editing back to `Store<String>` with `@Qualifier("textStore")` cleared the warning
  and restored the correct target, proving live document changes update the index.

Final checks: 190 Spring Beans tests (including the copied demo's Mapper check),
39 shared-parser tests, clean plugin clippy/format, rebuilt WASM and matching
source fingerprint, eight standalone Java 8 context tests and 13 tests in the
copied Boot/MyBatis demo. Screenshots and bridge results live in the task's output
report rather than in the source commit. No frontend, host protocol or IPC types
changed, so frontend suite/bindings regeneration was not required for this plugin
change. The debug app build succeeded with a macOS linker compact-unwind size
warning; it did not affect the observed editor checks.
