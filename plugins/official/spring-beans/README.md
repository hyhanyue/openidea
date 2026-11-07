# Spring Beans Navigator

Spring bean support for Java, Kotlin and Spring XML files in OpenIDEA:

- **Beans**: stereotype classes (`@Service("name")`…), `@Bean` methods (names and aliases), Spring Data repository interfaces, XML `<bean>` definitions and `<alias>`es, with `@Primary`, `@Profile` (and `<beans profile>`) and `@Qualifier`.
- **Injection navigation**: a gutter arrow on each injection point opens the bean Spring would inject, and Go to Definition on the type lists the candidates. Candidates are matched by type through the project's type hierarchy, then narrowed by `@Qualifier`/`@Named`/`@Resource(name)`, `@Primary` and the parameter or field name. `List<T>`, `Map<String, T>` and arrays take every `T`; `Optional<T>`, `ObjectProvider<T>` and `Provider<T>` take a `T`. Covered: constructor parameters of beans, `@Autowired`/`@Inject`/`@Resource` fields and setters, Lombok-constructor `final` fields, `@Bean` parameters, Kotlin primary and secondary constructors and `lateinit` properties, XML `ref`/`parent`/`depends-on`/`factory-bean`.
- **Where a bean is injected**: a gutter icon on each bean lists the injection points that receive it.
- **Bean names in strings**: navigation and completion in `@Qualifier`, `@Named`, `@Resource(name)`, `@DependsOn`, `getBean("…")`, XML references, SpEL `@bean` references; XML `class` attributes navigate to and complete project classes.
- **SpEL**: `#parameter` completion in `@Cacheable`/`@CachePut`/`@CacheEvict` keys and conditions, `@EventListener` conditions and `@PreAuthorize`-style expressions.
- **AspectJ pointcuts** in `@Pointcut`/`@Before`/`@After…`/`@Around`: designator completion, named pointcut completion and navigation (`inApp()`, `Aspects.inApp()`), bean names in `bean(…)`.
- **Spring Beans** tool window, grouped by kind, and a **Spring Bean Graph** of who depends on whom.
- **Field injection** warning on `@Autowired`/`@Inject` fields, with a _Use constructor injection_ quick fix that adds the constructor, or extends the class's only one (not offered with several constructors, `@Autowired` setters or an unparsable body). XML `p:`/`c:` `*-ref` attributes (prefixes bound to the Spring `p`/`c` namespaces) are followed like `ref`; a Kotlin `@Bean fun x() = Foo(…)` is typed `Foo`.
- **Bean inspections** on Java files, run on open, save and typing, each group switchable in the settings: `@Transactional`/`@Async`/`@Cacheable`-style annotations on private, final or static methods, `@Scheduled` methods with parameters, bean names shared by two beans of the project (profiles and `@Conditional*` considered), private or final `@Bean` methods in proxied `@Configuration` classes, bean names starting with a capital, and (off by default) self-invocation of such annotated methods. Text-based: annotations imported from a non-Spring package and computed names are left alone; resolved dependency registrations supplement the source model when JDT is ready.
- **Unresolved references** (off by default, `springBeans.checkUnresolvedReferences`): a `@DependsOn("name")` no project bean defines (Java beans, `@Bean` methods, XML `<bean>`s and aliases all count), and a `@ComponentScan` package with no project source in it. Dependency and dynamic registrations can remain incomplete, so the check can over-report and stays off until you enable it. To keep it quiet: when the project declares `@SpringBootApplication` or `@EnableAutoConfiguration`, a `@DependsOn` name is only reported if it is project-shaped, meaning it equals (ignoring case) the name of a project class or bean (a typo, or a class never made a bean); a `@ComponentScan` is only checked for packages sharing their first two segments (`com.acme`) with project sources, and is skipped with `basePackageClasses`, placeholders or computed values.

Endpoints, Spring Boot run configurations and `@Value` key navigation are built into the IDE.

Source matching uses the source context and generic model below. For Java, resolved JDT classpaths also contribute referenced Maven-module and dependency-JAR registrations. Binary matching uses fully qualified identities; unresolved generic qualifiers or runtime conditions remain candidates, and the gutter does not claim a unique injection for them.

This is not a runtime container inspection.

```sh
cargo test
openidea-plugin build
openidea-plugin link
```

## Personal fork additions

Java project-local missing and ambiguous Bean injection warnings are enabled by
`springBeans.checkInjectionResolution`. Constructor, field and setter injection
use the same candidates as navigation, including qualifiers, primary beans and
MyBatis `@Mapper` interfaces. An ambiguous gutter has no arbitrary first target;
Go to Definition lists the candidates. Optional/provider/collection injections,
library types and profile/condition-dependent candidates are not diagnosed.
This is a conservative source index, not a complete Spring application context:
programmatic registration, custom scan filters and auto-configuration ordering can
require runtime verification.

