# Java 8 Spring semantic refactor fixture

Boot 2.7.18, MyBatis starter 2.3.2, H2, no web server or TCP port.
Run on a **copy** with JDK 8 and Maven: `mvn -o test`.

`RuntimeTest` starts the actual Boot context, verifies Bean/Qualifier injection,
@Value and ConfigurationProperties binding, mapper queries and nested-select XML
references. An unrelated mapper method, SQL fragment, SQL alias, ordinary strings
and comments deliberately share names. They must survive each rename.

The independent Rust host harness applies framework plans through real Documents
transactions, runs Maven after apply, performs grouped undo, and runs Maven again.
Mapper Java edits must come from a real JDT LS response, supplied using
`OPENIDEA_JAVA_EDITS`; the harness never globally replaces Java names.

JavaBean leaf coverage adds `Settings.displayName`, its getter/setter, three
resource spellings, `@Value` references and typed Java callers. The unrelated
class deliberately has the same field/accessor names. `BindingAliasesTest`
uses real JavaBeans descriptors and Boot Binder to verify each resource spelling
independently, before and after refactoring.

Capture property responses with the final `property` argument to
`scripts/spring-refactor-jdt.mjs`; set `OPENIDEA_PROPERTY_EDITS` to that JSON and
run the ignored `java8_property_field_accessors_and_resource_aliases_apply_and_undo`
host-harness test. It replays those real JDT responses through the production
orchestrator, applies one Documents transaction, runs Maven, undoes the group,
and runs Maven again for all nine declaration/resource/usage entry points.
