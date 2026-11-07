# Java 8 Spring dependency model oracle

A Maven parent with exactly two reactor modules (`api`, `app`) and a separate
binary library. Uses Spring Boot 2.7.18 / Framework 5.3.31 and Java 8 bytecode.
No HTTP server or port is opened.

```sh
python3 fixtures/java-spring-dependencies/verify.py /absolute/isolated/oracle
# Import /absolute/isolated/oracle/project in OpenIDEA as a Maven project.
# The library sources are outside the reactor; vendor.jar is the build dependency.
# Optional: attach project/lib/vendor-sources.jar through the existing Java source attachment UI.
```

`verify.py` reads cached Spring jars from `~/.m2/repository` and writes everything
into the chosen output directory. It does not install an artifact or modify the
cache. Override `JAVA8_HOME` / `MAVEN_CACHE` if needed. The direct `systemPath`
dependency makes the fixture reproducible without publishing the vendor JAR.
The separate `library/` tree exists solely to rebuild that binary.

The container oracle verifies cross-module configuration and interfaces, a JAR
`@Bean` factory and alias, dependency component scanning, both
`AutoConfiguration.imports` and legacy `spring.factories`, and excludes an
unscanned component plus property/class-condition failures. It is distinct from
the indexer: real Spring intentionally executes fixture factories, whereas the
indexer's test sets a property that makes the library's static initializer throw
if application classes are ever initialized.

Run the real Maven/JDT model test with Java 25 on PATH (the language server's
runtime), after the oracle. The test uses an **isolated** Maven repository at
`../maven-repository` relative to the oracle output; copy the local dependency
cache there, or configure the script's `repo` path. It imports the parent and
two modules using m2e, resolves classpath identities, reads Java 8 class files,
checks binary/decompiled and attached-source targets, and invalidates on unsaved
imports. The app itself still uses Java 8.

```sh
python3 java/java-usages/build.py /path/to/jdtls
python3 java/java-usages/tests/spring-beans.py /path/to/jdtls /absolute/isolated/oracle
```

Desktop acceptance remains separate: open `App.java`, verify injection markers
and definitions for `Port`, `Token`, `ModuleValue`, then inspect Spring Beans and
Spring Bean Graph. The JAR factories must show their dependency source and
unknown conditional registrations must be labelled as candidates.
