# Java / Spring services and IDEA feature parity

This personal fork targets Java 8, Spring Boot 2.7, MyBatis and macOS first. The
goal is to make those daily workflows comparable to IDEA through reproducible
checks. This is not a claim of complete Ultimate compatibility.

## Two optional engines

| Route       | Java insight                      | Spring insight                                              | Status                                                                             |
| ----------- | --------------------------------- | ----------------------------------------------------------- | ---------------------------------------------------------------------------------- |
| Open source | Eclipse JDT LS                    | Official Spring Tools language server, plus OpenIDEA panels | Native bridge and real-service tests implemented                                   |
| IntelliJ    | Official IntelliJ Language Server | Whatever its installed bundle exposes through LSP           | Protocol and installed-editor completion, acceptance and project navigation tested |

The routes are alternatives for Java completion/diagnostics. The IntelliJ preset
retains JDT LS for OpenIDEA's existing build/run/debug commands and code lenses,
but excludes its completion, hover and diagnostics. Kotlin is not added by this
preset until its existing server and debugger integrations have been evaluated.

IntelliJ's custom protocol extensions and run lenses are not advertised: OpenIDEA
does not yet implement their pickers, clipboard actions or IntelliJ DAP launch
commands. Existing OpenIDEA/JDT run and debug actions remain the supported path.
Standard LSP support does not guarantee every IntelliJ quick fix or library
decompilation feature works.

## Implemented Spring bridge

`ide-lsp/src/spring.rs` handles Spring Tools' documented Java model requests:

- Classpath listener registration/removal and callbacks from JDT LS.
- Type/location/Javadoc lookup, type/package search, hierarchy, completion and GAV.
- Spring-specific document language IDs for properties/YAML, without changing
  what the other servers receive.
- Listener ownership scoped to the Java server instance generation. Arbitrary
  Java workspace commands are not forwarded.

The bridge is activated only for a configured server named `spring-boot`. It uses
the existing JDT instance; it does not run a second Java language server or use
the deprecated Maven/classpath fallback in Spring Tools.

Additional JDT bundles now retain the managed OpenIDEA usages/debugger bundles,
instead of replacing them. Other servers keep the prior array override behavior.

## Preparing a profile

Requires Python 3.11+ on macOS. The helpers generate a settings fragment and a
launch specification in a chosen directory. They **do not edit project settings**
or download software. Both output paths must be new, avoiding silent overwrite.

```sh
python3 scripts/java-services/prepare.py --engine spring \
  --project /path/to/project --project-jdk /path/to/jdk8 \
  --spring /path/to/extracted-vsix/extension --java-home /path/to/ls-jdk \
  --output /path/to/spring-profile

python3 scripts/java-services/prepare.py --engine intellij \
  --project /path/to/project --project-jdk /path/to/jdk8 \
  --server /path/to/intellij-server/bin/intellij-server \
  --java-home /path/to/import-jdk --maven-home /path/to/maven \
  --output /path/to/intellij-profile
```

The IntelliJ profile remains disabled until the user reads and accepts that
bundle's `EULA.txt`. Only then regenerate with `--accept-eula`. The launcher checks
the exact license content hash and supplies the server's documented acceptance
argument. No license key or subscription is bundled. The server can require a
trial or a separately activated Ultimate subscription.

Merge the selected fragment through the existing `languageServers` project
settings, retaining other Java/Maven settings. Do not apply both profiles at once.
The Spring profile requires an app containing the new native bridge; merely
adding its settings to the older 0.1.39 app does not install that bridge. When
using official Spring completion, disable the community Spring Boot Tools
completion provider to avoid duplicate suggestions. Panels can be evaluated
independently.

The IntelliJ launcher uses locked per-project index slots so simultaneous clients
do not share a RocksDB lock. It reads the configured Maven home from project
settings when omitted, and puts its `bin` on PATH for the Maven importer. The
import runtime is independent of the Java 8 project SDK. Usage statistics use the
official server's default of none. Consent-related inherited variables and the
stdout launcher-debug switch are cleared.

## Verification performed on this Mac

Baseline project: `spring-mybatis-java8-demo`, Boot 2.7.18, MyBatis Starter 2.3.2,
H2, project SDK 1.8.0_421. Tooling uses an independent managed JDK 25.0.5.

