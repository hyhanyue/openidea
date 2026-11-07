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
- **Bean inspections** on Java files, run on open, save and typing, each group switchable in the settings: `@Transactional`/`@Async`/`@Cacheable`-style annotations on private, final or static methods, `@Scheduled` methods with parameters, bean names shared by two beans of the project (profiles and `@Conditional*` considered), private or final `@Bean` methods in proxied `@Configuration` classes, bean names starting with a capital, and (off by default) self-invocation of such annotated methods. Text-based: annotations imported from a non-Spring package and computed names are left alone; beans from libraries are not seen.
- **Unresolved references** (off by default, `springBeans.checkUnresolvedReferences`): a `@DependsOn("name")` no project bean defines (Java beans, `@Bean` methods, XML `<bean>`s and aliases all count), and a `@ComponentScan` package with no project source in it. Beans from libraries and auto-configuration are invisible to the plugin, so the check can over-report and stays off until you enable it. To keep it quiet: when the project declares `@SpringBootApplication` or `@EnableAutoConfiguration`, a `@DependsOn` name is only reported if it is project-shaped, meaning it equals (ignoring case) the name of a project class or bean (a typo, or a class never made a bean); a `@ComponentScan` is only checked for packages sharing their first two segments (`com.acme`) with project sources, and is skipped with `basePackageClasses`, placeholders or computed values.

Endpoints, Spring Boot run configurations and `@Value` key navigation are built into the IDE.

Matching is text-based, by simple type name. Beans from libraries and auto-configuration aren't seen, and conditions (`@ConditionalOn…`) are ignored, so the gutter may list beans that would not be active.

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
programmatic registration, component scan boundaries and auto-configuration can
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
