import { useState } from "react";
import { tx } from "@/platform/i18n";
import { Dialog, IconButton } from "@/ui";
import { DiffEditor } from "./DiffEditor";
import { diffInput, diffTitle, type DiffData } from "./diffInput";
import { openDialog } from "./dialogs";

export function showCommitDiff(data: DiffData) {
  openDialog((close) => <CommitDiffDialog initial={data} onClose={close} />);
}

function CommitDiffDialog({ initial, onClose }: { initial: DiffData; onClose: () => void }) {
  const [data, setData] = useState(initial);
  const [maximized, setMaximized] = useState(false);
  const input = diffInput(data);

  return (
    <Dialog
      open
      title={<span>{diffTitle(data)}</span>}
      aria-label={tx("Diff")}
      onClose={onClose}
      closeShortcut="Meta+W"
      resizable="both"
      maximized={maximized}
      minWidth={480}
      minHeight={280}
      initialSize={{ w: 1040, h: 660 }}
    >
      <DiffEditor
        key={input.key}
        input={input}
        onNavigate={setData}
        toolbarActions={
          <IconButton
            icon={maximized ? "minimize" : "maximize"}
            title={maximized ? "Restore Size" : "Maximize"}
            pressed={maximized}
            size="small"
            onClick={() => setMaximized(!maximized)}
          />
        }
      />
    </Dialog>
  );
}
