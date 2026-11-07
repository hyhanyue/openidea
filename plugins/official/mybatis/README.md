# MyBatis Navigator

Navigation, mapper statement generation and local-schema SQL completion for OpenIDEA.

## Generate a missing XML statement

Put the caret on a Java mapper method and choose **Generate MyBatis XML statement for …**
from intentions (Option+Enter on the IntelliJ macOS keymap). The action inserts a statement
into the project's existing mapper XML with the matching namespace. The host applies the
workspace edit through its normal version-check and undo pipeline.

The template includes the statement kind, query result type and parameter bindings. Review
and replace the explicit `TODO_TABLE` / `TODO_COLUMN` placeholders before running it. The
plugin does not infer business SQL from a method name. Missing/ambiguous XML, existing ids,
overloaded names and unsupported signatures are rejected rather than creating another mapping.

## Complete SQL

Use normal completion in mapper SQL text for keywords, tables and columns, including
`SELECT u.na FROM users u`. Table and column metadata comes from project `*.sql` CREATE TABLE
statements, with `schema.sql` prioritized. Existing statement-id and `#{parameter}` completion
and Java/XML navigation remain available.

There is no database connection. This first version does not evaluate ALTER/DROP migrations,
CTE/derived-table projections, includes, or dynamic SQL branches. Duplicate CREATE definitions
merge columns. Quoted SQL identifier editing and unsupported contexts are conservatively skipped.

Metadata is cached per project. Existing files refresh on subsequent SQL requests after two
seconds; newly created files are rediscovered after thirty seconds. Discovery retains at most
128 SQL files, 256 tables and 256 columns per table, skipping files/documents over 512 KiB.
Refresh work is sliced between synchronous SDK reads, so an individual slow host read can
still exceed the editor budget. Large projects may need several requests to populate metadata.

## Local development

This repository's `openidea.mybatis` id is reserved for its signed official distribution. A
personal test package should use a distinct local id and name, and be loaded using the supported
`openidea-plugin link <folder>` workflow. Disable the installed MyBatis Navigator while testing
that package to avoid duplicate providers. This development version has not been published to
the marketplace.

## MyBatisX bird icons

`*Mapper.java` and `*Mapper.xml` use dedicated file/tab icons regardless of icon theme.
These are filename patterns, not content-based Mapper detection. Other Java/XML files
keep their normal icons. Navigation markers use the matching target's bird: red from
Java to XML, blue from XML to Java, as in MyBatisX. This requires the updated desktop
renderer; an older OpenIDEA app falls back to its built-in navigation glyph.

The two bird SVGs are adapted from [baomidou/MybatisX](https://github.com/baomidou/MybatisX),
licensed under Apache-2.0 (the accompanying LICENSE). Original paths:
`src/main/resources/images/statement.svg` (Git blob `51fd725bb8a6856b815425478f437660e9225ced`)
and `mapper_method.svg` (Git blob `af2ce866c6c850c3737bced5a660b7e8eeda4cc7`).
Modifications inline class fills, normalize local ids and remove editor metadata for
OpenIDEA's existing SVG sanitizer. Geometry and colors are retained. No MyBatisX code
is incorporated and this local package does not claim official MyBatisX affiliation.
