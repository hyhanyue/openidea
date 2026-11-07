# Spring MVC / WebFlux mapping delivery

## Checkout and integration

- Independent clone: `/Users/hyl/Documents/Codex/2026-11-07/openidea-spring-mvc-mappings/work/openidea`.
- Branch `spring-mvc-complete`, base `57653df63a91789a3544436109ee288b7cfb0ba6`.
- All 12 handoff SHA256 entries matched; patch applied cleanly without copying uncommitted changes.
- Handoff checkpoint `4900b91f` contains the complete original WIP. Main session reports it already integrated as `313e2d83`; only commits after `4900b91f` should be cherry-picked next.
- No push, release, installed-app replacement, native app build/launch, original-demo modification or port 18088 use.

## Changes beyond the handoff

- Preserve absent versus unresolved mappings. Unresolved class/overridden-method mappings cannot silently fall through to a root or inherited route. Arrays resolve atomically; property/SpEL paths remain unknown. Parameter annotations respect qualified/imported Spring identities.
- Resolve local, cross-file and nested composed annotations, stereotype metadata, aliases/defaults, explicit and transitive implicit alias sets. Reject conflicting values, unequal defaults, invalid local links, absent meta targets and missing target attributes. Alias override priority preserves outer defaults over fixed inner metadata and propagates into implicit alias peers.
- Correct the handoff assumption about single-sided local AliasFor: Spring 5.3.31 does support it (introduced in 5.2.1). A real Java8 fixture and a parser regression now prove this.
- Scope compile-time constants lexically and honor qualified references; do not borrow same-named fields from unrelated classes or treat mutable Java fields as constants. Share a weakly held constant index per parsed source and retain per-path declarations rather than rebuilding them per query.
- Canonicalize generic parent arguments using the consumer's imports, preserve overload identity, merge inherited parameter annotations, and navigate to an inherited concrete superclass implementation when an interface contract is also present.
- Share project models among endpoint provider, gutter, menus, intentions and request actions. Retain all explicit methods, class/method method unions and path combinations; unrestricted mappings are `*` in search and choose GET as a request-generation convenience.
- Cache source snapshots separately from derived models. Unsaved annotation/interface edits update dependency consumers without another IPC scan. File events invalidate disk snapshots; close/save events release overlays. Concurrent consumers share work, failures retry, obsolete/disposed scans cannot install results, and parsing/indexing yields every 16 sources. Editor revisions prevent delayed results from touching destroyed or newer documents.
- Bound host reads to 1024 files / 48 MiB total and 4 MiB per source, including aggregate content-root results. Frontend scans and overlays are each bounded to 1024 files / 24 Mi UTF16 units; individual sources above 2 Mi UTF16 units are omitted. Oversized unsaved files suppress their stale disk routes. Method/path expansion is capped at 1024 paths per endpoint and 20,000 search entries. Limits can produce incomplete results, never proof of absence. An in-flight native scan completes in its background worker; frontend cancellation prevents subsequent parsing/publication without adding an IPC binding.
- Use native gutter buttons with keyboard activation and existing translated labels. New CSS resets use transparent/inherited colors and existing layout tokens. No new user-visible catalog keys were introduced.
- Generate all method/path variants or the selected menu variant. Request-body lookup uses inherited declaration imports and qualified concrete generic DTO types.

## Final audit checkpoint

The reviewed mapping/model implementation is committed as `be7c77ec` after the imported WIP. A subsequent small audit fixes constant reuse across concatenation branches (reproduced as a failing test first), refreshes gutter choices before opening them, abandons request generation if its document/project changed during the scan, and releases overlays on project switches. The final HTTP suite has 65 passing tests, including the new generation race and project-switch cases. A final real-framework fixture also checks that explicitly writing an empty alias default does not conflict with a non-default peer; the framework output is byte-identical to the previous ground truth. The parser normalizes defaults before alias conflict checks. Full-suite validation began on the reviewed checkpoint; the final changed modules were re-run separately afterward.

## Evidence and reproduction

`fixtures/java-spring-mappings/README.md` documents regeneration. `ground-truth.json` is emitted by real MVC and WebFlux RequestMappingHandlerMapping instances using Boot 2.7.18 / Spring 5.3.31 on JDK8. Maven runs only inside this clone, uses a private dependency cache at `../maven-repo`, and opens no network listener. Ten Java tests cover MockMvc dispatch, multilevel aliases, defaults, conflicts, custom stereotypes, cross-file generic interfaces and parents, overloads, foreign annotation names and unrestricted/union methods. The TypeScript ground-truth test compares every static handler against both framework registries.

