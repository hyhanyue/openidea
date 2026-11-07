# Spring/MyBatis 跨文件语义重构交付报告

状态：源码、针对性测试、真实 Java8/Boot/MyBatis 验证完成；完整原生 host/app 构建及 Mac UI 操作验收留待主会话统一集成，未冒充 UI 实测完成。

## 独立交付与提交

- clone：`/Users/hyl/Documents/Codex/2026-11-07/openidea-spring-semantic-refactor/work/openidea`
- 分支：`spring-semantic-refactor`
- 基线：`57653df63a91789a3544436109ee288b7cfb0ba6`，未复制其他会话未提交内容。
- `5f14980277018fd032ec5eec40a94b53f9b7e1ab`：现有 rename、插件、事务层审计。
- `9ce75f10dd652827cb73d1aa2e116f34b1d0fabf`：修复 macOS 撤销恢复源码时的旧 mtime，避免增量编译继续使用重构后的 class。
- `5b5f3032172634a50f74966c419cfc3200ed0f44`：源码范围内 Spring/MyBatis 语义重命名、预览/取消、快照冲突检测、独立测试和 Java8 fixture。
- `d2fbe32ca208f2899595343c463f72eac7b53e9c`：ConfigurationProperties 前缀必须满足小写 canonical 格式，增加反例。
- `5238c225a2dd18366536119f7e5673304e6a2c14`：第一轮交付报告。
- `ea510787562c86e1cb3241fe2c094f959bb2bb30`：生产 Snapshot 文档成员及生命周期校验回归和修复。
- `e3181cb65c1dcc363e86f3b9c4c212c6b00d0e8b`：JavaBean 配置叶子、字段/accessor/调用处、真实 JDT 编辑合并及 Java8 apply/undo 验证。
- `7a7a651c078ccd45f29e1046b237bfa1ab8d5c49`：嵌套绑定 DTO 保守拒绝，防止退回仅 Java 改名而遗漏配置。

未推送、未发布、未替换已安装应用；原 demo 和主会话 checkout 未修改；没有占用 18088；没有向其他聊天发送消息。禁止修改的 MVC、`ops/routes.rs`、`commands/nav.rs` 均未修改。

## 审计结论与复用机制

原链路 `src/editor/lsp/rename.ts → lsp_prepare_rename/lsp_rename → applyWorkspaceEdit` 只使用语言服务结果。Spring/MyBatis 插件没有 rename provider，已有导航结果不会自动进入 rename。原预览只列文件和计数，无法逐项检查替换内容。

继续由现有 `Documents` 事务层唯一负责写入：`prepare_workspace_edit` 的磁盘/dirty buffer 指纹、文档版本校验、私有副本 preflight、失败回滚、整组 undo/redo 和现有 workspace history 均复用。没有增加另一套写文件/撤销实现，没有修改 IPC 类型或手写生成的 bindings。

新 planner 位于 `plugins/support/spring-refactor`，复用 `spring-text` 的 Java/XML/config scanner，只有纯计算 API。`ops/spring_refactor.rs` 负责独立源码快照，`ops/spring_rename.rs` 负责 Java LSP 与框架计划合并。所有范围在 IPC 上使用 UTF-16。

## 三类标识符的实际覆盖

