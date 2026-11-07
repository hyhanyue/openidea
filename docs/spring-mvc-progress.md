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

The reviewed mapping/model implementation is committed as `be7c77ec` after the imported WIP. A subsequent small audit fixes constant reuse across concatenation branches (reproduced as a failing test first), refreshes gutter choices before opening them, abandons request generation if its document/project changed during the scan, and releases overlays on project switches. The final pre-seam HTTP suite had 65 passing tests; after the requested Runtime metadata seam it has 72 passing tests, including the new generation race and project-switch cases. A final real-framework fixture also checks that explicitly writing an empty alias default does not conflict with a non-default peer; the framework output is byte-identical to the previous ground truth. The parser normalizes defaults before alias conflict checks. Final changed modules were re-run separately after each audit fix, followed by a fresh full-suite run after code freeze.

## Evidence and reproduction

`fixtures/java-spring-mappings/README.md` documents regeneration. `ground-truth.json` is emitted by real MVC and WebFlux RequestMappingHandlerMapping instances using Boot 2.7.18 / Spring 5.3.31 on JDK8. Maven runs only inside this clone, uses a private dependency cache at `../maven-repo`, and opens no network listener. Ten Java tests cover MockMvc dispatch, multilevel aliases, defaults, conflicts, custom stereotypes, cross-file generic interfaces and parents, overloads, foreign annotation names and unrestricted/union methods. The TypeScript ground-truth test compares every static handler against both framework registries.

The initial five new review regressions all failed against the imported WIP before fixes. An additional whole-fixture comparison exposed and reproduced the nested-default/implicit-alias issue before its fix.

## Validation completed

- `node_modules/.bin/vitest run src/features/http-client`: 10 files, 72 tests passed.
- Real JDK8 Maven fixture: 10 tests passed, 0 failures/errors. Ground-truth evidence regenerated after the final fixture extension.
- `node_modules/.bin/tsrs --noEmit` and tsconfig.node/perf/scripts/smoke projects: passed.
- `node_modules/.bin/tsc --noEmit && node_modules/.bin/vite build`: passed; existing chunk-size/dynamic-import warnings.
- `node_modules/.bin/oxlint`: passed, existing warnings in unrelated BuildView/VCS/logcat/List/notebook modules. Targeted Spring lint is clean.
- `node_modules/.bin/oxfmt --check`: all 2104 matched files passed after the final regression additions; `git diff --check` passed.
- `node_modules/.bin/knip`: passed with one existing ignore-pattern configuration hint.
- Shared toolchain `rustfmt --edition 2024` on the two owned Rust files: passed.
- Broad frontend suite (four workers, begun before the late Runtime seam request): 656 files passed, 1 environment-restricted file failed; 7274 tests passed, 3 port tests failed, 6 skipped; 335.46 seconds. All 3 errors are `listen EPERM 127.0.0.1` in unchanged dev-session tests. That entire file passed all 5 tests with localhost access. The subsequent Runtime seam has a separate final HTTP regression run: 10 files / 72 tests passed. The earlier two-worker run was discarded because it spanned source changes and cached old modules against new fixture/tests; only this session's obsolete runner was stopped.

Dependency setup: offline pnpm lacked cached tarballs, so this session copied the installed node_modules into its own checkout and invoked local binaries. Public Maven dependencies were downloaded into this session's isolated cache. No shared target or dependency installation was mutated.

## Full-suite environment check

The sandboxed full run reported three timeouts in unchanged `scripts/dev-session.test.ts`; these tests listen on localhost ports. Re-running that entire file with localhost access passed all 5 tests in 717 ms. This confirms the port sandbox restriction rather than a change to the launcher. The completed four-worker sandboxed broad run passed 7274 tests and failed only those 3 port tests, with 6 skips (657 files total). The earlier run mixed transformed modules from before the final fixes with newly extended tests, so its Spring failures are not a valid result for the final tree.

## Required unified Mac/native acceptance (not performed here)

No native app runtime or E2E screenshot evidence is claimed. Per the parallel-session constraint, reserve the cold ide-host/Tauri build for the integrating session:

1. Run `cargo fmt --all --check`, `cargo test -p ide-host --lib ops::routes::tests`, then the required ide-host clippy/native build checks in the integration checkout's independent target. The changed Rust adapter/scanner have only been format-checked here, not Rust-compiled.
2. Launch the integrated checkout using `pnpm dev:app`; use `pnpm dev:status` to locate and verify that exact worktree's bridge. Leave the installed release and original Java8 demo untouched; use this fixture or a copied demo on a free port, never 18088.
3. Open `CrossController.java`: verify route search contains POST and PUT for `/cross/save`, `/cross/write`, `/v2/save`, `/v2/write`; search navigation lands on the concrete implementation. Gutter and editor menu offer the same eight choices; choose PUT `/v2/write` and inspect the generated request body.
4. Open `InheritedController.java`: controller gutter generates `/parent/send`, body includes Order.name; route search navigates to GenericBase.send. Check UTF16 navigation with a surrogate-pair comment on the method's line.
5. Edit Layer.routes and Contract paths without saving; all open consumer menus/gutters and route search should follow the overlay. Save, modify externally, rename/delete the dependency, then undo/restore and verify rescans. Confirm no fake root route for a dynamic property/SpEL or unresolved annotation expression.
6. Keyboard-activate a gutter button; verify menu placement, localized labels and request insertion. Rapidly edit/switch/close documents and switch projects while a large scan runs; verify responsiveness and absence of stale icons/results. Capture before/after screenshots via the repository's verify workflow during unified acceptance.

