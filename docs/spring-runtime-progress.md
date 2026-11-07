# Spring Runtime progress

Branch: `feat/spring-runtime`; baseline: `57653df63a91789a3544436109ee288b7cfb0ba6`.
Independent checkout: `/Users/hyl/Documents/Codex/2026-11-07/openidea-spring-runtime/work/openidea`.
Independent Cargo target; no main checkout edits, application launch, release, push, or installed-app changes.

## Official interface assessment

- Existing `plugins/official/spring-beans` contributes source Beans/Graph. Existing `searchEndpoints` workbench service supplies HTTP source endpoints. Both remain intact.
- Existing `crates/ide-lsp/src/spring.rs` bridges Spring Tools Java model callbacks. Existing `lspExecuteCommand` discards command return values, so it cannot supply a typed runtime collection to a panel.
- Standard LSP has hover/codelens and generic executeCommand, not an Actuator collection contract: https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/
- Spring Tools has custom `sts/livedata/get`, `connect`, `disconnect`, `refresh`, and `listConnected` commands, including HTTP support. It is **not JMX-only**. Its get handler exposes beans/mappings/properties/contextPath/port/profiles; configprops and per-endpoint HTTP status are absent from that contract.
- Upstream `SpringProcessLiveDataExtractorOverHttp.retrieveLiveData` starts by reading env and returns null from its outer exception handler if that read fails. It also reads metrics/startup. `HttpActuatorConnection` uses an unconfigured `RestClient.create()` and does not accept this panel's ephemeral HTTP Basic credentials or provide the loopback, redirect, size, or redaction boundary. Reusing it would not meet independent endpoint failures or this task's privacy requirements. No existing live hover functionality is disabled or replaced.
- Source inspected: https://github.com/spring-projects/spring-tools/tree/main/headless-services/spring-boot-language-server/src/main/java/org/springframework/ide/vscode/boot/java/livehover/v2 (`SpringProcessCommandHandler.java`, `HttpActuatorConnection.java`, `SpringProcessLiveDataExtractorOverHttp.java`, `SpringProcessConnectorRemote.java`).
- Boot 2.7.18 response contract: https://docs.spring.io/spring-boot/docs/2.7.18/actuator-api/htmlsingle/

Phase 1 commit: `f23b71002adf5724402aff7aef71cafd7c6cd801`. Phase 2 commit: `b0d89de3f40ad92f371ca555609df8e7a2479316`.

## Phase 1: transport and fixture

Implemented in `ide-http::spring_runtime` plus a thin local Tauri adapter:

- Explicit HTTP/HTTPS loopback URL only; no credentials/query/fragment in URLs, no discovery/automatic attach. Numeric loopback IPv4/IPv6 accepted. localhost is pinned to loopback addresses.
- GET only for beans, conditions, env, configprops, mappings. No proxy, redirects, HTTP history, response logging, configuration writes, or exposure changes. System certificate verification remains enabled.
- 5 second per-endpoint timeout, 8 MiB decoded response cap, 20,000 row cap. Cancellation interrupts I/O; JSON parsing/projection runs in a blocking worker.
- Configuration values never cross IPC. env shows property-source names and keys; configprops shows context, bean/prefix and property paths. Condition messages and raw mapping predicates are omitted because they can contain configuration/header values.
- Separate endpoint states for 401, 403, 404 (disabled/unexposed/wrong path), refused redirect, HTTP error, timeout, unreachable runtime, invalid/oversized response and incompatible structure. Unknown response shape is never treated as an empty successful result.
- Servlet and reactive mapping containers accepted. Unknown fields tolerated; unknown handler/source identity stays unresolved.
- Independent Java 8 / Boot 2.7.18 fixture selectively exposes only the five endpoints and binds loopback, with ephemeral ports. Optional real Spring Security scenario uses ephemeral environment-only credentials; no generated passwords are logged.

Tests executed so far:

- Java 8 Maven package passed. Additional public Spring Security jars were resolved into **isolated** cache `../runtime-test-cache/maven-cache`, reading the existing Maven cache without writing it.
- `python3 fixtures/spring-runtime-java8/verify.py`: passed actual five-endpoint projection/redaction, actual Spring Security 401/403 and disabled env 404; both fixture children stopped.
- `cargo test -p ide-http --offline`: passed 29 tests, with 2 Java fixture tests intentionally ignored in the default run and explicitly passed by verify.py. Response-size cap and in-flight cancellation checks also passed.
- `cargo clippy -p ide-http --all-targets --offline -- -D warnings`: passed on the final implementation. `cargo fmt --all --check` also passed.