| Check                                                          | Evidence                                                                                       |
| -------------------------------------------------------------- | ---------------------------------------------------------------------------------------------- |
| Spring `server.po` → `server.port`                             | Real official Spring Tools 2.3.0 + JDT LS, Python protocol probe and native OpenIDEA manager   |
| Spring `demo.gre` → `demo.greeting`                            | Reads the actual project's generated configuration metadata, not a hardcoded key list          |
| Spring configuration definition                                | Native bridge follows `demo.greeting` to `DemoProperties.java`                                 |
| IntelliJ `userMapper.` → `findById`                            | Official ILS-263.7084.0, valid user-authorized trial, Java 8 SDK                               |
| IntelliJ definition → `UserMapper.java`                        | Real server returns the method's source location                                               |
| Installed OpenIDEA: `userMapper.f`                             | Candidate appears, accepting produces `findById()` once, entering `id` restores the valid call |
| Installed OpenIDEA Run                                         | Starts with Java 1.8.0_421; HTTP checks return Alice and Bob through MyBatis                   |
| Classpath callback registration/removal and request forwarding | In-memory JSON-RPC integration tests                                                           |
| Additional bundles preserve usages/debugger support            | SDK regression test                                                                            |

The official IntelliJ Apple Silicon archive was checked against JetBrains'
published SHA-256 before use. Downloaded binaries, licenses, machine-specific
profiles, activation state and caches are not committed or redistributed.

The fresh-prefix editor check exposed a real interoperability issue: IntelliJ
returns `filterText: ".findById"` and an edit replacing `.f`. The old lookup's
word matching hides those candidates. The frontend now normalizes that matching
text without changing the server's edit or snippet. The launcher applies the
same narrow compatibility normalization for older installed app builds. A
CodeMirror regression checks visibility and acceptance with exactly one dot, and
framing tests ensure the compatibility adapter preserves unrelated messages.

Reproduce the native Spring checks by setting `OPENIDEA_SPRING_TEST_PROJECT`,
`OPENIDEA_SPRING_HOME` (VSIX `extension` directory), `OPENIDEA_JDT_HOME`,
`OPENIDEA_LS_JAVA_HOME` and `OPENIDEA_PROJECT_JAVA_HOME`:

```sh
cargo test -p ide-lsp --test real_servers \
  spring_tools_uses_java8_project_metadata -- --ignored --nocapture
cargo test -p ide-lsp
cargo test -p ide-sdk jdtls_additional_bundles
```

`probe.py` checks Spring independently of the native app. `probe_intellij.py`
checks an approved IntelliJ launch specification, completion and definition. Its
`--start-trial` switch must only be used with explicit user authorization. Neither
probe edits source files. Imports may create IDE metadata or populate tool caches.

## Spring Bean injection milestone

The personal fork vendors the MIT Spring Beans Navigator and its `spring-text`
source helpers (fixed upstream commit in `plugins/official/spring-beans/UPSTREAM.md`).
It adds conservative Java project-local missing/ambiguous injection warnings,
recognizes MyBatis `@Mapper` beans, filters qualifiers before selecting primary
beans, and does not navigate to an arbitrary first bean when candidates conflict.
A single candidate has a Spring injection gutter target; Go to Definition can
list multiple candidates. Optional, provider and collection injection, library
types, and profile/condition-dependent cases are excluded from these warnings.
`springBeans.checkInjectionResolution` controls this new check.

This source scanner is not a complete Spring context evaluator: generic type
arguments, component-scan boundaries, programmatic registration and conditional
auto-configuration need further work. It reports project-index warnings, not
proof that a runtime application will fail.

The Mac setup keeps IntelliJ for Java and JDT for run/debug plus Spring's Java
model bridge. The Spring profile's JDT bundles and `spring-boot` section may be
merged into that setup while retaining IntelliJ's Java profile; do not overwrite
its existing Java SDK, Maven or JDT feature settings. Disable the replaced
community Spring Boot configuration provider to avoid mixed metadata versions.

The Java 8 demo includes isolated `src/test/java/dev/openidea/beanchecks` examples
for missing beans, ambiguous injection and primary/qualifier navigation. They
are outside the running application's component scan. Three real-context tests
check these outcomes, alongside its two existing application/MyBatis tests.

