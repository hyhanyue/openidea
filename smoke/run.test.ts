/**
 * Running and testing each language's `fixtures/` project the way the Run tool window does:
 * the backend detects the configurations, the run store starts them (with its test
 * reporting), parses the output into the test tree, and reruns the failed tests.
 */
import { execFileSync } from "node:child_process";
import { cpSync, mkdtempSync, readFileSync, realpathSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterEach, expect, test } from "vitest";
import { commands, type RunConfig } from "@/bindings";
import { onIdeEvent } from "@/platform/ipc";
import { openProject, useProject } from "@/platform/project";
import {
  failedTests,
  loadConfigs,
  onRunExited,
  rerunFailed,
  runSelectedTests,
  resetRuns,
  runtime,
  startRun,
  stopSession,
  useRun,
  waitForExit,
} from "@/features/run/store";
import { nodeStatus } from "@/features/run/testing/results";
import { connectDesktop } from "./desktop";
import { repoRoot, Server } from "./server";

interface Case {
  fixture: string;
  /** Runs in the project copy first (installs its dependencies). */
  setup?: (root: string) => void;
  /** A configuration that runs to completion printing this, or a server stopped once it prints it. */
  run?: { config: string; prints: string; stop?: boolean };
  tests: {
    config: string;
    /** The configuration with the fixture's intentionally failing test switched on. */
    failing: (config: RunConfig) => RunConfig;
    passed: number;
    failed: string;
    /** The failure's location ends with this file. */
    file: string;
  };
}

const env = (config: RunConfig, vars: Record<string, string>): RunConfig => ({
  ...config,
  env: { ...config.env, ...vars },
});
const args = (config: RunConfig, extra: string[]): RunConfig => ({ ...config, args: [...config.args, ...extra] });

const CASES: Record<string, Case> = {
  typescript: {
    fixture: "ts-app",
    setup: (root) => execFileSync("npm", ["install", "--no-audit", "--no-fund"], { cwd: root, stdio: "ignore" }),
    run: { config: "npm:start", prints: "Total: $134.97" },
    tests: {
      config: "npm:test",
      failing: (c) => env(c, { OPENIDEA_FAIL: "1" }),
      passed: 4,
      failed: "intentionally fails (set OPENIDEA_FAIL=1)",
      file: "tests/cart.test.ts",
    },
  },
  python: {
    fixture: "python-app",
    // A project venv with pytest, as the fixture's README sets it up.
    setup: (root) => {
      execFileSync("python3", ["-m", "venv", ".venv"], { cwd: root, stdio: "ignore" });
      execFileSync(join(root, ".venv", "bin", "pip"), ["install", "-q", "pytest"], { cwd: root, stdio: "ignore" });
    },
    tests: {
      config: "pytest",
      failing: (c) => env(c, { OPENIDEA_FAIL: "1" }),
      passed: 4,
      failed: "test_intentionally_failing",
      file: "tests/test_models.py",
    },
  },
  rust: {
    fixture: "rust-app",
    run: { config: "cargo:run:inventory", prints: "Sold 3 mice, 9 left" },
    tests: {
      config: "cargo:test",
      failing: (c) => args(c, ["--features", "failing-tests"]),
      // `cargo test` stops after the failing unit tests: the integration tests don't run.
      passed: 7,
      failed: "intentionally_failing",
      file: "src/lib.rs",
    },
  },
  go: {
    fixture: "go-app",
    run: { config: "go:run:server", prints: "listening on", stop: true },
    tests: {
      config: "go:test",
      failing: (c) => env(c, { OPENIDEA_FAIL: "1" }),
      passed: 3,
      failed: "TestIntentionallyFailing",
      file: "store_test.go",
    },
  },
  java: {
    fixture: "java-app",
    tests: {
      config: "maven:test",
      failing: (c) => args(c, ["-DopenideaFail=true"]),
      passed: 2,
      failed: "intentionallyFails",
      file: "OrderBookTest.java",
    },
  },
  kotlin: {
    fixture: "kotlin-gradle",
    run: { config: "gradle:run", prints: "Lent 'Kotlin in Action' until 2024-01-15" },
    tests: {
      config: "gradle:test",
      failing: (c) => args(c, ["-PopenideaFail"]),
      passed: 3,
      failed: "intentionally failing",
      file: "LibraryTest.kt",
    },
  },
};

