import { registerActions } from "@/platform/actions";
import { registerServiceView, showServiceView } from "@/workbench/contributions";
import { showSourceView } from "./sourceViews";
import { RuntimePanel } from "./RuntimePanel";
import { disconnect, refresh, watchProject } from "./store";

const VIEW = "spring.runtime";
export function activate(): () => void {
  const disposers = [
    registerServiceView({
      id: VIEW,
      title: "Spring Runtime",
      icon: "spring-config",
      order: 25,
      component: RuntimePanel,
      onUse: true,
    }),
    registerActions([
      {
        id: "springRuntime.show",
        title: "Spring Runtime",
        category: "Spring",
        when: "projectOpen",
        run: () => showServiceView(VIEW, true),
      },
      {
        id: "springRuntime.refresh",
        title: "Refresh Spring Runtime",
        category: "Spring",
        when: "projectOpen",
        run: () => refresh(),
      },
      {
        id: "springRuntime.disconnect",
        title: "Disconnect Spring Runtime",
        category: "Spring",
        when: "projectOpen",
        run: () => disconnect(),
      },
      {
        id: "springRuntime.sourceBeans",
        title: "Spring Beans (source)",
        category: "Spring",
        when: "projectOpen",
        run: () => showSourceView("beans"),
      },
      {
        id: "springRuntime.sourceGraph",
        title: "Spring Bean Graph (source)",
        category: "Spring",
        when: "projectOpen",
        run: () => showSourceView("bean-graph"),
      },
    ]),
    watchProject(),
  ];
  return () => {
    disconnect();
    disposers.reverse().forEach((d) => d());
  };
}
