# Spring Run / Debug delivery

Independent clone: `/Users/hyl/Documents/Codex/2026-11-07/openidea-spring-run-debug/work/openidea`.
Branch: `codex/spring-run-debug`. Baseline: `57653df63a91789a3544436109ee288b7cfb0ba6`.
Commits (apply in order): audit `8cde1937`; implementation **`f65beb11d976b92661b2476f701bf682bfc91644`**. This final report is frozen in a following documentation commit.

## Implemented

- Existing Application/Spring Boot forms already persist profiles, VM/program args, env/env files, cwd, JRE and module. Kept that model and added actual shared/local persistence + Java8 regression rather than a duplicate runner.
- Maven Boot gets separate application program arguments and application working directory, forwarded via `spring-boot.run.arguments` and `spring-boot.run.workingDirectory`. Maven's own cwd/goals remain independent. Fixed fork argument quoting when one argument contains both quote characters; real Maven run/debug verifies it.
- Java Run and Debug retain the resolved Project SDK / explicit run JRE, independently of the JDT tooling JDK. Tests explicitly use a Java8 project executable with a different tooling javaHome.
- When system environment inheritance is disabled, Java Debug uses the existing Run process manager to launch the project JVM, then the same debugger attaches on loopback JDWP. This fixes java-debug merging the JDT tooling process environment even when inheritance was disabled. Env-file precedence, args, cwd and project executable follow the Run path.
- Spring lifecycle facts distinguish preparation, compilation, application startup, observed Boot startup completion, failure, stopping and stopped. Process liveness stays separate: a startup failure log does not falsely claim a live process exited. Direct Java Debug and Maven/Run output use the same parser. `Started ... (JVM running for ...)` is a log milestone, not an HTTP readiness/health guarantee.
- Failure summary categories: occupied port, missing required Bean, ambiguous Bean and configuration binding. Existing JVM stack links remain; Boot's explicit file/classpath-resource configuration origins now link to line/column. Duplicate resource paths across modules remain unresolved; cwd is never used to guess a classpath module.
- Restart refuses to replace a still-live/still-starting process after its stop timeout. Stop during native `runStart` waits for its owned RunId and stops only that process; reset/close while spawn is in flight also disposes its returned process. A failed stop restores live state. No process-name kills or service-port kills.
- Application update already distinguishes resource/class publication, debugger HotSwap and restart. No DevTools dependency is installed. No Initializr/starter creation entry was found in inspected source; no new download flow.

## Frozen producer/consumer contract

Authoritative shared file (single writer F):
`/Users/hyl/Documents/Codex/2026-11-06/https-github-com-anomalyco-opencode-https-2/work/spring-parity-coordination/launch-context-contract.json`.
Committed identical fixture: `docs/spring-launch-context-contract.json`.
Protocol: `openidea.spring-launch-context`, version **1**. Six public synthetic combination samples cover dev→prod, CLI/system/env precedence, saved nonboolean properties, deselection, unknown high-priority values and stable plugin ID routing.

- Run publishes through platform `useSpringLaunchContext`; frontend features do not import each other's stores.
- Bridge: existing `pluginRunCommand(installedId, "openidea.setLaunchContext", JSON.stringify({projectRoot, context}), null, [])`. Exact known aliases only, routed to the actually enabled active/inactive package, preferring `hyhanyue.spring-beans`; source `aritti.spring-beans` and migration `hyhanyue.spring-beans-local` are supported. Native validates enabled status and expected project root.
- **Guest reads `host::setting("springBeans.launchContext")` as a JSON object**, with `{version:1, launch:{configId,name,kind,persisted,mainClass,module,cwd,profiles,commandLineProperties,systemProperties,environment,inheritEnvironment,envFiles,unresolvedSources}}`. This is **not** the draft `openideaLaunchContext` JSON-string envelope in `springBeans.propertyOverrides` and does not overwrite that user setting. The old persistent analysis settings remain user-owned.
- Each source map is `Map<string, Fact>`, where Fact is `{state:"known",value:string}` or `{state:"unknown"}`. The production transport carries only non-sensitive literal booleans and validated profile names as known values. Every other configured value is an **unknown present override with a source**, never known absence / false / the old `!key` tombstone. An unknown high-priority value blocks lower-priority file fallback for that key.
- `persisted:true` refers to the exact selected ID in `.openidea/run.toml` / `.openidea/run.local.toml` under the current authorized project. The consumer reads matching saved args/env values in memory, validates selection metadata, and can resolve nonboolean properties. It must not pick a different ID, import the complete inherited process environment, expand third-party plugin scope, or log/show/persist sensitive values. Unsaved/unreadable/mismatched references retain source-specific unknown overrides.
- `context:null` removes the transient layer only. Guest `{version:1,launch:null}` means conflicting/unknown selection, not permission to retain a previous launch. Selected configs with unresolved profiles use `{state:"unknown",values:[],source:"unknown"}`.
- The native host replaces context atomically per `(window, project)` and snapshots it once per guest invocation. Windows without a selection still participate in conflict detection. Background editor providers lack window identity; conflicting window selections yield unknown. Views retain invoking-window identity.
- Marker/view generations invalidate pending results; Spring view/marker requests wait for synchronization. Native navigation contributions are invalidated after replacement. Closed windows and late project-mismatched requests cannot leak earlier context.
- IPC/WIT wire types are unchanged; `src/bindings.ts` was not edited and needs no regeneration. `HostApi::settings_overlay` is an internal defaulted Rust method, restricted to known Spring IDs by the host implementation.

