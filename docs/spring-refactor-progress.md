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