let server: Server | null = null;
let disconnect: (() => void) | null = null;
let base = "";

afterEach(async () => {
  // As switching projects does: run ids restart with each server.
  resetRuns();
  disconnect?.();
  await server?.close();
  // A language server's own process (jdtls's JVM) can outlive the server's for a moment:
  // deleting its data directory has to wait for it.
  if (base) rmSync(base, { recursive: true, force: true, maxRetries: 10, retryDelay: 1000 });
});

async function open(c: Pick<Case, "fixture" | "setup">): Promise<string> {
  base = realpathSync(mkdtempSync(join(tmpdir(), "openidea-smoke-")));
  const root = join(base, c.fixture);
  cpSync(join(repoRoot, "fixtures", c.fixture), root, { recursive: true });
  c.setup?.(root);
  server = await Server.start(join(base, "home"));
  disconnect = connectDesktop(server);
  const unsubscribe = onIdeEvent("runExited", (e) => onRunExited(e.run, e.code));
  const previous = disconnect;
  disconnect = () => {
    unsubscribe();
    previous();
  };
  await openProject(root);
  const trusted = await commands.projectTrustSet(root, true, false);
  if (trusted.status !== "ok") throw new Error(trusted.error.message);
  useProject.setState({ project: { ...useProject.getState().project!, trusted: true } });
  await loadConfigs();
  return root;
}

function config(id: string): RunConfig {
  const found = useRun.getState().configs.find((c) => c.id === id);
  if (!found)
    throw new Error(
      `no configuration ${id}: ${useRun
        .getState()
        .configs.map((c) => c.id)
        .join(", ")}`,
    );
  return found;
}

/** Runs `config` to its end; returns its session id and output. */
async function runToEnd(config: RunConfig, timeoutMs = 8 * 60_000): Promise<{ id: string; output: string[] }> {
  const output: string[] = [];
  const session = await startRun(config, undefined, undefined, { prepared: true, onLine: (l) => output.push(l) });
  if (!session) throw new Error(`${config.id} didn't start`);
  await waitForExit([session.id], timeoutMs);
  const ended = useRun.getState().sessions.find((s) => s.id === session.id)!;
  expect(ended.status, `${config.id} is still running:\n${output.join("\n")}`).not.toBe("running");
  return { id: session.id, output };
}

async function rerunFailedToEnd(id: string, timeout = 60_000): Promise<void> {
  const before = useRun.getState().sessions.find((s) => s.id === id)!.startedAt;
  expect(await rerunFailed(id), "Rerun Failed Tests is offered").toBe(true);
  await expect
    .poll(
      () => {
        const session = useRun.getState().sessions.find((s) => s.id === id)!;
        return session.startedAt > before && session.endedAt !== null && session.endedAt >= session.startedAt;
      },
      { timeout },
    )
    .toBe(true);
}