The initial five new review regressions all failed against the imported WIP before fixes. An additional whole-fixture comparison exposed and reproduced the nested-default/implicit-alias issue before its fix.

## Validation completed

- `node_modules/.bin/vitest run src/features/http-client`: 8 files, 65 tests passed.
- Real JDK8 Maven fixture: 10 tests passed, 0 failures/errors. Ground-truth evidence regenerated after the final fixture extension.
- `node_modules/.bin/tsrs --noEmit` and tsconfig.node/perf/scripts/smoke projects: passed.
- `node_modules/.bin/tsc --noEmit && node_modules/.bin/vite build`: passed; existing chunk-size/dynamic-import warnings.
- `node_modules/.bin/oxlint`: passed, existing warnings in unrelated BuildView/VCS/logcat/List/notebook modules. Targeted Spring lint is clean.
- `node_modules/.bin/oxfmt --check`: all 2102 matched files passed after the final regression additions; `git diff --check` passed.
- `node_modules/.bin/knip`: passed with one existing ignore-pattern configuration hint.
- Shared toolchain `rustfmt --edition 2024` on the two owned Rust files: passed.
- Full frontend suite: running (two workers to limit contention); final result will be appended.

Dependency setup: offline pnpm lacked cached tarballs, so this session copied the installed node_modules into its own checkout and invoked local binaries. Public Maven dependencies were downloaded into this session's isolated cache. No shared target or dependency installation was mutated.

## Full-suite environment check

The sandboxed full run reported three timeouts in unchanged `scripts/dev-session.test.ts`; these tests listen on localhost ports. Re-running that entire file with localhost access passed all 5 tests in 717 ms. This confirms the port sandbox restriction rather than a change to the launcher. The full sandboxed suite is still running; final aggregate counts will be appended.

## Required unified Mac/native acceptance (not performed here)

No native app runtime or E2E screenshot evidence is claimed. Per the parallel-session constraint, reserve the cold ide-host/Tauri build for the integrating session:

1. Run `cargo fmt --all --check`, `cargo test -p ide-host --lib ops::routes::tests`, then the required ide-host clippy/native build checks in the integration checkout's independent target. The changed Rust adapter/scanner have only been format-checked here, not Rust-compiled.
2. Launch the integrated checkout using `pnpm dev:app`; use `pnpm dev:status` to locate and verify that exact worktree's bridge. Leave the installed release and original Java8 demo untouched; use this fixture or a copied demo on a free port, never 18088.
3. Open `CrossController.java`: verify route search contains POST and PUT for `/cross/save`, `/cross/write`, `/v2/save`, `/v2/write`; search navigation lands on the concrete implementation. Gutter and editor menu offer the same eight choices; choose PUT `/v2/write` and inspect the generated request body.
4. Open `InheritedController.java`: controller gutter generates `/parent/send`, body includes Order.name; route search navigates to GenericBase.send. Check UTF16 navigation with a surrogate-pair comment on the method's line.
5. Edit Layer.routes and Contract paths without saving; all open consumer menus/gutters and route search should follow the overlay. Save, modify externally, rename/delete the dependency, then undo/restore and verify rescans. Confirm no fake root route for a dynamic property/SpEL or unresolved annotation expression.
6. Keyboard-activate a gutter button; verify menu placement, localized labels and request insertion. Rapidly edit/switch/close documents and switch projects while a large scan runs; verify responsiveness and absence of stale icons/results. Capture before/after screenshots via the repository's verify workflow during unified acceptance.

## Explicit limitations and integration notes

- This is source-static annotated-controller assistance for MVC and WebFlux. Functional RouterFunction DSLs, programmatic registrations, runtime bean activation/conditions/proxies and annotations/contracts only present in dependency JARs are not resolved. Dynamic placeholders/SpEL are deliberately omitted. Missing/ambiguous declarations do not establish that a running server has no route.
- The frontend retains the existing project-root source boundary; external content roots returned by the host are filtered until the endpoint-provider API can express their ownership. This pre-existing API limitation remains for integration planning.
- Full Java compiler overload/type inference (wildcards, raw/bounded/method type variables, type-use syntax, arbitrary annotation value evaluation) is outside this source reader. Covered generic source contracts and their concrete overloads are proven by real Spring fixtures; unsupported forms need additional evidence before extension.
- Request skeleton generation remains a shallow source-based preview; it does not evaluate runtime Jackson configuration or arbitrary serializers.
- No other session's feature files were changed. The shared entrypoints are `controllersForSource`, `knownControllers`, `updateRouteSource`, `onRouteIndexChanged` and `routeIndex` within the owned Spring folder. IPC types/bindings are unchanged; native command remains background-dispatched.