| 类型                    | 已实现                                                                                                                                                          | 明确边界                                                                                                                                                                                             |
| ----------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Bean 名                 | 显式 `@Bean`/常用 stereotype 字符串、导入有据的 `@Qualifier`/`@Resource(name)`、Spring namespace XML 的声明和引用                                               | Qualifier 标签与 Bean 名分离；声明重名/Qualifier 标签冲突时拒绝；动态常量、SpEL、getBean 字符串及按名称自动注入不解析                                                                                |
| 隐式 Bean               | 从 Java 类/工厂方法发起时让 JDT 改 Java 符号，并按 Spring decapitalize 改已识别 Bean 引用；从 Qualifier 发起时显式写入新 Bean 名，保留 Java 类/方法名及原有注释 | 不通过修改普通 Java 字符串猜测 Java 符号引用；不声称能处理运行时注册或库中的引用                                                                                                                     |
| 配置 key                | application/bootstrap 的 properties/YAML key、Java `@Value` placeholder、这些配置值中的 placeholder；保留默认值、普通值和注释                                   | key 只能保留在原父 mapping 内；不移动 YAML 子树；普通独立 key 按原 key 匹配；不处理任意导入文件、XML placeholder 或环境变量                                                                          |
| ConfigurationProperties | prefix 本身；唯一普通 JavaBean 标量叶子的字段/getter/setter/Java 调用处，以及 properties/YAML/@Value 的 camel/kebab/snake 别名；保留原拼写风格、默认值及 prefix                                                                        | 仅支持源码内、字面量非空 canonical prefix、无继承的可变类、私有可变标量字段及唯一 public getter/setter；record/构造器绑定、继承、嵌套 DTO、集合、缺失/重载 accessor、自定义 BeanInfo、工厂绑定、重复属性或未知 prefix 保守拒绝；拒绝配置单边改名                                                                                                     |
| Mapper statement        | 唯一 Java 方法 + namespace/statement id；XML association/collection/constructor 等 nested select 引用，含跨 namespace 的完整引用                                | JDT 必须确实返回声明修改；无 Java 方法、重载、重复 statement/databaseId 变体拒绝；SQL 标识符、sql fragment、resultMap、普通文本和无关 namespace 不改；SqlSession 字符串、注解 nested-select 等不解析 |

所有 Java 类、方法及调用处由 JDT 重命名；Bean 名、配置 key 和 SQL/statement 标识符没有混用。实现中没有全局字符串替换。

## 预览、取消和一致性

- 在最近 Maven/Gradle 模块的 `src/main/java`、`src/main/resources`、`src/test/java`、`src/test/resources` 内读取源码；排除兄弟模块、构建产物和 symlink 遍历。每个支持文件上限 2 MiB，总计 32 MiB/1500 个文件，超限拒绝。
- open/未保存文档覆盖磁盘内容，包含未保存新文件。扫描前后校验整个源码目录指纹、已打开文档的成员、路径和版本；捕获文档关闭也统一视为源码冲突。
- 将整个已扫描目录作为预览 precondition 保留，因此后来新增的同名定义、原本不需要修改的引用变化也使预览失效。JDT 的其他编辑路径也捕获自己的 precondition。
- 语义重构即使单文件也必须显示预览。预览明确说明仅覆盖列出的源码修改；普通 Java LSP 结果落在该扫描范围内时也保留范围提示，不宣称完整安全重命名。
- 逐文件展开 UTF-16 行列范围、替换前后的文本；前文本通过现有 `fsReadText` 读取，优先未保存 buffer。
- 计算中可取消，替换/关闭 pending popup 会取消 RequestId；晚到结果不会重新打开预览或写入。预览取消也不写入。
- Java 声明和框架编辑冲突则整组拒绝。框架范围与 JDT 原文件文本修改合并后再移动文件；资源操作之后又出现文本修改的混合顺序保守拒绝。
- 应用和整组撤销均复用现有事务历史。真实写入中途故障的回滚由原事务测试覆盖。
- 全部新增用户文字同步 en/zh，CSS 只使用 tokens。

## 真实发现并修复的撤销缺陷

macOS `fs::copy` 会保留旧 mtime。原事务撤销恢复 `.java` 后，Maven 增量编译认为源码早于上次 class，继续使用重命名后的 `@Value("${service.delay:99}")` class，但配置已经恢复为 `service.timeout`。实际 Boot 测试从预期 11 变成默认值 99。

