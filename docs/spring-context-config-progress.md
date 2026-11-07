# Spring context and configuration work

This checkout owns Bean conditions/scanning/generics and configuration
sources/placeholders/Java navigation after the user-directed six-chat split.
Runtime Actuator UI, JAR/dependency indexing, cross-file refactoring, MVC route
parsing and run/debug improvements belong to the other chats. This branch avoids
`src/features/http-client/spring/`, `crates/ide-host/src/ops/routes.rs`,
`fixtures/java-spring-mappings` and the main chat's `annotation_checks.rs`.

## Commits

- `8f1f543cbd4350ee544265baaa9a61b2c0c5bb50`: concrete Java generic qualifiers.
- Merged fork main `ad80b6c88b057313bc41ea269092739bef884cd3` into the independent branch.
- `57653df63a91789a3544436109ee288b7cfb0ba6`: source eligibility, native provider
  project context, and refresh after project plugin settings change. The parent
  already integrated this stage in its review branch.
- `5a157dd2586a0721748cb74b5cb6f80e4899f722`: configuration sources, placeholders, Java links,
  first-party launch-reference consumer, plain/property-class registrations and
  source dependency refresh. This is source-only; the parent will rebuild on SDK 1.4.

- `ab269600`: integration handoff documentation. A final source correction normalizes
  indexed environment names before nested fields, with matching Java 8 coverage.

## Earlier verified work

Generic baseline: `Store<String>` incorrectly selected a primary `Store<Integer>`.
The fix shares generic candidates across navigation, graph, reverse references and
resolution warnings. Earlier acceptance: 190 plugin tests, 39 parser tests,
eight real generic-context tests and 13 tests in a copied Java 8/MyBatis demo.
Actual before/after gutter clicks were captured in this task's outputs.

Context baseline: with dev selected, source navigation chose a prod Bean or a
Bean outside the application's scan package. Profiles, property conditions,
scan bases, imported configuration and definite user-Bean backoff now share a
three-state eligibility model. Native providers used global settings while views
used project settings; provider invocation context now uses the owning project.
Editor gutter and visible plugin views refresh on effective plugin setting changes,
and pending old marker results are discarded.

Stage 1 acceptance: 195 plugin regressions; four real Boot 2.7.18 / Java 8
contexts; 22 native plugin runtime/compatibility tests; four marker tests;
three Settings E2E tests in installed Chrome; full frontend suite (7239 initial
passes, three sandbox-denied socket tests subsequently passing in the five-test
socket recheck, six skips); typecheck, build, formatting, knip, plugin/runtime
clippy. Existing VCS hook warnings remain unrelated. Actual Mac screenshots and
bridge records cover dev→prod switching without edits, Bean filtering and correct
gutter targets. Evidence is in the task's `outputs/stage1-context/`.

## Configuration implementation

The source parser reuses `spring-text::config`. Spring Tools retains completion
and validation. Added definitions cover Java annotation placeholders, bound Java
fields back to declarations, and placeholders/nested defaults in YAML/properties.
Source-only configuration-key→Java-field fallback is opt-in through
`springBeans.configurationBindingFallback`; the default avoids competing with the
official server's route. Nested project property classes, empty prefixes and
factory-method property groups are supported. Nonliteral prefixes are skipped.
All locations use zero-based UTF-16 coordinates, including CRLF/Unicode cases.

Spring Configuration Sources lists source origins, selected/overridden candidates,
placeholder chains and Java binding links. Its filter is project-scoped. Values
are hidden, including values recovered from saved launches. Saves and filesystem
changes coalesce source-dependent gutter/view refreshes without idle polling;
other projects and generated build output are ignored.

The bounded Boot 2.7 source model supports:

- Default classpath root/config and external root/config/wildcard directory groups;
  profile order is preserved, properties beat YAML at one location, and external
  config groups override packaged config groups.
- Multi-document activation/order and the default profile. Profile includes
  accumulate per property source; group lists follow effective source precedence,
  expand recursively in declaration order and stop cycles.
- Project-local classpath, explicit file and importer-relative imports; optional
  misses, required misses, repeated/load-once cycles and imported profile variants.
  Imported profile variants are promoted ahead of ordinary profile files within
  their location group. Ordinary imports stay beneath later enclosing documents.
