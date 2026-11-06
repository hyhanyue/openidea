import { readFileSync } from "node:fs";
import type { EditorView } from "@codemirror/view";
import { expect, ipcCalls, stubDesktop, test } from "./fixtures";

test("dismissing editor submenus preserves the Java viewport and selection", async ({ workbench: page }) => {
  const fixture = readFileSync(
    new URL("../fixtures/java-app/src/main/java/com/example/orders/OrderBook.java", import.meta.url),
    "utf8",
  );
  const body = Array.from({ length: 600 }, (_, i) => `        total += ${i}; // Probe ${i}`).join("\n");
  const source = fixture.replace(/}\s*$/, `    int execute(int total) {\n${body}\n        return total;\n    }\n}\n`);
  const path = "/dev/acme-api/OrderBook.java";
  await page.evaluate(
    async ({ path, source }) => {
      const backendModule = "/src/editor/core/backend.ts";
      const contributionsModule = "/src/workbench/contributions.ts";
      const { docBackend } = await import(/* @vite-ignore */ backendModule);
      const { editorService } = await import(/* @vite-ignore */ contributionsModule);
      docBackend().files.set(path, source);
      await editorService().openFile(path);
    },
    { path, source },
  );

  const content = page.locator('.cm-editor[data-language="Java"] .cm-content');
  await content
    .locator(".cm-line")
    .first()
    .click({ position: { x: 40, y: 9 } });
  const scroller = page.locator(".cm-scroller");
  const box = (await scroller.boundingBox())!;
  const x = box.x + 220;
  const y = box.y + box.height * 0.42;
  const viewport = () =>
    content.evaluate((element) => {
      const { view } = (element as unknown as { cmTile: { view: EditorView } }).cmTile;
      const { top, bottom } = view.scrollDOM.getBoundingClientRect();
      const stickyBottom = view.dom.querySelector(".cm-openidea-sticky")?.getBoundingClientRect().bottom ?? top;
      const lines = [...element.querySelectorAll(".cm-line")];
      const firstLineBelow = (edge: number) => {
        const line = lines.find((line) => {
          const rect = line.getBoundingClientRect();
          return rect.bottom > edge && rect.top < bottom;
        });
        return line ? view.state.doc.lineAt(view.posAtDOM(line)).number : null;
      };
      return {
        top: view.scrollDOM.scrollTop,
        firstLine: firstLineBelow(top),
        firstUncoveredLine: firstLineBelow(Math.max(top, stickyBottom)),
        selection: view.state.selection.main.toJSON(),
      };
    });

  const root = page.getByRole("menu", { name: "Context menu" });
  for (const { label, direction, offset } of [
    { label: "Copy / Paste Special", direction: "forward", offset: 4900 },
    { label: "Go To", direction: "backward", offset: 4800 },
    { label: "Folding", direction: "none", offset: 4800 },
  ]) {
    await test.step(label, async () => {
      await content.evaluate((element, direction) => {
        const { view } = (element as unknown as { cmTile: { view: EditorView } }).cmTile;
        view.focus();
        view.dispatch({
          selection: {
            anchor: direction === "backward" ? view.state.doc.length : 0,
            head: direction === "forward" ? view.state.doc.length : 0,
          },
        });
        view.scrollDOM.scrollTop = 0;
      }, direction);
      await page.waitForTimeout(120);
      await scroller.evaluate((element, offset) => (element.scrollTop = offset), offset);
      await page.waitForTimeout(150);
      const before = await viewport();
      expect(before.firstLine).toBeGreaterThan(100);

      await page.mouse.click(x, y, { button: "right" });
      await root.waitFor();
      await page.waitForTimeout(70);
      const submenu = root.getByRole("menuitem", { name: label, exact: true });
      await submenu.hover();
      await expect(submenu).toHaveAttribute("aria-expanded", "true");
      await page.waitForTimeout(260);
      const opened = await viewport();
      if (direction === "none") expect(opened.selection.anchor).toBeGreaterThan(0);
      else expect(opened.selection).toEqual(before.selection);

      await page.keyboard.press("Escape");
      await expect(submenu).toHaveAttribute("aria-expanded", "false");
      await page.waitForTimeout(100);
      await page.keyboard.press("Escape");
      await expect(root).toBeHidden();
      await expect(content).toBeFocused();
      // WebKit can reveal the selection after focus has already returned.
      await page.waitForTimeout(300);
      expect.soft(await viewport()).toEqual({ ...before, selection: opened.selection });
    });
  }

  await test.step("keyboard activation of Go To still reveals its target", async () => {
    await stubDesktop(page);
    await page.evaluate((path) => {
      const target = { line: 520, col: 8 };
      window.__ipc.handlers.lsp_goto = () => [{ path, range: { start: target, end: target } }];
    }, path);
    await page.mouse.click(x, y, { button: "right" });
    await root.getByRole("menuitem", { name: "Go To", exact: true }).hover();
    await expect(page.getByRole("menuitem", { name: "Declaration or Usages" })).toBeFocused();
    await page.keyboard.press("Enter");
    await expect(root).toBeHidden();
    await expect.poll(() => ipcCalls(page, "lsp_goto")).toHaveLength(1);
    await expect.poll(async () => (await viewport()).firstLine).toBeGreaterThan(450);
  });
});