## Runtime navigation metadata contract (late explicit scope addition)

Code checkpoint: `5b843128640ce89621f62c7136a92c2dfe6b95ca`.

The parent explicitly authorized the small `src/workbench/contributions.ts` interface addition. `EndpointEntry` gains two **optional** strings; other endpoint providers need no changes:

- `qualifiedController`: the actual source controller type's unique fully qualified **JVM binary name**, matching `Class.getName()`. Example: `dev.openidea.mappings.MappingSamples$Child`.
- `declaringClass`: the class/interface that declares the source method at `path/line/col`, using the same binary-name convention. For the inherited Child method it is `dev.openidea.mappings.MappingSamples$Base`; for a concrete override of an interface method it is the overriding controller class, not the annotation-bearing interface.

Identity is assigned inside each exact source-model revision, including unsaved replacements. Duplicate source identities (including binary-name collisions) omit the uncertain field. Unknown library parents do not create invented inherited endpoints. The provider retains separate controller rows when two same-simple-name controllers inherit the same method/coordinate; consumers must not collapse them by target coordinate before applying their identity/candidate rules. Existing real declaration/implementation `path/line/col` remain unchanged and UTF16.

`fixtures/java-spring-mappings/navigation-identities.json` records real MVC **and** WebFlux `HandlerMethod.getBeanType().getName()` and `getMethod().getDeclaringClass().getName()` alongside mapping keys. `navigationGroundTruth.test.ts` compares all 28 framework handler rows to static identities; `navigationIdentity.test.ts` covers cross-file inheritance coordinates, concrete generic overrides, unsaved revisions, same-name different-package consumers, nested binary names, duplicate FQN omission and unavailable library contracts. The real Java8 fixture remains 10/10; its prior mapping JSON is byte-identical after adding the identity evidence.

Runtime integration acceptance (Runtime session owns its actual Actuator consumer fixture):

1. Actuator reports the concrete InheritedController class: match `qualifiedController` plus exact method name, HTTP method and route; navigate to GenericBase.java at the inherited method's original coordinate.
2. Actuator reports the declaring GenericBase class: match `declaringClass` with the same route/method constraints and strict unique-candidate/source-target checks; do not require the target path to equal the controller source path.
3. Concrete CrossController interface override: both fields name CrossController and coordinates remain at its implementation, not Contract.java.
4. Nested fixture types use `$` in both fields. Any runtime canonical-name handling must be explicit; do not treat arbitrary proxy suffixes or simple-name equality as identity evidence.
5. Duplicate FQNs, same-simple-name candidates in different packages, multiple candidates, unknown dependency types and proxies remain unresolved under Runtime's conservative rules. Without the optional fields, retain the existing same-source-file fallback. Other providers with neither field must continue to typecheck and behave as before.

No Runtime navigation implementation or other session's fixture was modified. The additional shared-file diff is exactly the four interface lines; the parent will merge it with unrelated workbench changes. No chat messages were sent to the Runtime session.

## Official references

- [Spring MVC 5.3.31 request mapping](https://docs.spring.io/spring-framework/docs/5.3.31/reference/html/web.html#mvc-ann-requestmapping)
- [Spring WebFlux 5.3.31 request mapping](https://docs.spring.io/spring-framework/docs/5.3.31/reference/html/web-reactive.html#webflux-ann-requestmapping)
- [Spring AliasFor 5.3.31 requirements](https://docs.spring.io/spring-framework/docs/5.3.31/javadoc-api/org/springframework/core/annotation/AliasFor.html)

## Explicit limitations and integration notes

- This is source-static annotated-controller assistance for MVC and WebFlux. Functional RouterFunction DSLs, programmatic registrations, runtime bean activation/conditions/proxies and annotations/contracts only present in dependency JARs are not resolved. Dynamic placeholders/SpEL are deliberately omitted. Missing/ambiguous declarations do not establish that a running server has no route.
- The frontend retains the existing project-root source boundary; external content roots returned by the host are filtered until the endpoint-provider API can express their ownership. This pre-existing API limitation remains for integration planning.
- Full Java compiler overload/type inference (wildcards, raw/bounded/method type variables, type-use syntax, arbitrary annotation value evaluation) is outside this source reader. Covered generic source contracts and their concrete overloads are proven by real Spring fixtures; unsupported forms need additional evidence before extension.
- Request generation preserves the existing first-media-condition choice; it is not a general solver for contradictory/negated params, headers or media predicates.
- Request skeleton generation remains a shallow source-based preview; it does not evaluate runtime Jackson configuration or arbitrary serializers.
- No other session's feature files were changed. The shared entrypoints are `controllersForSource`, `knownControllers`, `updateRouteSource`, `onRouteIndexChanged` and `routeIndex` within the owned Spring folder. IPC types/bindings are unchanged; native command remains background-dispatched.