- Environment-file literals, explicit environment, JVM system properties,
  application JSON, CLI properties and manual analysis overrides, in that order.
  Unknown present facts remain overrides and block lower-priority fallback.
- Nested placeholder defaults, bounded chains and cycle detection. Unavailable
  runtime facts do not turn absent source keys into known absence.

Java 8 acceptance exposed the imported-profile precedence rule; the source loader
was corrected to agree with Boot rather than retaining the original unit-test
assumption. Real tests also distinguish optional missing files from existing files
that themselves contain a required missing import, and assert the actual exception
for invalid profile-specific includes.

Plain imported classes now materialize as Beans with qualified import names and
constructor dependencies. `@EnableConfigurationProperties` and project
`@ConfigurationPropertiesScan` registrations use Boot's property Bean name
convention. A plain class discovered as source by a scan does not execute its
imports unless it is actually registered. Explicit property registration follows
the enabling configuration; the property class's own `@Profile` is not applied by
this registrar, as confirmed in Java 8. Missing-Bean backoff now verifies qualified
Java assignability instead of treating equal simple names as proof.

## Shared Run protocol and integration boundary

The sole shared contract writer is run/debug chat F. Its checked implementation
was at `915ff7bfca786d6c6af46cceca1c41e008184f3a`. This chat reads:

`/Users/hyl/Documents/Codex/2026-11-06/https-github-com-anomalyco-opencode-https-2/work/spring-parity-coordination/launch-context-contract.json`

Protocol: `openidea.spring-launch-context`, version 1. A public synthetic snapshot
is included in `fixtures/java-spring-configuration/launch-context-contract.json`.
The contract and committed snapshot have SHA-256
`3a5e0d26690ee5a64f17619b97cdf4edd272137d3bba9d15a175758a7f0f375e`.
The consumer runs the same producer expectations through its source resolver:
dev→prod, CLI precedence, saved nonboolean recovery, deselection, unknown high
priority and the stable `hyhanyue.spring-beans` alias. The native bridge/producer
remain F's changes and must be integrated with this consumer before Mac acceptance.

The host exposes a separate invocation-scoped JSON object under
`springBeans.launchContext`, using its first-party `openidea.setLaunchContext`
command. No new persistent setting/schema field is introduced. The earlier
experimental JSON-in-propertyOverrides envelope was removed. The existing manual
`springBeans.propertyOverrides` string remains user-owned and unchanged.

Only `persisted:true` references are recovered. The consumer reads only the current
project's `.openidea/run.toml` and `.openidea/run.local.toml`, matches the exact
selected configuration ID and checks kind/main/module/cwd, inherited-environment
flag, env-file paths, property key sets and known facts. The shared file wins a
duplicate ID. Mismatches/unreadable/unsaved references retain redacted unknown
facts; another configuration is never chosen by name or last-run status. Values
are parsed in memory and never written, logged or shown. The actual process
environment is never enumerated; no third-party permission boundary is expanded.

`Fact{state:unknown}`, malformed/null facts and unknown selection are not manual
`!key` tombstones. They cannot be interpreted as false or known absence. Missing,
null or changed launch context participates in the cache signature so previous
selection facts cannot survive deselection/conflict.

## Targeted verification

Passed: **224** plugin tests, **40** shared parser/index tests, **8** real
Boot 2.7.18 / Java 8 tests, and **14** focused frontend tests. Plugin clippy
(`-D warnings`), formatting, full frontend typecheck, frontend build, format check
and knip pass. Lint completes with warnings only in pre-existing VCS hooks and
incompatible-library call sites; none are in the changed source files. Build emits
existing mixed static/dynamic import warnings. Knip has one existing ignore hint.

The parent has integrated SDK 1.4 and explicitly requested no old-SDK WASM rebuild
here. Consequently checked-in WASM/fingerprint artifacts were deliberately left
unchanged; **source-to-WASM freshness is pending the parent's unified SDK 1.4 build**.
No stage 2 GUI acceptance, full native rebuild or combined Run/WASM acceptance is
claimed by these tests. Evidence is copied to `outputs/stage2-configuration/` in
the task workspace. Earlier full frontend and native checks belong to stage 1.

Reusable API for the parent dependency model:

```rust
environment::snapshot_project(project, files, current).application
```