## Phase 2: Services UI

Implemented and focused checks complete:

- Spring Runtime action opens a Services tab. Explicit connection form, optional memory-only Basic authentication, refresh/disconnect, endpoint selector, filter, 100-row paging, context and dependency facts.
- Disconnect/project switch cancels requests, discards stale responses, clears runtime facts and credentials. Draft credential inputs reset on project changes and clear on disconnect. New IPC stays local even when remote routing exists; remote projects are explicitly rejected by the adapter.
- Links reuse registered Spring Beans / Bean Graph source tool windows. They are labelled source views and are disabled when unavailable; runtime dependencies are shown from the connected application.
- Source navigation requires one exact fully qualified Java class in the current project. Mapping navigation additionally reuses the existing HTTP endpoint provider, requiring matching method/path and a unique source position. Complete controller/declaration metadata permits a parent-file target; older providers retain the same-file rule. Proxy classes, unknown identities, ambiguous packages/overloads and absent source return an unresolved status instead of jumping.
- All new text in en/zh; CSS Modules use existing design tokens. No protected MVC/route/navigation files changed.
- Frontend focused tests: 29 passed, 6 pre-existing locale skips; typecheck (all five configurations), knip, lint (existing unrelated warnings only), changed-file format checks and Vite production bundle passed. Final focused tests passed after the last UI edit. `git diff --check` passed.

## Native integration / unified GUI acceptance still required

No native app was built/launched, and **no GUI screenshots or macOS end-to-end results are claimed**. Per delegation, leave the heavy shared integration build to the main session.

1. Cherry-pick phase commits onto the integration checkout. Small shared-file additions: `src/features/index.ts`, `src-tauri/src/commands/mod.rs`, `crates/ide-http/src/lib.rs`, en/zh text catalog append.
2. Run `pnpm gen:bindings` during the unified native build and commit its generated result. `src/bindings.ts` was not hand-edited. Current frontend uses the isolated typed `invokeLocal<Snapshot>` adapter; runtime command is registered in the Specta builder. The generated-command drift check is **not complete** until regeneration.
3. Command contract: `spring_runtime_snapshot { request, base, user, password } -> IdeResult<ide_http::spring_runtime::SpringRuntimeSnapshot>`, represented by frontend `Snapshot`/`RuntimeRow`/`RuntimeState` in `model.ts`. Cancellation uses existing local `cancel_request`. There are no credentials/config values in the result. No remote host operation is required or added.
4. Run `pnpm tauri dev`/`pnpm dev:app` once in the integration checkout; use its explicit MCP port. Open Java 8 fixture and action “Spring Runtime”. Capture before/after only with synthetic fixture data and empty password input.
5. Verify narrow Services layouts and keyboard focus, en/zh labels, source Beans/Graph buttons, real Bean/type and controller navigation, unresolved library/proxy/duplicate identities, refresh after application restart, disconnect during I/O, project switch, disabled endpoint and bad base path, 401/403, malformed/redirect response, stopped runtime. Ensure runtime facts disappear on disconnect and HTTP history has no runtime exchanges.
6. A newer Boot server, localhost IPv6-only server and local trusted HTTPS certificate are compatibility/manual GUI scenarios, not claimed live-verified here. Reactive container and unknown-field tolerance have module coverage.

No endpoint wildcard exposure was configured in a user project. Original Java 8 demo remained read-only; port 18088 was not touched. Fixture logs and raw response bodies were not saved. Only this session's processes and targets are managed.

## Reproduction commands and artifact boundaries

With the supplied Java 8 and Rust toolchain paths exported as `JAVA_HOME`, `CARGO_HOME`, `RUSTUP_HOME`, and `CARGO`:

```sh
"/Applications/IntelliJ IDEA.app/Contents/plugins/maven/lib/maven3/bin/mvn" -s ../runtime-test-cache/maven-settings.xml -Dmaven.repo.local=../runtime-test-cache/maven-cache -f fixtures/spring-runtime-java8/pom.xml -q -DskipTests package
python3 fixtures/spring-runtime-java8/verify.py
cargo test -p ide-http --offline
cargo clippy -p ide-http --all-targets --offline -- -D warnings
cargo fmt --all --check
pnpm vitest run src/features/spring-runtime src/platform/locales/textCatalog.test.ts
pnpm typecheck
pnpm lint
pnpm knip
pnpm build
```

