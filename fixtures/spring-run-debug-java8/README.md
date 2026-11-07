# Java 8 launch fixture

Independent source copy of the authorized Spring/MyBatis demo, with an allowlisted
`/launch-probe` endpoint and profiles `missing`, `ambiguous`, `binding` for startup failures.
It is a test project, not an application template. No DevTools dependency.

Always pass `--server.port=0`; never use the original demo's port. Build:

```sh
JAVA_HOME=/Library/Java/JavaVirtualMachines/jdk-1.8.jdk/Contents/Home "/Applications/IntelliJ IDEA.app/Contents/plugins/maven/lib/maven3/bin/mvn" -o -f fixtures/spring-run-debug-java8/pom.xml compile org.apache.maven.plugins:maven-dependency-plugin:3.8.1:build-classpath -Dmdep.outputFile=target/classpath.txt
```

Then run the opt-in `ide-run` test with `OPENIDEA_JAVA8_FIXTURE` pointing to this folder
and `JAVA_HOME` pointing to Java 8. Set `OPENIDEA_MAVEN` to the absolute `mvn` executable
to include the real Maven Run/Debug cases. It tests the actual ProcessManager, persisted configs,
HTTP responses, two profiles, env/VM/program args/cwd, JDWP suspend/resume, restart and
failure paths. It does not replace the unified Mac GUI / DAP breakpoint acceptance.
