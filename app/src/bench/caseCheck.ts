// `--case <project.zdp>`: the Reserves case tab driven as Tim would (Reserves case plan, workstream 4). The project (the
// demo pit saved by testdata/synthetic/workflow_demo.py) is opened, the tab opened on its Base table at `rom_loss`, the
// coal roof loss typed from 0.1 to 0.15 m in the step's settings by seam and ply, and the whole case read back as it runs
// again; then the edit discarded. With `--pictures <folder>`, the window at each stage.
import type { DockviewApi } from 'dockview-react';
import { openProject, type ProjectSummary, writePicture } from '../bridge';
import type { Viewport } from '../viewport/Viewport';
import { showPanel } from '../workspace/Workspace';
import { windowPicture } from './windowPicture';

const wait = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));

type Show = (summary: ProjectSummary) => Promise<void> | undefined;

/** Waits up to `ms` for `get` to give something, every 100 ms. */
async function until<T>(get: () => T | null | undefined | false, ms = 30_000): Promise<T | null> {
  for (let k = 0; k < ms / 100; k++) {
    const got = get();
    if (got) return got;
    await wait(100);
  }
  return null;
}

/** A whole-case row's cells as shown: its label, then the last run, now and the change where there are edits. */
const caseRow = (label: string) => {
  const row = document.querySelector<HTMLElement>(`[data-tool-panel="case"] [data-slot="case-row"][data-row="${CSS.escape(label)}"]`);
  return row ? [...row.querySelectorAll('td')].map((td) => td.textContent ?? '') : null;
};

export async function checkCase(path: string, log: (line: string) => void, show: Show, dock: () => DockviewApi | null, viewport: Viewport | null = null) {
  const picture = viewport
    ? async (name: string) => {
        await wait(600);
        log(`case: ${await writePicture(name, await windowPicture(viewport))}`);
      }
    : null;
  const api = dock();
  if (!api) return { error: 'no dock' };
  const results: Record<string, unknown> = {};
  const failures: string[] = [];
  const expect = (what: string, ok: boolean, detail?: unknown) => {
    results[what] = detail === undefined ? ok : { ok, detail };
    if (!ok) failures.push(what);
    log(`case: ${what}: ${ok ? 'yes' : 'NO'}${detail === undefined ? '' : ` (${JSON.stringify(detail)})`}`);
  };
  log(`case: opening ${path}`);
  const summary = await openProject(path);
  await show(summary);
  await wait(800);
  const base = summary.objects.find((o) => o.kind === 'table' && o.name === 'Reserves Base');
  expect('the project holds the Base reserves table', !!base);
  if (!base) return { ...results, failures };

  showPanel(api, 'case');
  const pick = await until(() => document.querySelector<HTMLSelectElement>('[data-tool-panel="case"] select[aria-label="The reserves table whose case to show"]'));
  if (pick) {
    Object.getOwnPropertyDescriptor(HTMLSelectElement.prototype, 'value')?.set?.call(pick, base.id);
    pick.dispatchEvent(new Event('change', { bubbles: true }));
  }
  const started = performance.now();
  // The case shown is the Base table's, not the one the tab opened on first.
  const opened = await until(() => document.querySelector(`[data-tool-panel="case"][data-case="${base.id}"]`) && caseRow('Roof loss'));
  expect('the tab opens at rom_loss with the whole case', !!opened, { ms: Math.round(performance.now() - started), row: opened });
  const picked = document.querySelector('[data-tool-panel="case"] [data-slot="case-step"].bg-selection')?.getAttribute('data-step');
  expect('rom_loss is the step picked', picked === 'rom_loss', picked);
  expect('the band says no ground is made or lost', (document.querySelector('[data-slot="case-band"]')?.textContent ?? '').includes('Balanced'));
  expect('roof loss at 0.1 m is 166,169 m³', (opened?.[1] ?? '').includes('166,169'), opened?.[1]);
  const stages = [...document.querySelectorAll('[data-slot="case-steps"] .uppercase')].map((e) => e.textContent);
  expect('the steps go by stage', stages.includes('In situ') && stages.includes('ROM') && stages.includes('Final'), stages);
  const blocks = document.querySelectorAll('[data-slot="map-block"]').length;
  expect('the block map has every block column', blocks === 90, blocks);
  expect('the strip log shows the picked block', !!document.querySelector('[data-slot="strip-log-after"]'));
  const lit = [...document.querySelectorAll('[data-slot="case-input"][data-lit="yes"]')].map((e) => e.textContent);
  expect('the settings table rom_loss reads is lit', lit.some((t) => t?.includes('Reserves assumptions')), lit);
  await picture?.('case-rom-loss');

  // The coal roof loss typed 0.1 to 0.15 in its row, as a planner types it.
  const cell = await until(() =>
    [...document.querySelectorAll<HTMLInputElement>('[data-slot="settings-by-seam"] input')].find((i) => i.getAttribute('aria-label')?.endsWith(', roof_loss') && i.value === '0.1'),
  );
  expect('the coal row holds roof_loss 0.1 to type in', !!cell);
  if (cell) {
    const typed = performance.now();
    cell.focus();
    const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value')?.set;
    for (const text of ['0.1', '0.15']) {
      setter?.call(cell, text);
      cell.dispatchEvent(new Event('input', { bubbles: true }));
      await wait(60);
    }
    const now = await until(() => {
      const r = caseRow('Roof loss');
      return r && r.length >= 3 && r[2].includes('249,253') ? r : null;
    }, 15_000);
    expect('typed 0.15 m, the whole case runs again: roof loss 249,253 m³', !!now, { ms: Math.round(performance.now() - typed), row: now ?? caseRow('Roof loss') });
    const ran = [...document.querySelectorAll('[data-slot="case-step"]')].filter((e) => /\d+ ms/.test(e.textContent ?? '')).map((e) => e.getAttribute('data-step'));
    expect('only rom_loss ran again', ran.length === 1 && ran[0] === 'rom_loss', ran);
    const rom = caseRow('ROM coal');
    expect('ROM coal is not yet written at rom_loss', rom === null, rom);
    await picture?.('case-roof-015');
    // The same edit is the Parameters tab's: it shows there as edited.
    showPanel(api, 'params');
    await wait(500);
    showPanel(api, 'case');
    cell.blur();
    const discard = await until(() => [...document.querySelectorAll<HTMLButtonElement>('[data-tool-panel="case"] button')].find((b) => b.textContent?.includes('Discard')));
    discard?.click();
    const back = await until(() => {
      const r = caseRow('Roof loss');
      return r && r.length === 2 && r[1].includes('166,169') ? r : null;
    }, 15_000);
    expect('discarded, the case is as saved again', !!back, back ?? caseRow('Roof loss'));
  }
  // At final, ROM coal at 0.1 m, as the table has it.
  const final = document.querySelector<HTMLElement>('[data-tool-panel="case"] [data-slot="case-step"][data-step="final"]');
  final?.click();
  const rom = await until(() => caseRow('ROM coal'), 15_000);
  expect('at final, ROM coal 9,234,489 t', (rom?.[1] ?? '').includes('9,234,489'), rom);
  await picture?.('case-final');
  return { ...results, failures };
}