Frontend commands were executed using this checkout's `node_modules/.bin/*` tools to avoid pnpm automatically reinstalling shared dependencies. Equivalent typecheck configurations were executed individually; `vite build` plus the typechecks covers `pnpm build`. `pnpm format:check` was scoped to changed frontend files; full Rust formatting passed. Full app/workspace Rust checks and UI e2e were not run. Existing unrelated React lint and Vite chunk/dynamic-import warnings remain outside this task's scope.

Only code, fixture source and this report are committed. No fixture jars, logs, dependency caches, raw Actuator responses, screenshots or credentials are included. Native compilation and generated bindings remain explicit integration work, not a tested claim.

## Follow-up: MVC metadata, nested names, and source-name redaction

Follow-up implementation completed after integration review. Runtime consumes optional
`qualifiedController?: string` and `declaringClass?: string` through a local structural
extension of `EndpointEntry`; MVC owns the actual contribution interface and provider
population. No workbench/MVC source files are edited here. Complete metadata plus exact
project symbols and a unique method/verb/path permits navigation to the declaration in a
parent Java file. Missing metadata retains the same-file fallback. Contradictory identities,
multiple candidates, proxy names and external/unknown source stay unresolved.

JVM binary nested names are compared with Java source nested names only after raw FQN
matching. Normalization does not rewrite dollar signs in package segments, and ambiguous
normalized identities remain unresolved. A runtime role must also resolve to the same
exact class location as its MVC controller or declaration metadata.

Fixture additions: inherited BaseController method, overridden controller method, nested
static controller and nested Bean. Real Actuator class-name checks and source-name
redaction regression tests passed. Source names now use an allowlist: ordinary local
file sources are retained, known config-server URLs show a safe origin, and unknown
programmatic names use an index. Configprops bean/prefix and condition-name metadata
reject free-form URI/message text; configuration values/input origins remain omitted.


### Observed Java 8 / Boot 2.7.18 identities

| Fixture mapping / bean | Actual runtime identity | MVC metadata / target |
| --- | --- | --- |
| GET `/runtime-child/inherited` | mappings `com.example.runtime.base.BaseController` | controller `com.example.runtime.ChildController`, declaration `com.example.runtime.base.BaseController`, target `base/BaseController.java` |
| GET `/runtime-nested/inherited` | mappings `com.example.runtime.base.BaseController` | controller `com.example.runtime.NestedFixture.InnerController`, declaration BaseController, target `base/BaseController.java` |
| GET `/runtime-override/override` | mappings `com.example.runtime.OverrideController` | controller and declaration OverrideController, target `OverrideController.java` |
| GET `/runtime-nested/mapping` | mappings `com.example.runtime.NestedFixture.InnerController` | source FQN `com.example.runtime.NestedFixture.InnerController`, target `NestedFixture.java` |
| Nested controller and Bean | beans `com.example.runtime.NestedFixture$InnerController` / `NestedFixture$InnerBean` | canonical source nested FQN, unique class position in `NestedFixture.java` |

Boot 2.7.18 actually uses canonical dotted names for these nested **mapping** className
fields, and binary dollar names for the **beans** type fields. Both were asserted from the
real responses; no assumption that mappings must return a dollar name is made. Unit tests
also cover a mapping response using a binary nested name and a runtime handler reporting
the controller FQN instead of the declaration FQN.

### Follow-up verification

- Frontend/navigation/text catalog: **40 passed**, 6 existing locale skips. Tests include
  both inherited handler class identities, nested controller inheritance, raw FQN priority,
  ambiguous normalized symbols, package dollar names, same-name/different-package classes,
  multiple provider identities/targets, incomplete metadata fallback, unrelated files,
  proxies and unknown/library classes. Bare or relative symbol names never bypass their
  reported package/container identity.
- Full ide-http module suite: **31 passed**, 2 fixture-only tests ignored in the default
  run. Both ignored fixture tests explicitly passed via `verify.py` after the final changes.
- The live fixture also adds synthetic external and programmatic property-source names.
  Userinfo/query authentication markers never appear in projected IPC; config-server
  detail is its scheme/host origin, unknown source detail is an index. Actual local
  `application.properties` source detail remains available. Condition source/condition/
  unconditional-class strings and configprops bean/prefix identifiers are constrained;
  free-form URI/message metadata is omitted/indexed. Input value/origin/message fields
  remain excluded.
