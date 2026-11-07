# Spring dependency Bean model — integration progress

Branch: `spring-dependency-model`; baseline: `57653df63a91789a3544436109ee288b7cfb0ba6`.
Independent clone: `/Users/hyl/Documents/Codex/2026-11-07/openidea-spring-dependency-beans/work/openidea`.
The original demo, main checkout, installed application and port 18088 were not modified.

## Scope and evidence

The existing `aritti.spring-beans` Navigator indexes project text. It does not have
classpath or archive access. `crates/ide-lsp/src/spring.rs` bridges Spring Tools
requests to the existing JDT LS, including its classpath listeners; it does not
supply that resolved model to the Navigator. `ConfigurationCommand` already uses
`IJavaProject.getAllPackageFragmentRoots()` for property navigation. This change
uses the same JDT authority rather than parsing arbitrary POM paths or walking `.m2`.

Reviewed primary documentation:

- [JDT IJavaProject](https://help.eclipse.org/latest/topic/org.eclipse.jdt.doc.isv/reference/api/org/eclipse/jdt/core/IJavaProject.html): referenced project source roots are obtained through the resolved Java model.
- [Boot 2.7.18 auto-configuration](https://docs.spring.io/spring-boot/docs/2.7.18/reference/html/features.html#features.developing-auto-configuration): metadata lists candidate configurations; conditions still control registration.

## Implementation and integration contract

`openidea.java.spring.beans` takes `[sourceUri, applicationClass]` and returns a
bounded, declarative JSON snapshot: `beans`, `injections`, `root`, `incomplete`,
`revision`. Each registration carries full type identity and supertypes, source
location, module/dependency origin, names/aliases/qualifiers, factory parameter
types, generic uncertainty and `active`/`unknown` eligibility. Its Java 21 bundle
runs inside JDT LS; it reads Java 8 application class files without initializing
application classes.

Source roots come from JDT/m2e, including referenced reactor modules; production
models skip Maven test roots. Ordinary dependency components are considered only
inside a known scan or explicit import. Boot metadata supports both the 2.7
`AutoConfiguration.imports` file and legacy `spring.factories`. Direct imports
are not obscured by identical meta-annotations. Registered configurations expose
factory methods, including inherited methods. Import selectors/registrars are
unknown rather than application Beans. Missing-class conditions are excluded;
property, profile, bean-order and other runtime conditions remain unknown.

The new WIT 1.4 `project.spring-beans(path, context)` returns cached JSON or `none`
while background work runs. It exposes neither archive paths to read nor command
execution. The native host requires a trusted open project and a canonical source
path inside its existing read boundary. Only exact `jdt:` symbols already returned
by that project's model may pass the existing view-navigation boundary. Filesystem
reads outside the project remain refused. Lifecycle hooks cancel and discard
snapshots on source/build/watch/restart/trust/project-close changes. Java snapshots
also compare build roots, archive stamps, source attachments and working copies.

`dependencies.rs` is a thin Navigator adapter. Local `context.rs` and its source
configuration/environment evaluation remain authoritative; the adapter does not
rewrite them. The shared World candidate path powers definition, gutters, tree,
completion and graph. Binary injection matching uses JDT-qualified identities,
not a simple-name guess. Unknown/pending build models suppress missing-library
warnings. Source configuration work can extend the JSON model with structured
condition facts; no additional file capability is necessary. The current binary
property/profile conditions deliberately stay unknown even when source launch
properties are selected.

Background completion emits the existing code-lens refresh event. Plugin gutters
and the Spring views re-request their model; no UI thread does classpath discovery.
Source attachments use JDT navigation; no-source libraries reuse the existing
read-only decompiler. Class-file fallback ranges may be zero; graph/tree keys also
include Bean names so different factories never collapse into one node.

## Bounds and lifecycle

- Up to 4,000 source units / 16 Mi UTF-16 source characters, 16,000 types and 12,000 registrations/injection records.
- Metadata/class entries: 1 MiB; archive: 256 MiB / 100,000 entries; metadata archives: 512.
- Requested binary content: 64 MiB aggregate; class nesting 24 / path depth 32.
- Annotation expansion: depth 8 / 128 annotations; hierarchy expansion: depth 24 / 256 types.
- No extraction, nested ZIP traversal, class loading or static initialization.
- Cancellable Java lock and traversal; 90-second host deadline; eight per-project
  host snapshots and eight Java snapshots. Project close/untrust cancels pending
  requests and revokes returned navigation targets. Bundle stop removes its listener.

## API/build coordination

WIT and SDK copies move from released 1.3 to **1.4**. `ENGINE_VERSION` and both
`API_VERSION` constants agree. The released SDK snapshot is untouched. The old
component compatibility assertions remain intact; the main session should rebuild
all packaged components at final integration, because the existing
`shipped_components_are_built_for_this_api` check requires current API binaries.
Only the Spring plugin needs the new model method; its source manifest now requires `^1.4`. The fork resource rewrite must preserve that floor (the main session owns `prepare-fork-resources.py`). Rebuild the current-API fixtures (`hello`, `indexer`, `semantic-tokens`, `misbehave`)
and all official plugins with `plugins/build.sh` after merging the concurrent runtime
context API changes. The final released snapshot belongs to `release-prepare`.

Every official plugin and each current-API fixture is fingerprinted against the
whole SDK source and WIT tree, so all those `.wasm.sources` files become stale,
including plugins whose own Rust sources did not change. Only Spring was rebuilt
here; do not run full `plugins/build.sh --check` until the unified rebuild. The
main session's new `--fingerprint` option can inspect hashes without installing
targets. **Preserve** `sdk/rust/openidea-plugin/tests/api/released/` as released
1.3 until final release preparation, and preserve the explicit
`openidea:plugin/ui@1.3.0` component in
`released_13_component_calls_an_existing_interface_on_14`: it is an inline WAT
fixture, intentionally independent of the current SDK and never rewritten to
1.4 by a bulk version replacement. It proves a real old-interface function call
on a 1.4 linker even after every current-API fixture is rebuilt. The initial
compatibility run also linked the pre-rebuild 1.3 hello component against the actual 1.4 host.

The JDT bundle advances from 1.9.0 to 1.10.0 (SHA-256 `c3af47b7db933ce8c1e26f1a67caf07a86f52058921f87b37b32cbfad8401d63`), including its installed filename, so
OSGi cannot keep an old cached command implementation. Other sessions modifying
this same bundle should combine sources and choose one final bundle version, then
rebuild it once. No installed bundle has been replaced here.

## Validation (updated as checks finish)

- Offline Java 8 Maven reactor package: passed using an isolated dependency cache.
- Java 8 container oracle: passed, Spring Boot 2.7.18 / Framework 5.3.31; no web application.
- Real m2e parent + two-module import: passed; cross-module `Port`, `Token`,
  `ModuleValue`, dependency factory alias, scanned binary component and both
  metadata formats resolved. 51 registrations/candidates, explicitly incomplete
  because real Boot includes dynamic and unresolved conditions.
- Java static initializer tripwire: passed; JDT process uses `-Dopenidea.model.only=true`.
- JDT binary/decompiler navigation and attached source-JAR range: passed.
- Unsaved import edits and restoration: passed.
- Bean plugin: 202 tests passed, including binary graph identity, primary selection and unresolved generic qualifier cases.
- SDK: 10 unit + 3 frozen API + 2 WIT coverage tests passed; released snapshot unchanged.
- Frontend: gutter/catalog targeted tests passed (25 passed, 6 existing skips); TypeScript passed. The async-model refresh test uses the fork plugin ID.
- `cargo check -p ide-lsp --lib --offline -j 2`: passed with lifecycle/refresh changes.
- LSP cache close/cancellation test: passed; ide-lsp and ide-plugin Clippy passed with warnings denied.
- WIT native compatibility/runtime tests: 9 passed, 1 deliberately filtered check requiring all packaged WASMs to be rebuilt. The existing strict shipped-component assertion is unchanged.
- Source and ID-rewritten Spring manifests both require `^1.4`; the Spring WASM is validated as API 1.4. The complete fork staging script is owned by the main session and its final generated package remains an integration check.
- JDT invalidation includes changed ZIP metadata at the same classpath path and a 1 MiB metadata-limit regression; plain `@EnableAutoConfiguration` is also covered.
- UTF-16 ranges are checked against a source line containing an astral character. The real container includes two unrelated `Token` classes, inherited factories and `@Primary`.

Evidence stays under this chat's `work/java8-oracle` (container log, model JSON,
source-attached model, JSON-RPC transcript). Full native app build, live macOS UI,
source attachment UI flow, actual marker/tree refresh and screenshots are **pending
unified acceptance**, not claimed as executed here. Suggested acceptance: open the
oracle Maven parent, inspect App injection targets and JAR factory names in both
Spring views, toggle imports, then close the project while discovery is pending.

No changes were made to MVC routes, `ops/routes.rs` or `commands/nav.rs`.

Out-of-scope findings: the existing generic PluginView `setValue(initial)` effect
triggers `react/set-state-in-effect` when linting that normally ignored directory.
No unrelated UI behavior was changed to silence it; targeted lint passes with
that one pre-existing rule excluded. Native `ide-host` glue and the fork staging
package still need the main session's unified compile/packaging/UI acceptance.
No FactoryBean runtime product type or dynamic registration is inferred.

Local commits:

1. `2796c44f13b041eba10f13bdbd53cad2dee94ebf` — JDT dependency model and Java 8 oracle.
2. `9cb4a8f38175890268e9d15bc9b4a911bc290a5f` — WIT 1.4/native/SDK/provider integration and extended regressions.
3. The documentation handoff commit follows these two commits.

No main push, release or installed-app replacement was performed.

The main session announced a companion change deriving fork-package API floors
from WIT/SDK and checking both default WASM source fingerprints. That companion
must accompany this branch: this independent clone intentionally retains the old
resource preparer. Both source-package IDs are covered by the manifest and UI
adapter checks, while the final **staged fork package** is still a main-session
integration acceptance item.

Source-context integration point: `dependencies::Snapshot::load` currently reads
`springBeans.applicationClass`, and `World::new` picks that source root (or the
only discovered Boot root). After merging the other session's launch-context API,
feed the same selected application FQCN into both places and into
`Project::spring_beans(path, context)`. No permission/API signature change is
needed. Do not mark binary property/profile conditions active merely because a
source launch is selected; they deliberately remain unknown until structured
condition facts are joined to that shared evaluator.

Reproducible check commands (run from this clone, with the documented Rust cache,
Java 25 and JDT installation on PATH):

```sh
python3 fixtures/java-spring-dependencies/verify.py ../java8-oracle
python3 java/java-usages/build.py /path/to/jdtls
python3 java/java-usages/tests/spring-beans.py /path/to/jdtls ../java8-oracle
cargo test --manifest-path plugins/official/spring-beans/Cargo.toml --offline
cargo test --manifest-path sdk/rust/openidea-plugin/Cargo.toml --offline
cargo test -p ide-lsp --lib jdtls::spring_beans::tests --offline -j 2
cargo test -p ide-plugin --test api_compat --offline -j 2 -- --skip shipped_components_are_built_for_this_api
cargo clippy -p ide-lsp --lib --offline -j 2 -- -D warnings
cargo clippy -p ide-plugin --all-targets --offline -j 2 -- -D warnings
plugins/build.sh official/spring-beans
node node_modules/vitest/vitest.mjs run src/features/plugins/lineMarkerGutter.test.ts src/platform/locales/textCatalog.test.ts
node node_modules/typescript/bin/tsc --noEmit
```

Full `plugins/build.sh --check`, strict shipped-component API checking, native
`ide-host`/desktop integration and live macOS screenshots belong to the unified
rebuild. The individual module Clippy checks, frozen API tests and Spring WASM
build passed here. The final Java bundle hash above was stable across repeated
builds. No second heavy app/native build was started in parallel.

## Integrated launch selection

The integrating checkout now passes `environment::Snapshot.application` directly
into dependency discovery and selects that application's source file to identify
its Maven module. It no longer independently reads the persistent setting or
falls back to the first source module. A conflicting or unavailable application
selection leaves dependency discovery pending instead of borrowing another
module's model. The source-only missing-Bean unit fixtures explicitly include a
selected Boot application and a ready dependency model.

The combined regression first failed with `(first/App.java, "")` while the
second module was selected. It now checks the selected second module, a manual
analysis override selecting the first module, and clearing conflicting window
selection without issuing another discovery request. The SDK mock records
project/path/application requests only for tests; production launch values are
not added to this metadata.