async function smoke(language: string) {
  const c = CASES[language]!;
  await open(c);

  if (c.run) {
    const output: string[] = [];
    const session = await startRun(config(c.run.config), undefined, undefined, {
      prepared: true,
      onLine: (l) => output.push(l),
    });
    expect(session, "the run starts").toBeTruthy();
    await expect.poll(() => output.join("\n"), { timeout: 5 * 60_000 }).toContain(c.run.prints);
    if (c.run.stop) await stopSession(session!.id);
    await waitForExit([session!.id], 60_000);
    const ended = useRun.getState().sessions.find((s) => s.id === session!.id)!;
    expect(ended.status, output.join("\n")).toBe("finished");
    if (!c.run.stop) expect(ended.exitCode, output.join("\n")).toBe(0);
  }

  // Tests, with the intentionally failing one switched on: one failure, located, the rest passed.
  // Switched on in the saved configuration, as reruns launch the configuration as it is saved now.
  const failing = c.tests.failing(config(c.tests.config));
  useRun.setState((s) => ({ configs: s.configs.map((x) => (x.id === failing.id ? failing : x)) }));
  const { id, output } = await runToEnd(failing);
  const tests = runtime(id)!.tests!;
  const log = `\n--- output ---\n${output.slice(-80).join("\n")}`;
  expect(tests.counts(), log).toMatchObject({ passed: c.tests.passed, failed: 1 });
  const failure = tests.firstFailure()!;
  expect(failure.name, log).toBe(c.tests.failed);
  expect(failure.location?.path ?? "", `${JSON.stringify(failure.location)}${log}`).toMatch(
    new RegExp(`${c.tests.file.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}$`),
  );
  expect(failure.output.join("\n"), "the failure's message is kept").not.toBe("");
  expect(failedTests(id)).toHaveLength(1);

  // Rerun Failed Tests runs that one test only.
  const rerunOutput: string[] = [];
  await rerunFailedToEnd(id, 8 * 60_000);
  const again = runtime(id)!.tests!;
  rerunOutput.push(...again.roots.map((r) => `${r.name}: ${nodeStatus(r)}`));
  expect(again.counts(), rerunOutput.join("\n")).toMatchObject({ passed: 0, failed: 1 });
  expect(again.firstFailure()?.name).toBe(c.tests.failed);
}

for (const language of Object.keys(CASES)) {
  test(language, () => smoke(language));
}

test.each(["separate", "shared"])(
  "Maven reactor scopes failed and selected tests by module and plugin in %s report directories",
  async (directories) => {
    await open({
      fixture: "java-maven-reactor",
      setup: (root) => {
        if (directories === "shared") replaceInFixture(root, "reactor/beta/pom.xml", "/integration-xml", "/unit-xml");
      },
    });
    const detected = useRun.getState().configs.find((c) => c.kind === "maven" && c.isTest)!;
    const run = {
      ...detected,
      id: "maven-reactor:verify",
      cwd: null,
      args: ["-B", "-f", "reactor/pom.xml", "-Pwith-beta", "-pl", "beta", "-am", "-T", "2", "verify", "-DopenideaFail"],
    };
    const { id, output } = await runToEnd(run);
    const alpha = "reactor/alpha/";
    const beta = "reactor/beta/";
    expect(failedTests(id), output.join("\n")).toEqual([[beta, "demo.SameTest", "same"]]);
    await rerunFailedToEnd(id);
    expect(runtime(id)!.tests!.roots.map((r) => r.name)).toEqual([beta]);
    expect(runtime(id)!.tests!.at([beta, "demo.SameTest", "same"])?.mavenPlugins).toEqual(new Set(["surefire"]));
    expect(runtime(id)!.tests!.counts()).toMatchObject({ passed: 0, failed: 1, total: 1 });

    const original = await runToEnd({ ...run, id: "maven-reactor:select", args: run.args.slice(0, -1) });
    expect(runtime(original.id)!.tests!.counts(), original.output.join("\n")).toMatchObject({ passed: 6, total: 6 });
    const paths = [
      [alpha, "demo.SameTest", "same"],
      [beta, "demo.SameTest", "same"],
      [beta, "demo.WorkflowIT", "integration"],
    ];
    expect(
      await runSelectedTests(
        original.id,
        paths.map((p) => p.join("\0")),
        false,
      ),
    ).toBe(true);
    const selection = useRun.getState().sessions.find((s) => s.config.id.startsWith("context:tests:"))!;
    await waitForExit([selection.id], 60_000);
    const tests = runtime(selection.id)!.tests!;
    expect(tests.counts()).toMatchObject({ passed: 3, failed: 0, total: 3 });
    for (const path of paths) expect(tests.at(path)?.own).toBe("passed");
    expect(tests.at(paths[2]!)?.mavenPlugins).toEqual(new Set(["failsafe"]));
  },
);

