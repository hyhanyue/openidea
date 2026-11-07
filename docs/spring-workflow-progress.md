# Java 8 / macOS Spring workflow implementation

Baseline generic qualifiers: `8f1f543cbd4350ee544265baaa9a61b2c0c5bb50`.
The feature branch safely merged fork main `ad80b6c88b057313bc41ea269092739bef884cd3`.
No releases, tags, main branch writes or installed application changes are part of this task.

| Stage                               | State                         | Acceptance                                                                                                                                                                                                                                              |
| ----------------------------------- | ----------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 1. Eligibility / scanning           | Verified                      | Profile expressions, known property conditions, auto-config missing-Bean backoff for definite user registrations, scan packages and imported configurations; four Java 8 container tests, 195 plugin regressions and native provider-context tests pass |
| 2. Configuration links / precedence | Source verified / GUI pending | 224 plugin / 40 parser / 8 real Java 8 / 14 frontend tests; provenance, imports/includes/groups, placeholders, Java bindings and shared launch contract. SDK 1.4 WASM/GUI acceptance belongs to the parent                                              |
| 3. Local runtime Spring panel       | Transferred                   | Explicit loopback connection, read-only Actuator, masked configuration, source links and failure states                                                                                                                                                 |
| 4. Multi-module / dependency Beans  | Transferred                   | Bounded project-scoped JAR metadata; no third-party class initialization; parent/two modules/dependency fixture                                                                                                                                         |
| 5. Cross-file refactoring           | Transferred                   | Semantic Bean, config key and Mapper references through preview, version checks and workspace undo                                                                                                                                                      |

Stage 1 source analysis settings (project overrides global):

- `springBeans.applicationClass`: fully qualified application class; unset selects the single Boot application.
- `springBeans.activeProfiles`: comma-separated profiles; empty selects `default`; unset remains unknown.
- `springBeans.propertyOverrides`: known `key=value` lines, or `!key` for known absence.
- `springBeans.environmentComplete`: only enable for a fully described launch with no unseen environment/config sources.

An inactive candidate is excluded consistently from definitions, gutter, Bean list and graph.
An unknown candidate stays visible with an eligibility label, suppresses missing/ambiguous
resolution warnings and cannot be collapsed using primary/name selection. A sole uncertain
candidate is navigable through the candidates list rather than a definite injection arrow.
`ConditionalOnMissingBean` backoff is only definite for an auto-configuration with an
unconditionally registered user Bean of the requested project type/name. Absence and arbitrary
condition processing order remain unknown. Complex scan filters, dynamic selectors/registrars,
missing imports and unsupported conditions remain uncertain.

Desktop acceptance exposed a native runtime defect: editor providers used an empty Invocation,
so their settings came from the global scope even when a tool window used project settings.
Provider calls now use the document's longest owning project and editor context; a path-component
boundary test prevents `/app` from claiming `/application`. This also fixes project-specific
settings for completion, definitions, actions, inline completions and semantic tokens.

Evidence is saved under this task's `outputs/stageN-*` directories, outside the source commit.
The original Java 8 demo and port 18088 remain untouched.

Stage 1 desktop verification: on the same isolated Java sources, baseline
WASM lists `prodPort` and `outsidePort` as primary candidates for the dev Client.
The new WASM, using the project's `dev` analysis profile, lists six in-scope Beans
and points the injection to `DevPort`. The imported clock configuration is present,
while the prod/outside/fallback Beans are absent. No original application, original
demo, installed app or port 18088 is involved.

Stage 1 validation: 195 Spring Beans tests; 22 native plugin runtime/compatibility
checks; four real Java 8 contexts; full frontend suite (7239 passed initially,
three sandbox-denied localhost tests then passed in the five-test socket recheck,
six skipped); four focused marker tests; three Settings E2E tests in installed
Chrome; typecheck, build, format, knip and focused Rust clippy. Lint reports only
existing VCS hook-dependency warnings. The available WebKit runtime differs on an
unrelated existing font serialization assertion; the original Chrome expectations
pass unchanged. Screenshots show actual gutter clicks and live dev→prod changes.

Ownership update (user-directed six-chat split): this checkout now owns stages 1
and 2 only. Stages 3–5 are assigned to other independent chats; the parent chat
owns MVC route parsing and `crates/ide-host/src/ops/routes.rs`. None of those
paths or follow-on implementations will be changed here. Further heavy native
builds and desktop acceptance are deferred to the parent's integration pass.
Stage 1 already has standalone native evidence. Stage 2 UI acceptance remains
pending even when its targeted tests pass.

Stage 2 source commit: `5a157dd2586a0721748cb74b5cb6f80e4899f722`. The effective selected entry point is available
through `environment::snapshot_project(project, files, current).application`; the
parent dependency snapshot should use this instead of the persistent application
setting alone. Full protocol and limits: `docs/spring-context-config-progress.md`.
The parent explicitly requested unified SDK 1.4 builds, so no old-SDK WASM or
additional native app was built here.
