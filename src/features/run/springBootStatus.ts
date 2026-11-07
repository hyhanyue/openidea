import { tx } from "@/platform/i18n";
import type { SpringBootLifecycle } from "@/platform/springBootLifecycle";

export function springBootStatus(state: SpringBootLifecycle): string {
  switch (state.phase) {
    case "preparing":
      return tx("Preparing Spring Boot");
    case "compiling":
      return tx("Compiling Spring Boot");
    case "starting":
      return tx("Starting Spring Boot");
    case "ready":
      return tx("Spring Boot started");
    case "stopping":
      return tx("Stopping Spring Boot");
    case "stopped":
      return tx("Spring Boot stopped");
    case "failed":
      switch (state.failure) {
        case "port":
          return tx("Spring Boot failed: port already in use");
        case "missingBean":
          return tx("Spring Boot failed: required bean missing");
        case "ambiguousBean":
          return tx("Spring Boot failed: multiple matching beans");
        case "binding":
          return tx("Spring Boot failed: configuration binding error");
        default:
          return tx("Spring Boot failed: see console for details");
      }
  }
}
