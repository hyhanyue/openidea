import { tx } from "@/platform/i18n";

export function isSpringBeansPlugin(pluginId: string): boolean {
  return pluginId === "aritti.spring-beans" || pluginId === "hyhanyue.spring-beans";
}

/** Labels emitted by the static Spring dependency model, including its graph HTML. */
export function springLabels(text: string): string {
  for (const [label, translated] of [
    ["@Bean (dependency)", tx("@Bean (dependency)")],
    ["@Component (dependency)", tx("@Component (dependency)")],
    ["@Bean (module)", tx("@Bean (module)")],
    ["@Component (module)", tx("@Component (module)")],
  ]) {
    text = text.replaceAll(label!, translated!);
  }
  return text;
}