在 macOS 复制事务快照时刷新目标文件 mtime。修复后，**撤销之后直接执行增量 `mvn test`** 也能重新编译、启动并通过查询。没有通过每次 clean 掩盖问题；仅初始基线 clean，后续十次 apply/undo 验证使用增量构建。

## 已完成验证

| 检查                        | 结果                                                                                                                                                                            |
| --------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| planner 独立 Cargo tests | 21 通过：原覆盖及 JavaBean 叶子三符号计划、别名、boolean is accessor、JDT 多行编辑精化、注解身份、未知 prefix、构造器/继承/嵌套 DTO/工厂/BeanInfo/冲突等拒绝反例 |
| 独立 host harness | 17 常规测试通过，另 2 个依赖环境的 ignored 集成测试已单独执行通过。直接编译生产 adapter/orchestrator，新增文档成员/关闭/移动、属性多 JDT 请求、自动 accessor 去重、缺失 accessor/取消整体拒绝 |
| workspace 事务单元测试      | 8 通过，含中途写入失败回滚、dirty buffer 恢复、失败回滚保留 recovery、崩溃清理等                                                                                                |
| workspace integration tests | 原 22 通过 + 新 mtime 回归 1 通过                                                                                                                                               |
| frontend targeted tests     | 100 通过，6 个原有 localization 条件跳过；含 pending 取消/晚到结果、单文件范围预览、UTF-16 替换前文本、预览取消、workspace undo                                                 |
| clippy                      | planner、host harness、`ide-workspace --all-targets` 均 `-D warnings` 通过                                                                                                      |
| formatting/lint             | root + 独立 crate 的 cargo fmt check、相关前端 oxfmt/oxlint、git diff check 通过                                                                                                |
| typecheck/build             | `tsrs --noEmit`、Vite production build 通过；Vite 仍有原有大 chunk / ineffective dynamic import 提示                                                                            |
| 真实 Java8 fixture | Boot 2.7.18、MyBatis starter 2.3.2、H2：9 个叶子入口各 apply + grouped undo（基线 + 18 次增量 Maven）；增强 fixture 上原 5 场景再次验证（基线 + 10 次增量 Maven）；共 30 次成功 Maven，两个 JUnit 测试验证启动/注入/绑定/调用/查询和三种别名独立绑定 |

host harness 的常规编排单元测试使用明确标识的 FakeLsp。两个 Java8 测试则把**真实 JDT 的 rename response**通过测试传输层重放给生产编排器；这不等于已在原生 app 中操作。`scripts/spring-refactor-jdt.mjs` 以独立 config/workspace 启动已安装 JDT LS 1.63，只捕获 prepareRename/rename，拒绝服务写源码。Mapper 返回声明及 Java caller 的 edits；属性字段/getter/setter 分别捕获 3/3/2 个 Java 文件的编辑，随后经过生产编排器和真实 Documents 事务应用及撤销。

真实 JDT 的字段 rename 会自动改 accessor，并可能返回跨行合并 TextEdit 或标识符后缀插入。新增精化仅在 JDT 给出的编辑窗口内按 Java token 和原样空隙分解；100ms diff deadline，验证新旧编辑回放结果逐字节一致，不重新猜测 Java 引用。字段/getter/setter 三组必须均有声明修改证明；已由 JDT 一次改完的 accessor 去重，缺失任何一组或取消均不返回部分计划。真实路径同时处理 macOS `/var` 与 `/private/var` 别名。扫描、解析、精化和计划使用 blocking pool。

9 个属性入口为字段声明、getter、setter、properties snake key、YAML leaf、@Value、getter 调用、setter 调用和字段使用处；每个入口均验证三种资源别名、新 accessor/调用、未相关同名类、普通文本和注释保持正确，再执行整组撤销及增量 Maven。`BindingAliasesTest` 通过真实 JavaBeans Introspector 描述符和 Boot Binder 分别绑定 camel/kebab/snake 三种来源，避免仅凭多个别名同时存在而误判成功。

