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

- Up to 4,000 source units, 16,000 types and 12,000 registrations/injection records.
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
Only the Spring plugin needs the new model method; its source manifest now requires `^1.4`. The fork resource rewrite must preserve that floor (the main session owns `prepare-fork-resources.py`). Rebuild the misbehave fixture
and all plugins with `plugins/build.sh` after merging the concurrent runtime
context API changes. The final released snapshot belongs to `release-prepare`.

The JDT bundle advances from 1.9.0 to 1.10.0, including its installed filename, so
OSGi cannot keep an old cached command implementation. Other sessions modifying
this same bundle should combine sources and choose one final bundle version, then
rebuild it once. No installed bundle has been replaced here.

## Validation (updated as checks finish)

- Java 8 container oracle: passed, Spring Boot 2.7.18 / Framework 5.3.31; no web application.
- Real m2e parent + two-module import: passed; cross-module `Port`, `Token`,
  `ModuleValue`, dependency factory alias, scanned binary component and both
  metadata formats resolved. 48 registrations/candidates, explicitly incomplete
  because real Boot includes dynamic and unresolved conditions.
- Java static initializer tripwire: passed; JDT process uses `-Dopenidea.model.only=true`.
- JDT binary/decompiler navigation and attached source-JAR range: passed.
- Unsaved import edits and restoration: passed.
- Bean plugin: 199 tests passed before the final binary graph identity case.
- SDK: 10 unit + 3 frozen API + 2 WIT coverage tests passed; released snapshot unchanged.
- Frontend: gutter/catalog targeted tests passed (24 passed, 6 existing skips); TypeScript passed.
- `cargo check -p ide-lsp --lib --offline -j 2`: passed before final lifecycle/refresh changes.
- WIT native compatibility/runtime tests and final reruns: in progress.

Evidence stays under this chat's `work/java8-oracle` (container log, model JSON,
source-attached model, JSON-RPC transcript). Full native app build, live macOS UI,
source attachment UI flow, actual marker/tree refresh and screenshots are **pending
unified acceptance**, not claimed as executed here. Suggested acceptance: open the
oracle Maven parent, inspect App injection targets and JAR factory names in both
Spring views, toggle imports, then close the project while discovery is pending.

No changes were made to MVC routes, `ops/routes.rs` or `commands/nav.rs`.