`files` is the current project `BTreeMap<String, FileModel>`; `current` is an optional
unsaved config `(path, text)`. The public `Snapshot.application` uses the manual
analysis application override, then the selected transient launch's main class,
then the only indexed Boot application. It does not read persistent
`springBeans.applicationClass` alone. This module does not load dependency metadata,
so callers can use the same source environment without recursing into JAR indexing.
Call `environment::invalidate()` when the host signals changed context/sources.
F's host snapshots retain window/project isolation; conflicting windows yield
unknown selection. The parent's merge of `settingsRevision.ts` must keep both this
branch's debounced source invalidation and F's launch-context invalidation.

Commands (using the checkout's isolated Rust toolchain and JDK 8):

```sh
cargo test --offline --manifest-path plugins/official/spring-beans/Cargo.toml
cargo test --offline --manifest-path plugins/support/spring-text/Cargo.toml
cargo clippy --offline --manifest-path plugins/official/spring-beans/Cargo.toml --all-targets -- -D warnings
cargo fmt --manifest-path plugins/official/spring-beans/Cargo.toml --check
JAVA_HOME=/Library/Java/JavaVirtualMachines/jdk-1.8.jdk/Contents/Home mvn -o -f fixtures/java-spring-configuration/pom.xml test
pnpm vitest run src/features/plugins/settingsRevision.test.ts src/features/plugins/lineMarkerGutter.test.ts src/features/plugins/navigation.test.ts src/features/plugins/markerStore.test.ts
pnpm typecheck
pnpm lint
pnpm format:check
plugins/build.sh official/spring-beans
plugins/build.sh --check official/spring-beans
```

## Unified Mac acceptance still required

The independent native app was stopped after the split to avoid concurrent heavy
builds. No further native build or app installation is part of this pass. Once F's
host/producer and this consumer are integrated, the parent can run:

```sh
pnpm dev:app
pnpm dev:status
```

Use the reported worktree identifier/port and verify the active project, then:

1. Copy `fixtures/java-spring-configuration` into a scratch project and select JDK 8.
2. Save two first-party Spring Boot launches for `dev.openidea.configuration.App`,
   module `""`, cwd `"."`, with inheritance off; set profiles dev/prod, then a CLI
   override. Select each exact ID and inspect the source origins and Bean candidates.
3. Open Configuration Sources; verify the selected launch/profile, overridden
   declarations, import-cycle status, placeholder-cycle uncertainty and hidden values.
4. Click origins and Java bindings. Test definitions in `@Value`, the bound field,
   YAML/properties placeholders and nested defaults. Confirm UTF-16 positions.
5. Edit/save a property or selected launch; verify view, diagnostics and gutter
   update together. Deselect; verify old facts disappear. Test an unsaved redacted
   override and conflicting windows; neither may reuse the saved value.
6. Keep the default Spring Tools key definition route; enable the source fallback
   explicitly for its separate key→field check. Capture before/after screenshots at
   the same window size, using public fixture values only.

## Limits

This is a selected source environment, not the running Spring container. Dynamic
conditions/selectors/registrars, custom scan filters and unsupported ordering stay
uncertain. Full classpath/auto-configuration knowledge belongs to the dependency
chat. Kotlin/XML generic inference and full Java compilation/binding semantics are
not claimed. Ambiguous derived registrations remain conservative.

Remote/config-tree providers, paths outside the authorized project, actual inherited
process state, implicit JVM option variables, programmatic profile changes, complex
YAML/escaped or continued properties, and computed env-file values are not guessed.
Local simple env-file values are supported; unavailable/complex entries remain
unknown at the appropriate layer. Boot 2.7 properties use ISO-8859-1; non-ASCII or
escaped values outside this parser's supported subset remain unknown. YAML is UTF-8; booleans/null and canonical numeric literals are handled, while
ambiguous typed scalar spellings remain unknown.
Overlapping imports of an already loaded default location at a different precedence
are explicitly uncertain rather than assigned an unverified source order.

Analysis bounds: 128 config files, 1 MiB/file and 8 MiB total config text, 8192
source entries, import depth 8, placeholder depth 32, 64 expanded profiles and 500
visible keys. Large derived Bean registrations are materialized in bounded batches.
These limits produce visible uncertainty/truncation instead of silent certainty.

The original demo, port 18088, installed app, primary checkout, main, tags and
release state remain untouched. No pushes or releases were made.
