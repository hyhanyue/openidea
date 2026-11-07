# Spring Run / Debug progress

Baseline: `57653df63a91789a3544436109ee288b7cfb0ba6`; independent clone and branch `codex/spring-run-debug`.

## Audit

- Existing `RunConfig` saves args, params, env, cwd, module, env files and inheritance in project/local TOML. Application/Spring Boot forms already expose profiles, VM/program args, JRE, module.
- Run/Debug share `prepareJavaLaunch`; backend selects Project JDK separately from JDT tooling JDK. Explicit run JRE wins.
- Java 8 (1.8.0_421) real JDWP probe `java -agentlib:jdwp=transport=dt_socket,server=y,suspend=n,address=127.0.0.1:0 -version` succeeded. Initial sandbox denied socket bind; elevated local probe succeeded. No speculative JDWP rewrite.
- Missing: Maven Boot dedicated application args and application cwd; selected launch context does not reach Spring Beans settings; Boot lifecycle lacks startup/ready/failure facts.
- Stop uses owned RunId and process group; rerun waits for old run exit. No process-name kills.
- Update already distinguishes resource copy, debugger HotSwap and restart. No dependency injection / DevTools installation needed.

## Validation and commits

In progress. No application rebuild or GUI claim. No changes to installed app, main, release, original demo or port 18088.

## Integration boundary (current implementation)

- Run feature publishes `useSpringLaunchContext` in `src/platform/springLaunchContext.ts`; consumers use platform services rather than importing the Run feature.
- `src/features/plugins/springLaunchContext.ts` resolves the **actually enabled** known Spring Beans package: stable `hyhanyue.spring-beans`, source `aritti.spring-beans`, migration `hyhanyue.spring-beans-local`. It does not invoke guessed or arbitrary IDs. The host also restricts the reserved bridge command to those aliases and an active/inactive installed package.
- Existing IPC `pluginRunCommand(id, "openidea.setLaunchContext", JSON.stringify({projectRoot, context}), null, [])` transports a sanitized context. This is a host-reserved command, not a guest plugin command. No IPC/WIT serialization schema changes or generated-bindings edit.
- `context` is `null` to **remove the temporary layer** on deselection / unsupported unknown launch. Persistent user plugin settings remain intact. A selected Java application with an unresolved profile instead sends `activeProfiles: null`, never the previous selection's profile.
- Context shape: `{applicationClass: string|null, activeProfiles: string|null, propertyOverrides: Map<string,string|null>, environmentComplete: false}`. **propertyOverrides is an object map, not the old properties-text setting string, and not the other session's draft JSON-in-a-string envelope.** This is the integration contract for `environment.rs` to consume. Arbitrary string/number values and sensitive keys have null values; only non-sensitive boolean switches are carried. `null` means unknown, never missing/false.
- Host overlays exactly `springBeans.applicationClass`, `springBeans.activeProfiles`, `springBeans.propertyOverrides`, `springBeans.environmentComplete`; it never writes the user's settings. Map replacement is atomic. Runtime snapshots all overrides per guest invocation, avoiding mixed generations across multiple setting reads.
- Context storage is `(window, project)` scoped and cleared on window close. Background editor providers currently have no window identity; they use a root's facts only when all windows agree, otherwise all launch facts become unknown. Views keep the invoking window's facts. Late requests for another project are rejected by the host root guard.
- Setting revisions invalidate in-flight markers/views; marker and Spring view requests wait for synchronization. Host navigation contributions are invalidated after context changes.
- Explicit CLI properties beat VM/environment values. Repeated Boot CLI properties combine comma-separated values. Inherited environments, env files, JVM option env variables, JSON and dynamic substitutions keep unsupported facts unknown; this adapter never claims a complete Spring Environment.

## Verified so far

- Frontend focused launch/SDK tests: 68 passed (before subsequent extra lifecycle cases).
- `cargo test -p ide-run --test config --test spring_launch_context --offline`: 14 passed (before extra late-project request case).
- Java8 fixture compile succeeded offline using cached Maven compiler; dependency plugin 3.3.0 was incompletely cached, so classpath generation used cached `org.apache.maven.plugins:maven-dependency-plugin:3.8.1:build-classpath` successfully.
- Real Java8 `ide-run` regression passed in 14.60s: shared/local config persistence, dev/prod HTTP response, env/VM/args/cwd, actual JDWP suspend/resume, stop/restart isolation, missing Bean, ambiguous Bean, binding and occupied-port failures. macOS `/var` → `/private/var` canonicalization is compared as paths. Additional env-file/Maven cases are in progress.
- No full environment is printed or written to this report. The probe endpoint returns only synthetic test markers, runtime version, cwd and a boolean for inheritance.
- Existing application update differentiates class/resource publication, debugger HotSwap and restart; no DevTools dependency is added. No Initializr/starter creation entry was found in the inspected source; no download flow added.

## Pending unified Mac acceptance

Native UI build/launch, screenshots, Java debugger breakpoint/variable inspection and stable-ID real WASM integration remain for the main session's combined Mac acceptance. The JDWP regression proves the real transport and suspended/resumed Java8 app, not the full IDE DAP/GUI flow. No claim of GUI completion.