## Validation

All commands run from this independent clone. Shared dependencies are read from caches; `target` is local to this clone. Frontend binaries are invoked directly from cached `node_modules/.bin` because pnpm's auto-install guard refuses an external node_modules symlink; no shared dependency directory was mutated.

- Java8 JDWP probe: `/Library/Java/JavaVirtualMachines/jdk-1.8.jdk/Contents/Home/bin/java -agentlib:jdwp=transport=dt_socket,server=y,suspend=n,address=127.0.0.1:0 -version` succeeded (Java 1.8.0_421). The initial sandbox socket-bind denial was rerun with local execution permission. No speculative JDWP syntax change.
- Fixture compile/classpath: `JAVA_HOME=/Library/Java/JavaVirtualMachines/jdk-1.8.jdk/Contents/Home "/Applications/IntelliJ IDEA.app/Contents/plugins/maven/lib/maven3/bin/mvn" -o -f fixtures/spring-run-debug-java8/pom.xml compile org.apache.maven.plugins:maven-dependency-plugin:3.8.1:build-classpath -Dmdep.outputFile=target/classpath.txt` passed. Default dependency plugin 3.3.0 was incompletely cached; cached 3.8.1 succeeded without download.
- Real regression: `JAVA_HOME=<Java8> OPENIDEA_JAVA8_FIXTURE=<this clone>/fixtures/spring-run-debug-java8 OPENIDEA_MAVEN=<IDEA Maven>/bin/mvn cargo test -p ide-run --test spring_java8 --offline -- --ignored --nocapture` passed in **41.17s**. Actual Java8/Boot2.7.18 processes and HTTP verified shared/local config reload, two profiles, env/env-file precedence, VM options, quoted program args, cwd, Java8 executable, JDWP suspend/resume, restart, isolated stop, and four failure paths. Real Maven Run and JDWP Debug were included. Binding failure emitted an explicit source origin. Ports are ephemeral; 18088 was not used.
- `cargo test -p ide-run --test config --test spring_launch_context --offline`: **15 passed**. Includes the exact shared JSON producer payloads, all known aliases, project guard, window isolation, invocation snapshots and unknown facts.
- `cargo check -p ide-host --lib --offline -j 2` passed (cold metadata check 10m11s; no app link/launch).
- `cargo clippy -p ide-host -p ide-plugin -p ide-run --all-targets --offline -j 2 -- -D warnings` passed (1m53s).
- Frontend main/node/perf/scripts/smoke TypeScript checks passed; `vite build` passed (existing chunk-size/dynamic-import warnings).
- Broad frontend snapshot: **105 suites / 1227 tests passed** in 110.42s. Focused final changes (bounded-context fallback and in-flight spawn Stop) were rerun: **29 passed**, and the SDK/cancellation/selection group: **58 passed**. Text catalog validation, Oxfmt, Oxlint, Knip and `git diff --check` passed (Knip's existing unused-ignore hint only).

No full launch environment appears in logs, screenshots, this contract or this report. Fixture HTTP exposes only allowlisted synthetic markers, Java version/cwd and an inheritance boolean. Original demo, main checkout, installed app, main branch and release artifacts remain untouched.

## Pending unified Mac acceptance

Native app UI, before/after screenshots, IDE DAP breakpoint/variable inspection, stable-ID real WASM wiring and combined session A environment consumption remain for unified Mac acceptance. The real JDWP test proves transport and suspended/resumed Java8 apps; it does not pretend to be the full IDE DAP/GUI flow. Producer/consumer combination conformance has passed as recorded below; the actual combined native app/WASM/UI remains a separate acceptance gate.

## Combined consumer conformance — passed

A's consumer reads the same `springBeans.launchContext` object and its copied fixture matches the canonical SHA256 **`3a5e0d26690ee5a64f17619b97cdf4edd272137d3bba9d15a175758a7f0f375e`**.

`scripts/verify-spring-launch-contract.py` copies the consumer, SDK, support and fixture sources into F's own isolated scratch tree. It appends only the combination test to that copy and uses an independent Cargo target. The original A checkout/target is never modified. The exact producer payloads run through the actual consumer environment snapshot in the shared sequence; this is not a mock consumer or separate hand-written interpretation.

Results: **1 F combination test passed**, then **3 A consumer contract tests passed**, covering the same shared samples, unmatched saved metadata, unknown/null rather than absence, shared/local duplicate-ID precedence and project-scoped env files. The final manifest records the frozen contract digest, source hashes and whether the source snapshot still matches A. Earlier in-progress source/test-copy issues were corrected before this successful snapshot.

Command:

```sh
python3 scripts/verify-spring-launch-contract.py \
  --consumer-root /Users/hyl/Documents/Codex/2026-11-07/openidea-spring-boot-parity/work/openidea \
  --work-dir /Users/hyl/Documents/Codex/2026-11-07/openidea-spring-run-debug/work/contract-consumer \
  --cargo /Users/hyl/Documents/Codex/2026-11-06/https-github-com-anomalyco-opencode-https-2/work/openidea-toolchain/rust/cargo/bin/cargo
```

Use the shared `CARGO_HOME` and `RUSTUP_HOME` from the task environment; no inherited environment values are emitted. The result manifest is in this session's `work/contract-consumer/manifest.json`. A owns its consumer commit; the main session must integrate that commit together with F's producer, not use the older draft envelope implementation.

## Integration / Mac acceptance recipe

1. Apply F's commits in order. No changes were made to MVC `http-client/spring`, `ops/routes.rs`, `commands/nav.rs`, runtime Actuator panels, generated bindings or installed/release resources.
2. Use an independent fixture copy. Set Project SDK to Java8 and JDT tooling SDK independently. Create dev/prod Spring Boot configs for `com.example.demo.DemoApplication`, explicit module, distinct cwd/env/env files, VM `-Dprobe.vm=...`, args `--server.port=0 --probe.arg=...`; save/reopen and inspect both configs. Never run the original demo or port 18088.
3. Run then Debug, exercise a controller breakpoint/variables, verify `/launch-probe`, switch configs and inspect Bean/Profile analysis. Check both ordinary inherited-env Debug and isolated-env owned Run+JDWP Debug. Verify stable `hyhanyue.spring-beans` real WASM, then unknown/deselected facts.
4. Stop/restart each config, Stop during preparation and launch, and exercise `missing`, `ambiguous`, `binding` plus a synthetic occupied port. Binding-origin links must select the deterministic resource; duplicate module resources must stay unresolved. Capture UI before/after evidence from the combined native build.
5. Update resources/classes/HotSwap/restart labels must keep their existing distinct meanings; do not add DevTools or claim restart is HotSwap.

Official references checked: [Spring Boot Maven plugin 2.7.18](https://docs.spring.io/spring-boot/docs/2.7.18/maven-plugin/reference/htmlsingle/), [Boot 2.7 external configuration](https://docs.spring.io/spring-boot/docs/2.7.x/reference/htmlsingle/), and [java-debug's environment merge implementation](https://github.com/microsoft/java-debug/blob/main/com.microsoft.java.debug.core/src/main/java/com/microsoft/java/debug/core/adapter/handler/LaunchRequestHandler.java).

## Final verification freeze

- Implementation commit: `f65beb11d976b92661b2476f701bf682bfc91644`.
- Consumer verification exit code: **0**; both filters passed; `sourceStillMatches: true` at snapshot completion.
- Canonical / committed / consumer copied contract SHA256: `3a5e0d26690ee5a64f17619b97cdf4edd272137d3bba9d15a175758a7f0f375e`.
- A consumer source fingerprints from the verified snapshot:
  - `plugins/official/spring-beans/src/environment.rs`: `d2bb84192dc23a45985b460d68d27c7c6d3e72d402693b66bfcba4c70941c4a9`.
  - `plugins/official/spring-beans/src/launch.rs`: `dbd1ec625f9aa5ed36f5df0a10fcb6faa551f2f8759a249450f1e5b9ba05cf87`.
  - `plugins/official/spring-beans/src/context.rs`: `c362ee47d3766c87162d329bcac022d9423d70bb1e787bcb2bc996bc5c8793e0`.
- Final source-text catalog: **20 passed, 6 skipped**; the skipped tests are existing conditional checks. The obsolete draft error key was removed from both en/zh catalogs.
- Final native Clippy after the unknown-context fallback: passed in 8.25s. Final native config/bridge tests: 15 passed. Final Vite build, TypeScript, Oxlint, Oxfmt and diff whitespace checks passed.
- No release, push, app replacement or native GUI launch was performed. Only this session's own Cargo targets are eligible for cleanup; the fixture's small Java target is retained for unified acceptance.

## Increment 1 — missing debugger / failed owned attach cleanup

Main's Mac preview found a missing-java-debug case after the initial isolated-env Run+JDWP launch: the JVM suspended successfully but action-dispatched attach failed without returning an outcome to its owner. This increment replaces that fire-and-forget boundary with `JavaAgentDebugger` in contributions.

- `prepare` probes the actual Java attach configuration / JDT debug server before spawning the owned JVM. Existing Microsoft java-debug install guidance remains available; no JVM starts when this preflight fails.
- Attach returns success/failure. Failure, cancellation or a 30-second attach timeout stops only the captured session + RunId. A later result cannot stop a restarted tab's newer process or another configuration. Stop during attach aborts its preparation and disposes a late-created native debug session.
- Run/Services explicitly show waiting for attach, failed attach with a still-live JVM, or historical attach failure/cancellation. Stop acknowledgment alone does not claim exit. Failed Stop leaves the run live with Stop/retry controls and an explicit message.
- Focused frontend regression: 73 passed, 6 existing catalog skips; includes real service preflight error handling, cancelled late native debug session, attach false/error/cancel/timeout, duplicate JDWP announcements, failed Stop retry and stale RunId isolation. TypeScript/Oxlint passed. No native build, UI control, installed app change or original demo was used for this increment.

## Increment 2 — owned Run output in Debug Console

Depends on increment 1, `ec30d2b436c45f5e21d03835d9bba6b8b405cba0` (already integrated by main as `5965626c`). This is a separate frontend-only increment for the successful Java8 owned attach whose Debug Console was empty in main's Mac preview.

- Owned Debug captures raw stdout/stderr before attach, retaining at most the configured console character cap or 1,048,576 characters, whichever is smaller, and at most 4096 chunks. Ordinary Run does not allocate this extra replay buffer. No output history is persisted.
- Once attach returns its actual Debug session ID, Run replays the retained startup output once, then streams new chunks through `JavaAgentDebugger.output` in contributions. Production features do not import each other's stores. Debug uses its existing batched console, ANSI and per-category partial-line handling. Run keeps receiving its normal output.
- The subscription captures the Run tab, native RunId and runtime generation. Stop, exit, close or reuse disconnects it; late old-process output cannot follow a restarted tab. The debugger service routes only to live actual IDs attached by that same preparation and matching process; closed, terminated and unrelated sessions receive nothing. Clearing Debug Console does not replay earlier history again. Output arriving during initial replay stays ordered and unique.
- Main's two `ServicesTree.debug.test.tsx` failures were reproduced before the fix. The lifecycle display hid an already observed HTTP port until Boot's final startup log. Run and direct Debug now show the observed port alongside the current startup phase, e.g. `Starting Spring Boot :8083`. They still require the actual `Started … (JVM running for …)` milestone to claim startup completion. Suspended/failed/stopped states retain priority, and split child output keeps its own execution's port. The tests now assert the combined startup label and explicitly reject a false completion claim.
- Regression includes the actual Run store → contributions service → actual Debug console store with native IPC mocked, in addition to replay/live stdout and stderr, ANSI, partial chunks, duplicate JDWP lines, simultaneous process isolation, Stop/restart, Run close, Debug close with pending output, terminated-session output, bounded replay and ordinary Run exclusion.

Validation: **97 suites passed; 1163 tests passed, 6 existing catalog checks skipped**, in 92.25s. This includes the complete seven-test `ServicesTree.debug.test.tsx` file and the real-store output bridge regression. Frontend/main, node, perf, scripts and smoke TypeScript checks passed; full Oxlint passed with existing unrelated warnings, full Oxfmt and Knip passed (existing ignore hint), Vite built in 5.86s with existing chunk/dynamic-import warnings, and `git diff --check` passed. The final independent commit ID is in the exported increment report.

No native build, preview UI control, installed app replacement, original demo launch, generated bindings change or other checkout write was performed. The integrated Mac preview still needs to verify the visible Debug Console and breakpoint flow with this frontend increment.
