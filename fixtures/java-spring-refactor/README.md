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