The source includes independent Spring gutter assets. The user's local preview
uses original Spring icons from their installed IDEA; those commercial resources
remain outside this source tree and are not included in its license.

## Remaining acceptance work, in order

1. Native bridge packaging, configuration completion acceptance, Bean diagnostics
   and gutter navigation were verified in the installed Mac app on 2026-11-07.
   Continue with cancellation and switching-engine regressions.
2. Spring injection: constructor injection, qualifiers, primary beans, multiple
   candidates, profiles and multi-module navigation. Compare actual targets and
   diagnostics, not only the number of beans in a panel.
3. Configuration: YAML and properties profiles, placeholder resolution, custom
   configuration classes, version-specific deprecated/unknown keys and quick fixes.
4. Java: rename across files/modules, extract method/variable, imports, Lombok and
   annotation processing, source JARs and library decompilation.
5. MyBatis: method/XML generation and SQL completion regressions, resultMap,
   foreach/bind/include, cross-file rename, and database-backed schema metadata.
6. Maven/tests/debugger: multi-module import, dependency changes, selective JUnit
   runs, conditional breakpoints, expression evaluation and rerun behavior.
7. Mac interaction: keymaps, caret/selection behavior, navigation history, tool
   windows and completion latency. Verify with the user's project before moving
   to larger business projects.

## Reference implementations

- [IntelliJ's unified product and open-source builds](https://www.jetbrains.com/help/idea/intellij-idea-single-distribution.html).
  A free feature in the unified binary is not itself evidence that its source is
  available for reuse. Check the selected source files and components individually.
- [IntelliJ Community Java sources](https://github.com/JetBrains/intellij-community/tree/master/java):
  useful references for analysis, debugger and editor behavior, but dependent on
  the IntelliJ platform rather than directly loadable into a Rust/Tauri app.
- [Spring Tools client integration guide](https://github.com/spring-projects/spring-tools/wiki/Developer-Manual-Integrate-Language-Server-Into-Client)
  and its public classpath/Java data protocol. This adapter was independently written.
- [JetBrains' official LSP client](https://github.com/JetBrains/intellij-lsp.nvim):
  reference for stdio startup, index isolation and public licensing requests. Its
  client license is separate from the downloaded language-server product.
- [MyBatisX](https://github.com/baomidou/MybatisX) and
  [MapStruct IDEA](https://github.com/mapstruct/mapstruct-idea): references for Java
  framework workflows. They require adaptation to OpenIDEA's SDK; no IntelliJ
  plugin binary compatibility is claimed.

## Installed Mac acceptance (2026-11-07)

The rebuilt application replaces `/Applications/OpenIDEA.app`; the previous app
was not retained. The local Spring Beans and MyBatisX icon packages are linked
under their personal plugin IDs, with only project reading and file events
granted. Replaced marketplace Spring providers are disabled to avoid duplicates.

- Missing project Bean: constructor parameter underline and Problems warning.
- Ambiguous Beans: both candidates named in the warning. Adding a qualifier
  clears the warning; clicking its injection gutter navigates to EmailChannel.
- Default injection navigates to the `@Primary` factory method; the explicit
  qualifier navigates to the backup factory method.
- The existing UserService Mapper injection navigates to UserMapper without
  a missing-Bean warning; Mapper tabs display the MyBatisX bird.
- Official Spring configuration completion is available in the installed app.
  A real acceptance check exposed retained fallback commands after a resolved
  completion replaces its edit. The stdio helper explicitly clears that removed
  command, preserving unresolved fallbacks and exact edits. Accepting `server.po`
  now yields `server.port=` once. The configuration was restored afterward.
- DemoApplication runs on Java 1.8.0_421 / Spring Boot 2.7.18, port 18088.
- Five Maven tests pass, including the three real Spring injection checks;
  179 plugin tests, 39 shared-parser tests, 76 focused frontend tests and five
  launcher checks pass.

Inspection demo files remain under `src/test/java/dev/openidea/beanchecks`, outside
the main application component scan. Missing and ambiguous examples intentionally
retain their warnings. Original application sources and properties are restored.