function replaceInFixture(root: string, file: string, from: string, to: string) {
  const path = join(root, file);
  const source = readFileSync(path, "utf8");
  expect(source).toContain(from);
  writeFileSync(path, source.replace(from, to));
}

test.each(["missing marker", "external reports directory", "no attributable XML"])(
  "Maven reactor retains unscoped console failures with %s",
  async (mode) => {
    const root = await open({
      fixture: "java-maven-reactor",
      setup: (root) => {
        if (mode === "external reports directory") {
          replaceInFixture(
            root,
            "reactor/beta/pom.xml",
            "${project.build.directory}/unit-xml",
            "${fixture.reportDirectory}",
          );
        } else {
          replaceInFixture(
            root,
            "reactor/beta/src/test/java/demo/SameTest.java",
            "void same() {",
            'void same() { System.clearProperty("openidea.testRun");',
          );
        }
      },
    });
    const detected = useRun.getState().configs.find((c) => c.kind === "maven" && c.isTest)!;
    const run = {
      ...detected,
      cwd: null,
      args: [
        "-B",
        "-f",
        "reactor/pom.xml",
        "-Pwith-beta",
        "-T",
        "2",
        "test",
        "-DopenideaFail",
        ...(mode === "no attributable XML" ? ["-DenablePropertiesElement=false"] : []),
      ],
      env:
        mode === "external reports directory"
          ? { ...detected.env, MAVEN_OPTS: "-Dfixture.reportDirectory=target/runtime-only" }
          : detected.env,
    };
    const { id, output } = await runToEnd(run);
    const tests = runtime(id)!.tests!;
    const directory = mode === "external reports directory" ? "runtime-only" : "unit-xml";
    const xml = readFileSync(join(root, "reactor/beta/target", directory, "TEST-demo.SameTest.xml"), "utf8");
    expect(xml.includes('name="openidea.testRun"')).toBe(mode === "external reports directory");
    const alpha = tests.at(["reactor/alpha/", "demo.SameTest", "same"]);
    expect(alpha?.own, output.join("\n")).toBe(mode === "no attributable XML" ? undefined : "passed");
    expect(tests.at(["reactor/beta/", "demo.SameTest", "same"])).toBeUndefined();
    expect(failedTests(id)).toEqual([["demo.SameTest", "same"]]);
    expect(tests.firstFailure()?.output.join("\n")).toContain("Fails only in beta");
    expect(rerunFailed(id)).toBe(false);
  },
);

test("Maven standalone console fallback keeps the legacy test filter", async () => {
  const root = await open({
    fixture: "java-app",
    setup: (root) => {
      replaceInFixture(
        root,
        "src/test/java/com/example/orders/OrderBookTest.java",
        "void intentionallyFails() {",
        'void intentionallyFails() { System.clearProperty("openidea.testRun");',
      );
    },
  });
  const run = args(config("maven:test"), ["-DopenideaFail=true"]);
  useRun.setState((s) => ({ configs: s.configs.map((c) => (c.id === run.id ? run : c)) }));
  const { id, output } = await runToEnd(run);
  expect(failedTests(id), output.join("\n")).toEqual([["com.example.orders.OrderBookTest", "intentionallyFails"]]);
  const xml = readFileSync(join(root, "target/surefire-reports/TEST-com.example.orders.OrderBookTest.xml"), "utf8");
  expect(xml).not.toContain('name="openidea.testRun"');

  await rerunFailedToEnd(id);
  const session = useRun.getState().sessions.find((s) => s.id === id)!;
  expect(session.config.args).toContain("-Dtest=com.example.orders.OrderBookTest#intentionallyFails");
  expect(session.config.mavenTestSelection).toBeUndefined();
  expect(runtime(id)!.tests!.counts()).toMatchObject({ passed: 0, failed: 1, total: 1 });
});
