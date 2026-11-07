# Mapper method → XML intention

`src/generate.rs` independently implements the method-to-statement workflow
associated with [MyBatisX](https://mybatis.plus/guide/mybatisx-idea-plugin.html).
No MyBatisX source is copied. XML parsing uses the MIT-licensed `quick-xml`
crate through Cargo. Parameter/return conventions follow the official
[Mapper XML reference](https://mybatis.org/mybatis-3/sqlmap-xml.html) and
[ParamNameResolver](https://mybatis.org/mybatis-3/xref/org/apache/ibatis/reflection/ParamNameResolver.html).

## Integration

This branch deliberately leaves `plugin.toml` and existing completion code
unchanged so the SQL-completion branch can be integrated independently.
Add `"code-actions"` to the existing Java/XML provider's `kinds`, or add:

```toml
[[contributes.providers]]
kinds = ["code-actions"]
languages = ["java"]
```

Keep the existing required `project.read` capability. No new commands,
settings, core SDK changes, `editor.write` or filesystem-write capability are
needed: the intention returns a `CodeAction` workspace edit to the host. It
never writes project files directly. The existing `Navigator` provider
registration already exports the resolver.

Build the merged plugin's WASM and refresh its packaged source hash using the
normal plugin build process. This branch does not replace `dist/plugin.wasm`.
Only the integration session should install the plugin or operate the Mac GUI.

## User flow

Place the caret in, or select, an abstract Mapper method declaration, then use
OpenIDEA's intentions popup (Option+Enter on macOS). Choose
`Generate MyBatis XML statement for <method>`. The action resolves into one
insertion before the matching XML's `</mapper>` and uses the editor's normal
workspace-edit preview, application and undo flow.

The discovery call uses only the supplied Java snapshot, with no host calls or
project scans. Namespace lookup, XML validation and entity reads happen when
the action is resolved, in a cancellable SDK task. A mapped method can still
appear in the initial menu; resolution refuses it with an “already has an XML
statement” error rather than duplicating it.

The action checks source path, version and content fingerprint, re-reads source
and XML before returning, and leaves final preview/application preconditions to
the host. `Project::read_file` includes unsaved editor content. The SDK does not
carry target-file versions in returned provider edits, so the final re-read and
host-captured preconditions are the strongest available plugin-only protection;
there is a small gap between them. No core changes are included.

## Generated templates and boundaries

- Supports select/find/get/count (and unknown-name select fallback),
  insert/add/save/create, update/modify/set and delete/remove method prefixes.
  These are editable SQL templates, not database-derived production queries.
  All table and column identifiers remain explicit `TODO_TABLE` /
  `TODO_COLUMN_n`; SELECT/UPDATE/DELETE retain a `WHERE 1 = 0` guard until edited.
- Uses `@Param("name")`, `@Param(value = "name")` and qualified annotations.
  Multiple unannotated parameters use `param1`, `param2`, etc., independent of
  Java compiler `-parameters`. MyBatis `RowBounds` / `ResultHandler` parameters
  are excluded. Explicit alias collisions produce an unresolved-binding TODO.
- Resolves scalar return types, explicit imports, same-package source types,
  `List<User>` / collection / optional / cursor element types and arrays.
  Unknown types, wildcards or type variables require an explicit resultType or
  resultMap TODO. There is no classpath/JDT type inference.
- A single unannotated source bean exposes `#{field}`; named/multiple beans
  expose `#{alias.field}` / `#{paramN.field}`. Reads direct instance fields only
  (at most 24); does not infer inherited properties or schema column mappings.
  Maps/unknown object types leave a named TODO instead of inventing keys.
- Collection/array predicates use `foreach`, with `list`, `collection`, `array`
  or the explicit/multiple-parameter alias. Object collections suggest the first
  declared property; review that choice. Batch INSERT/UPDATE mapping is an
  explicit editable TODO rather than a fabricated working batch implementation.
- Skips SQL-annotated, default/static/private, generic-method and overloaded
  declarations. Handles comments, literals, annotations and multiline signatures;
  this is a bounded Java lexer, not a full Java parser. The first interface in
  the source file is the target; inherited/nested mapper declarations and
  multi-method generation are outside this version.
- Requires one existing non-self-closing XML root with the exact namespace.
  Does not create or expand an XML file: the provider SDK exposes only text
  edits, not file-creation operations. Missing, malformed, oversized or duplicate
  namespace mappings fail without edits. Any existing CRUD id, including a
  databaseId variant and escaped/single-quoted attributes, blocks generation.
- Preserves original content, newline style, base indentation and UTF-16 edit
  coordinates. Comments/CDATA do not count as statements. External MyBatis DTD
  declarations are retained without network access; internal DTD/entity subsets
  are unsupported. Matching XML is limited to 2 MiB; discovery is capped at
  64 KiB of Java and the resolver/entity parser at 256 KiB. Project lookup is
  bounded at 1000 XML files and fails rather than scanning a truncated set.

## Verification

From the repository root (with this machine's Rust toolchain on PATH):

```sh
cargo test --offline --locked --manifest-path plugins/official/mybatis/Cargo.toml
cargo fmt --manifest-path plugins/official/mybatis/Cargo.toml -- --check
cargo clippy --offline --locked --manifest-path plugins/official/mybatis/Cargo.toml --all-targets -- -D warnings
cargo build --offline --locked --manifest-path plugins/official/mybatis/Cargo.toml --target wasm32-wasip2 --release
```

The optional real-project fixture test reads the shared Java 8 project, adds
five temporary Mapper declarations only in mock editor memory, generates and
re-parses XML, verifies both existing mappings, rejects a repeated generation,
and asserts the original Java/XML files are unchanged:

```sh
OPENIDEA_MYBATIS_JAVA8_FIXTURE=/absolute/path/to/spring-mybatis-java8-demo \
  cargo test --offline --locked --manifest-path plugins/official/mybatis/Cargo.toml \
  generate::tests::java8_project_read_only -- --ignored --exact
```

Real GUI preview/application/undo and a runnable SQL check after replacing the
TODO identifiers remain integration-session checks. Rust tests do not claim to
validate the Mac GUI or execute the generated SQL against H2.
