# Java 8 / macOS Spring workflow implementation

Baseline generic qualifiers: `8f1f543cbd4350ee544265baaa9a61b2c0c5bb50`.
The feature branch safely merged fork main `ad80b6c88b057313bc41ea269092739bef884cd3`.
No releases, tags, main branch writes or installed application changes are part of this task.

| Stage                               | State    | Acceptance                                                                                                                                                                                                                                              |
| ----------------------------------- | -------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 1. Eligibility / scanning           | Verified | Profile expressions, known property conditions, auto-config missing-Bean backoff for definite user registrations, scan packages and imported configurations; four Java 8 container tests, 195 plugin regressions and native provider-context tests pass |
| 2. Configuration links / precedence | Pending  | Reuse official metadata assistance; add source-chain and launch-context navigation                                                                                                                                                                      |
| 3. Local runtime Spring panel       | Pending  | Explicit loopback connection, read-only Actuator, masked configuration, source links and failure states                                                                                                                                                 |
| 4. Multi-module / dependency Beans  | Pending  | Bounded project-scoped JAR metadata; no third-party class initialization; parent/two modules/dependency fixture                                                                                                                                         |
| 5. Cross-file refactoring           | Pending  | Semantic Bean, config key and Mapper references through preview, version checks and workspace undo                                                                                                                                                      |

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