- Java 8 offline Maven package, typecheck, targeted lint/format, knip and frontend Vite build
  passed. Final `cargo clippy -p ide-http --all-targets --offline -- -D warnings` and `cargo fmt --all --check` passed.

### Integration scenarios for the main session

1. Integrate the MVC contribution fields exactly as optional strings. `qualifiedController`
   identifies the actual mapped controller source FQN; `declaringClass` identifies the
   method owner represented by path/line/col. No required fields were added by Runtime,
   and no other feature/provider files were changed here.
2. With both contributions present, connect the Java 8 fixture and locate the inherited
   method from ChildController and from the nested controller. Both should open the
   actual declaration in `base/BaseController.java`, with existing UTF-16 coordinates.
3. Locate the nested controller mapping and nested Beans; verify the correct class or
   method position in `NestedFixture.java`. Same-name classes in another package, duplicate
   normalized candidates and `$$`/JDK proxy names must remain unresolved. Raw exact FQN
   matches take priority over normalized aliases.
4. Repeat with either metadata field absent: parent-file navigation must remain unresolved,
   while the legacy unambiguous same-file mapping still opens. Verify no source jump for
   external/unknown declaration or controller identities.
5. The fixture's external/programmatic property sources must show only safe origin/index,
   no synthetic authentication marker; ordinary local source detail remains visible.

Native compilation, generated bindings and macOS GUI/end-to-end acceptance remain pending
for unified integration, unchanged from the original delivery. No GUI screenshots or
native navigation acceptance are claimed by the fixture/module checks.


## Follow-up: concrete Specta IPC result

Main-session native export reached a genuine compilation failure: `IdeResult<serde_json::Value>`
does not implement the required Specta FunctionResult. This independent patch replaces the
IPC result with a concrete domain contract; it does not import any other integrated line.

- `ide-http::spring_runtime` publicly re-exports `SpringRuntimeSnapshot`, `SpringRuntimeData`,
  `SpringRuntimeRow`, `SpringRuntimeState`, and `SpringRuntimeOutcome`. All derive
  Serialize/Deserialize/Specta Type. Snapshot explicitly contains the five endpoint data
  fields; data/rows contain only named strings, string vectors, booleans, u16 status and
  enum values. There are no Value/Unknown/flatten fields in the exported wire types.
- Missing optional fields deserialize to None and serialize with skip-none; optional fields
  are also marked optional for Specta. Existing camelCase field/state/outcome strings are
  preserved. Required endpoint/row fields remain required. Unknown fields/type/enum drift
  fail validation rather than passing unknown data through IPC.
- The existing allowlist JSON projection remains internal to the runtime pipeline. Its
  completed sanitized object is consumed/validated into `SpringRuntimeSnapshot` before
  return. Conversion runs in a blocking worker, honors cancellation, and reports only a
  constant internal error if the contract fails; projected contents never enter errors.
- Tauri glue now returns `IdeResult<SpringRuntimeSnapshot>` directly. Frontend and generated
  bindings were not edited. The frontend's native-only `invokeLocal` path and local
  cancellation path, plus the native adapter's remote-project rejection, remain unchanged.
- A lightweight regression test binds Specta FunctionResult to the **actual production
  snapshot future**, so changing its return type back to an unsupported type breaks this
  module test without needing a Tauri/native build. A dev-only Specta function feature
  enables this check; no dependency versions or Cargo.lock changes are needed.

Verification on this patch:

- `cargo test -p ide-http --offline`: **35 passed**, 2 fixture tests ignored by default.
- `fixtures/spring-runtime-java8/verify.py`: both actual Java 8 / Boot 2.7.18 fixture tests
  passed through the new typed return boundary (including inheritance/nested identities,
  source redaction and actual 401/403/disabled-404 behavior). Children stopped automatically.
- Four contract tests cover actual async FunctionResult support, missing optional fields /
  omitted None serialization, unchanged field/state/outcome wire casing and strict schema
  rejection with no data in the error. Final `cargo clippy -p ide-http --all-targets --offline -- -D warnings` and `cargo fmt --all --check` passed.

Integration: cherry-pick only this new commit on top of the main session's already-integrated
runtime patch (main reported current integration `cfde029a`). Then rerun the main native
`export_bindings` command. Generated public type names are the SpringRuntime-prefixed names
above, and the command result is now concrete. If the frontend adopts those generated
aliases, retain `invokeLocal` (or an explicitly native-only command adapter); do not route
this command or its cancellation through a remote server. No heavy native build or GUI
verification is claimed from this session; main performs that unified check.
