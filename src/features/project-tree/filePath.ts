/** Navigate › File Path (⌥⌘F12): the file and its folders, each revealed in the OS file manager. */
import { openPopup } from "@/editor/lsp/popupStore";
import { nameOf, parentOf } from "./paths";
import { revealPathsInFinder } from "./operations";

const DRIVE_ROOT = /^[A-Za-z]:\/?$/;

/** `path` then each folder above it, up to the file system (or drive) root. */
export function filePathChain(path: string): string[] {
  const out = [path];
  for (let p = path; p !== "/" && !DRIVE_ROOT.test(p);) {
    const parent = parentOf(p);
    if (parent === p) break;
    p = /^[A-Za-z]:$/.test(parent) ? `${parent}/` : parent;
    out.push(p);
  }
  return out;
}

export function showFilePathPopup(path: string, isDir = false) {
  const items = filePathChain(path).map((p, i) => ({
    id: p,
    label: nameOf(p) || p,
    detail: i === 0 ? undefined : p,
    icon: i === 0 && !isDir ? "file" : "folder",
    run: () => revealPathsInFinder([p]),
  }));
  openPopup({ kind: "list", anchor: { x: window.innerWidth / 2 - 160, y: 96 }, title: "File Path", items });
}
