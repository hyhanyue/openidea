import { openDiffDialog } from "./diffDialog";
import type { DiffData } from "./diffInput";

export function showCommitDiff(data: DiffData): void {
  openDiffDialog(data);
}
