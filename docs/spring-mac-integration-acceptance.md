# Spring integration: Mac / Java 8 acceptance

Local candidate: 0.1.41, plugin API 1.4. This report describes local evidence;
it does not assert that the candidate has been installed or published.

## Combined native application checks

Verified on this Mac in an isolated native development app using the integrated
source, production plugin host and packaged `hyhanyue.spring-beans` WASM.
Only disposable project copies were edited. The user's original project and
installed application were not replaced during these checks.

| Area                          | Observed result                                                                                                                                                                                                                                                                                                  |
| ----------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Launch context                | Selecting the dev and production run configurations changed the configuration source panel to the matching profiles and imported configuration groups. Blank native settings follow the launch context; explicit `default` selects the default profile.                                                          |
| Configuration navigation      | Opening `cycle.a` navigated to its source file. Go to declaration on a Java configuration field offered and opened the imported production property source.                                                                                                                                                      |
| Dependency Bean navigation    | The Bean panel included dependency Beans. Opening `applicationAvailability` displayed the attached read-only Spring Boot autoconfiguration JAR source.                                                                                                                                                           |
| Java 8 Run                    | The combined Boot 2.7.18 / MyBatis 2.3.2 / H2 fixture compiled and started using Java 1.8.0_421, selected profile `dev`, and an ephemeral loopback HTTP port. Stop terminated the owned process.                                                                                                                 |
| Actuator                      | Explicitly connecting to the test application's loopback endpoint displayed live Bean dependencies and MVC mappings. Locating a Bean opened `UserService.java`; locating `/users/{id}` opened the correct controller method.                                                                                     |
| MVC request generation        | Generate Request in HTTP Client created the controller request with a path variable. After setting the scratch request's ephemeral test URL, execution returned HTTP 200 and `{"id":1,"name":"Alice"}`.                                                                                                          |
| Java 8 Debug                  | After installing the official Microsoft Java debug adapter in the isolated app data, the owned JVM attached successfully. The line-25 controller breakpoint was verified and hit. Evaluating `id.longValue()` returned 1. Resume completed the pending HTTP request with 200; Stop terminated the owned process. |
| Configuration property rename | The native preview listed eight replacements in four files: field/accessors, Java callers/test, and `demo.greeting`. Cancel left files unchanged. Apply changed all four files; one editor Undo restored all four, checked on disk.                                                                              |
| Mapper rename                 | The native preview listed five replacements in four files, including the XML statement ID. Apply changed declaration, caller, tests and XML. Java 8 offline Maven tests passed after this actual GUI transaction; one Undo restored the original symbols and XML.                                                |

Native screenshots were inspected inline during these checks. No before/after
image files were saved, so this report does not claim an on-disk screenshot set.

## Integrated regression evidence

- Production Spring WASM launch-context test passes with the native settings
  schema, persistent overrides, profile changes, conflicting windows and deselection.
- Ten API compatibility tests pass, including an actual API 1.3 guest call on the
  API 1.4 host. All 17 packaged Rust components have current source fingerprints.
- Host unit tests: 181 passed, one ignored. Refactor production-adapter harness:
  17 passed, two ignored. Refactor planner: 21 passed.
- Spring plugin tests: 236 passed. MyBatis plugin tests: 37 passed, two ignored.
- Generated native bindings export and touched native packages' all-target
  clippy with warnings denied pass.
- Run/debug/plugin frontend regression: 1,229 passed. Combined Spring frontend:
  209 passed, six existing skips. Refactor frontend: 100 passed, six existing skips.
- Four resource staging tests pass. Spring Tools 2.3.0 and the stable personal
  plugin IDs are staged for 0.1.41.

Detailed scope and limits remain in the six domain progress reports. Static
source analysis cannot prove every runtime condition; the explicit runtime panel
provides observed application data when Actuator is available.

## Debug follow-up verification

The two issues exposed by native acceptance have been integrated as `5965626c`
and `a4fb798d`: adapter preflight / failed-attach cleanup, and the owned-process
Debug Console bridge. The final native test attached Java 8 successfully and
showed both the JDWP startup prefix and complete Spring Boot startup output in
Debug Console. A subsequent request caused DispatcherServlet initialization
logs to appear in the same console and returned HTTP 200 with the MyBatis data.
Stop terminated that exact test JVM with exit code 130.

The full frontend run passed 7,347 tests with six existing skips and exposed two
Services display regressions plus three sandbox loopback binding failures.
The three port tests passed when rerun with loopback access. After the Services
fix, the affected Run/Debugger/Services/catalog suite passed all 95 files and
1,147 tests with six existing skips. The independent run/debug chat also reports
97 files and 1,163 passing tests with six skips. These counts describe separate
runs, not a summed total.

## Remaining acceptance gates

Final package replacement and user review follow the release build.
Publishing remains conditional on the user's acceptance of the installed build.
