# Spring Runtime progress

Baseline `57653df63a91789a3544436109ee288b7cfb0ba6`, independent branch `feat/spring-runtime`.
Checkout `/Users/hyl/Documents/Codex/2026-11-07/openidea-spring-runtime/work/openidea`.

Phase 1 complete: read-only loopback Actuator transport, backend allowlist redaction,
endpoint-specific errors, cancellation/limits, thin local-only IPC adapter and isolated
Java 8 / Spring Boot 2.7.18 fixture. No protected MVC files, shared checkout, port 18088,
user application configuration, installed app, release or remote branch changed.

Official Spring Tools has custom `sts/livedata/*` commands and HTTP support. Inspected
`SpringProcessCommandHandler`, `HttpActuatorConnection` and
`SpringProcessLiveDataExtractorOverHttp` in the upstream `livehover/v2` package.
The current HTTP extractor reads env first, returns null on the outer exception path,
and does not provide configprops or independent endpoint HTTP/error/redaction semantics.
The existing local `lspExecuteCommand` also discards response data. Standard LSP hover
remains available; a dedicated native boundary is required for this panel's contract.

Sources:
- https://github.com/spring-projects/spring-tools/tree/main/headless-services/spring-boot-language-server/src/main/java/org/springframework/ide/vscode/boot/java/livehover/v2
- https://docs.spring.io/spring-boot/docs/2.7.18/actuator-api/htmlsingle/

Verification:
- `cargo test -p ide-http --offline`: 29 passed, 2 fixture-only tests ignored by default.
- Both ignored tests explicitly passed with `fixtures/spring-runtime-java8/verify.py`:
  actual five-endpoint JSON projection/redaction plus actual Spring Security 401/403 and
  a disabled env endpoint (404). Ephemeral loopback ports and credentials; children stopped.
- `cargo clippy -p ide-http --all-targets --offline -- -D warnings`: passed.
- Java 8 Maven package passed. Missing public security dependencies were resolved into an
  isolated cache, not written into the existing shared Maven cache.

Phase 2 Services panel implementation is in the working tree, with focused frontend tests.
Native command compilation, `pnpm gen:bindings` and macOS GUI acceptance are intentionally
reserved for the main session's unified native build. No GUI results/screenshots claimed.
