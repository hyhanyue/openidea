import { readFileSync } from "node:fs";
import { expect, it } from "vitest";
import type { RunConfig } from "./ipc";
import { springLaunchContext, publishSpringLaunchContext, useSpringLaunchContext } from "./springLaunchContext";
import { activeSpringBeansPlugin } from "./springPlugins";

const contract = JSON.parse(readFileSync("docs/spring-launch-context-contract.json", "utf8"));
for (const fixture of contract.fixtures) {
  it(`produces shared contract sample: ${fixture.name}`, () => {
    const actual = springLaunchContext(
      fixture.producerInput.config as RunConfig | null,
      fixture.producerInput.persisted,
    );
    if (fixture.producerExpected === null) expect(actual).toBeNull();
    else expect(actual).toEqual(fixture.producerExpected);
    expect(JSON.stringify(actual)).not.toContain("synthetic-private-value");
    if (fixture.installedPlugins)
      expect(activeSpringBeansPlugin(fixture.installedPlugins)).toBe(fixture.consumerExpected.pluginId);
  });
}
it("replaces dev with prod then removes the transient context on deselection", () => {
  for (const name of ["dev", "prod-persisted-nonboolean", "deselect-clears-launch"]) {
    const fixture = contract.fixtures.find((f: { name: string }) => f.name === name);
    publishSpringLaunchContext(fixture.producerInput.config, fixture.producerInput.persisted);
    const actual = useSpringLaunchContext.getState().context;
    if (fixture.producerExpected === null) expect(actual).toBeNull();
    else expect(actual).toEqual(fixture.producerExpected);
  }
});