已安装 JDT LS 1.63 的 Eclipse bundle 要求 JavaSE 25。首次用 Java21 启动因 OSGi JavaSE=25 失败；随后只读复用了已安装 Temurin 25 LS runtime，并把项目 runtime 明确指定为 JavaSE-1.8。Java 测试断言实际 JVM 为 1.8，日志为 1.8.0_421。

全 workspace 的 `cargo metadata --offline` 曾因缓存缺少 Android-only `android_system_properties 0.1.6` 失败，非本次 Mac 编译失败。锁文件已由 Cargo 为新路径依赖更新；Mac 模块测试和 clippy 正常完成。

### 复现命令

在本 clone 中使用独立 targets；下面是实际执行的针对性命令：

```sh
cargo test --manifest-path plugins/support/spring-refactor/Cargo.toml --offline
cargo test --manifest-path plugins/support/spring-refactor/host-tests/Cargo.toml --offline
cargo test -p ide-workspace --lib workspace_edit --offline
cargo test -p ide-workspace --test workspace_edit_mtime --test workspace_edits --offline
cargo clippy -p ide-workspace --all-targets --offline -- -D warnings
cargo clippy --manifest-path plugins/support/spring-refactor/Cargo.toml --all-targets --offline -- -D warnings
cargo clippy --manifest-path plugins/support/spring-refactor/host-tests/Cargo.toml --all-targets --offline -- -D warnings
node node_modules/vitest/vitest.mjs run src/editor/lsp/syntaxFallback.test.ts src/editor/lsp/workspaceEdit.test.ts src/editor/lsp/Popups.test.tsx src/editor/core/model/workspaceHistory.test.ts src/platform/locales/textCatalog.test.ts
node_modules/.bin/tsrs --noEmit
node_modules/.bin/vite build
```

真实 fixture 验证：先复制 `fixtures/java-spring-refactor` 到独立目录，禁止操作原 demo；`node scripts/spring-refactor-jdt.mjs <fixture-copy> <jdtls-install> <ls-jdk-runtime> <jdk8-home> <independent-scratch> <java-edits.json>`。然后设置 `JAVA_HOME=<jdk8-home>`、`OPENIDEA_FIXTURE=<fixture-copy>`、`OPENIDEA_JAVA_EDITS=<java-edits.json>`、`OPENIDEA_MAVEN=<maven-bin>`，执行：

```sh
cargo test --manifest-path plugins/support/spring-refactor/host-tests/Cargo.toml --offline java8_boot_mybatis_apply_and_grouped_undo -- --ignored --nocapture
```

叶子验证另用末尾 `property` 模式捕获：

```sh
node scripts/spring-refactor-jdt.mjs <fixture-copy> <jdtls-install> <ls-jdk-runtime> <jdk8-home> <independent-scratch> <property-edits.json> property
# 同样设置 JAVA_HOME、OPENIDEA_FIXTURE、OPENIDEA_MAVEN，并设置 OPENIDEA_PROPERTY_EDITS=<property-edits.json>
cargo test --manifest-path plugins/support/spring-refactor/host-tests/Cargo.toml --offline java8_property_field_accessors_and_resource_aliases_apply_and_undo -- --ignored --nocapture
```

增量验证的 fixture copy 为本会话 `work/fixture-leaf`；真实 JDT response 为 `work/jdt-property.json` 及 `work/jdt-mapper-followup.json`。30 份 Maven 日志、最后一次 JUnit XML、真实 JDT JSON 和前端 build 日志已留存到本会话 `outputs/validation-followup/`。Maven 使用 `-q`，成功退出由 harness 的 status assertion 校验，日志没有 BUILD SUCCESS 行。第一轮历史证据仍在 `outputs/validation/`。生成的独立 Rust target、dist 和 fixture target 在日志留存后清理。

## 主会话集成点与待验收