Concrete Java generic qualifiers now narrow candidates before primary/name
selection, including `@Bean` return types, nested arguments and substitutions
through project-defined generic superclasses/interfaces. Collections and
providers use the same generic candidates as navigation, reverse references and
the dependency graph. Explicitly imported or qualified argument names are
preserved. Raw types, wildcards, unresolved variables and unavailable library
hierarchy retain uncertain candidates and suppress resolution warnings; they do
not implement Spring's complete runtime fallback/type inference. Kotlin and XML
generic inference is unchanged. See `docs/spring-boot-parity-java8.md` in the
source repository for verified cases and remaining parity gaps.

The checked-in Spring gutter icons are independently drawn. The local Mac
preview can substitute original assets from the user's IDEA installation; those
commercial assets are kept outside the source tree. Upstream provenance is in
UPSTREAM.md; its MIT license is preserved.

### Selected source context

The application class, active profiles and known property overrides can be set
per project in the Spring Beans settings. With one Boot application it is
selected automatically. Profile expressions, literal `ConditionalOnProperty`,
component scan packages and imported configuration classes narrow candidates
consistently in the Bean list, graph, definitions, gutter and diagnostics.
Known user registrations make corresponding `ConditionalOnMissingBean` factory
methods in auto-configurations back off. Unknown environment values, dynamic
scan/import logic and unsupported condition ordering remain labeled uncertain.
The source view never claims to be the runtime application context.

Profile settings are comma-separated; an empty value means `default`, while an
unset value means unknown. Property overrides are `key=value` lines, with `!key`
for a property known to be absent. Only mark the environment complete for a fully
specified development launch with no inherited or external values. Native editor
providers now read the owning project's settings, matching the tool windows.

## Resolved Java dependencies (API 1.4)

A trusted project's existing JDT build model supplies sibling Maven source roots,
explicitly imported or scanned library components, configuration factory methods
(including inherited methods), and candidates from Boot 2.7 auto-configuration
metadata. Merely placing a `@Component` in a dependency does not activate it.
Known missing-class conditions are excluded; property/profile/order-dependent
conditions, dynamic registrars and unresolved generic qualifiers stay unknown.
Definition, gutter, Bean tree and dependency graph share these candidates and
show their module/JAR origin. Attached source navigation and the existing read-only
decompiler are reused. Discovery runs in a cancellable background request with
archive, entry, type and nesting limits; plugins gain no arbitrary `.m2` reads.

Java 依赖模型要求 API 1.4：只使用受信任项目已经解析的 JDT 构建路径，补充 Maven
依赖模块、扫描或导入的 JAR 组件、配置工厂方法和 Boot 2.7 自动配置候选。仅存在于
依赖里的 `@Component` 不会自动生效。运行时条件、动态注册与尚未解析的泛型限定保持
未知；定义、gutter、Bean 列表和依赖图共用候选并显示模块或 JAR 来源。发现工作在
可取消的后台完成，不授予插件任意读取 `.m2` 的能力。

### Configuration sources and Java links

The Spring Configuration Sources view lists source origins, overridden declarations,
placeholder chains and Java binding locations without displaying property values.
It follows Java annotation placeholders and `@ConfigurationProperties` fields back
to their configuration declarations. Definition support in properties/YAML also
follows placeholders and nested defaults. Spring Tools retains configuration
completion and validation; source-only key-to-field navigation is opt-in through
`springBeans.configurationBindingFallback`.

The source environment models Boot 2.7 default location groups, profile-file order,
activated multi-documents, project-local classpath/file imports, optional and missing
imports, load-once cycles, includes and nested profile groups. Imported profile
variants have Boot's precedence over ordinary profile files in their location group.
Ordinary imported documents remain beneath later documents in their enclosing file.
Unknown source overlaps, complex syntax, outside-project paths and remote providers
are reported without guessing or fetching external content.

The first-party Run bridge supplies the separate, transient
`springBeans.launchContext` v1 object. It is not a user setting or a persisted
property override. Saved launches are recovered in memory only from the current
project's `.openidea/run.toml` / `run.local.toml`, by exact selected ID and matching
reference fields; the shared file wins duplicate IDs. Unsaved/redacted unknown
facts block lower-priority values instead of being interpreted as known absence.
Simple authorized env files, explicit environment, JVM system properties, application
JSON and command-line properties retain their source precedence; manual analysis
overrides remain last. Inherited process environment is never enumerated. Values
are never written to settings, logs or the source view. See
`docs/spring-context-config-progress.md` for the shared integration contract,
Java 8 acceptance and pending integrated Mac checks.

Plain imported classes and `@EnableConfigurationProperties` / project property scans
participate in the Bean index, including constructor dependencies. Property-class
registration names use Boot's prefix-and-qualified-name convention. Explicit
`@EnableConfigurationProperties` registrations follow their enabling configuration's
eligibility; the property class's own `@Profile` does not filter this registrar.
Dynamic selectors/registrars remain uncertain.
