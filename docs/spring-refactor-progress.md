# Spring/MyBatis semantic refactor progress

Baseline: `57653df63a91789a3544436109ee288b7cfb0ba6`. Independent clone and branch `spring-semantic-refactor`; no source checkout modifications, no publish/install actions.

## Audit / reproduction

- `src/editor/lsp/rename.ts` calls JDT/XML/YAML rename, previews multi-file edits, then calls `applyWorkspaceEdit`. It does not query the Spring/MyBatis plugins.
- `spring-beans` and `mybatis` providers expose navigation, completion and actions, but no rename provider. Bean string identifiers, configuration paths and mapper XML statement identities are absent from this rename path.
- Existing plugin edits are prepared by the host; `Documents::prepare_workspace_edit` captures disk/dirty-buffer fingerprints. Apply preflights all operations, rejects versions/external writes, rolls back failures and records grouped undo/redo. This transaction layer must remain the only writer.
- Current preview lists file/count only. Source semantic rename needs visible replacement ranges plus an explicit scope warning, including single-file edits.

## Design boundary

Reuse `spring-text` Java/XML/config scanners in an independently testable, pure planner. Keep Bean names, configuration paths and mapper namespace/statement identities distinct. Only explicit source evidence is editable; ordinary strings/comments, custom annotations, SQL fragments and other modules are excluded. Java symbol changes are delegated to the existing LSP and merged before one transaction. No text-based Java call-site replacement.

Official sources reviewed:
- https://docs.spring.io/spring-framework/docs/5.3.31/reference/html/core.html (bean naming/qualifiers)
- https://docs.spring.io/spring-boot/docs/2.7.18/reference/html/features.html (external config/relaxed binding)
- https://mybatis.org/mybatis-3/sqlmap-xml.html (statement namespace/id and nested select references)

Native Mac app/UI verification is deliberately pending unified integration per the six-session build constraint. No screenshots or claims of UI execution yet.

## Implementation status (in progress)

- Added independent `plugins/support/spring-refactor` planner using existing spring-text parsers, and thin `ops/spring_refactor.rs` / `ops/spring_rename.rs` adapters. No new IPC types or generated bindings.
- Bean names: explicit @Bean/stereotypes, @Qualifier and @Resource literals, Spring XML references; implicit bean Java names use JDT plus decapitalized source references. Declaration qualifier tags and ambiguous bean names refuse.
- Config: application/bootstrap properties/YAML and @Value placeholders; ConfigurationProperties prefix + descendants. Moving YAML parent mappings or JavaBean leaf/accessor changes refuses; dynamic/relaxed-binding aliases remain explicitly outside preview scope.
- MyBatis: unique namespace+statement+Java method; nested-select XML references; overloaded methods/duplicate statements refuse. SQL fragments, identifiers, comments and unrelated namespaces stay unchanged.
- Preview: exact UTF-16 ranges, replacements and lazy before-text from dirty-buffer-aware fsReadText; explicit source scope; cancellable computation and discarded late responses.
- Host adapter compiles/exercises actual adapter and orchestration source in a lightweight independent harness (fake LSP only for orchestration failure tests). 8 planner tests and 8 adapter tests passing. 100 frontend tests passing, 6 existing localization skips.
- Found a real macOS transaction undo bug: fs::copy preserved original source mtime, so Maven incrementally reused renamed class files after undo (config fell back to 99 instead of 11). Refresh copied snapshot mtimes on macOS. All 8 workspace rollback/recovery unit tests passed after the fix. Dedicated mtime regression added.
- Real installed JDT LS 1.63 requires Java 25; Java 21 startup failed with an OSGi JavaSE=25 requirement. Reused installed Temurin 25 runtime read-only, with independent config/workspace and JavaSE-1.8 project runtime. Captured real JDT method rename edits for the declaration and Java caller; no Java source string replacement.
- Java8/Boot2.7.18/H2 fixture baseline, Bean apply+undo, config-key apply+undo, config-prefix apply+undo passed actual Maven compile/start/query. Final mapper combined transaction validation is underway. The initial XML test intentionally hit a comment and was correctly refused; test selector corrected to the actual statement.

No native app build/launch performed. UI/native integration remains pending unified Mac acceptance. No other chat messages sent.