1. 第一轮到 `5238c225`；本次增量依次集成 `ea510787`、`e3181cb6`、`7a7a651c` 和最终报告提交。薄接入仅有 `ide-host/Cargo.toml`、`ops/mod.rs` 与 `ops/lsp.rs` 两个 rename 入口；没有修改 nav/routes。若其他会话也修改 lsp.rs，保留原 prepare/code-action 等逻辑，只合并 rename 两处。
2. `spring-refactor::Sources` 为路径到源码文本的 BTreeMap；`target/recognized/refusal/plan` 为纯 API，输出 UTF-16 TextEdit、可选主 JavaRename 和 related_java。`JavaRename.new_name` 可覆盖用户输入，以派生字段/getter/setter 的不同新名称，`Plan.java_requests()` 统一遍历；planner 和 adapter 必须一起集成。这些均为内部 Rust API，无 IPC 改动。以后可由会话 1 的 source context/配置来源或会话 3 的依赖索引替换 Snapshot 数据供应，而不改变事务/预览接口。当前没有读取它们的未提交实现，也没有推断库 Bean/条件是否激活。
3. `WorkspaceEditSummary.changes` 是前端内存字段，原 IPC WorkspaceEdit 和 preconditions 不变，无需生成 bindings。目录 preconditions 同时用于显示明确的源码范围提示。
4. **完整 ide-host / native app 编译、app 内菜单/快捷键、截图和 e2e 未做**。遵守六会话避免同时重型构建的要求，主会话统一构建后按下列项目做 Mac 验收，不应把模块测试当成 app UI 验收。

Mac 验收清单：复制 fixture，选择 Java8，等待 JDT 导入；分别在 Bean 字符串、Qualifier(counter)、@Value key、ConfigurationProperties prefix、普通 JavaBean 字段/getter/setter/调用处/资源别名、Mapper Java 方法及 XML id 上 Shift+F6。检查逐项预览和取消；应用后 Cmd+Z 在任一受影响文件应整组撤销。带 emoji 的未保存 Consumer 内容必须保留；预览期间外部修改文件/新增同名定义应整体拒绝；计算时 Escape 取消后晚到结果不得重开预览。Mapper 的 OtherMapper.find、sql fragment find、SQL alias、普通文本和 XML 注释必须保持不变。

主会话后续可按明确边界扩展不可变/继承/嵌套绑定模型、非标准/跨模块配置来源及动态字符串引用；当前普通 JavaBean 叶子已完成，其他模型明确拒绝。

## 官方依据

- [Spring Framework 5.3.31 Core Technologies](https://docs.spring.io/spring-framework/docs/5.3.31/reference/html/core.html)：Bean 命名、aliases 与 qualifier 的不同语义。
- [Spring Boot 2.7.18 Core Features](https://docs.spring.io/spring-boot/docs/2.7.18/reference/html/features.html)：外部配置、ConfigurationProperties、canonical prefix 与 relaxed binding。
- [MyBatis Mapper XML](https://mybatis.org/mybatis-3/sqlmap-xml.html)：namespace/statement id、nested select 引用及 SQL fragment 分离。

## Snapshot 集成复查结论

- 已在生产 Snapshot adapter 上添加新增未落盘 Bean/Mapper 的回归。原实现已通过：目录指纹中的 unsaved extras 会捕获该变化，因此没有伪造“dirty 新文件漏检”的复现结论。
- 另确认两条失败回归：扫描后才打开被 ignore 的已有源码，原 verify 返回 Ok；捕获 dirty 文档保存后关闭，原 verify 返回 NotFound 而非 OutOfSync。现补充保存 roots 和捕获文档路径，校验根范围内新增文档、路径/版本变化和关闭，统一返回源码冲突。关闭/移出 dirty 源码的回归也纳入。
- 新增未落盘 Bean/Mapper 的回归同时验证 Snapshot.verify 和 prepared transaction apply；重复 Mapper FQN 也拒绝。修复范围和 JavaBean 叶子功能已分别提交，保持问题证据可追踪。
