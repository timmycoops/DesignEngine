// `--case <project.zdp>`: the Reserves case tab driven as Tim would (Reserves case plan, workstream 4). The project (the
// demo pit saved by testdata/synthetic/workflow_demo.py) is opened, the tab opened on its Base table at `rom_loss`, the
// coal roof loss typed from 0.1 to 0.15 m in the step's settings by seam and ply, and the whole case read back as it runs
// again; then the edit discarded. With `--pictures <folder>`, the window at each stage.
import type { DockviewApi } from 'dockview-react';
import { openProject, type ProjectSummary, runCommand, tableSetupView, writePicture } from '../bridge';
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
  // The case picked by its pill on the header (Case tab layout plan, decision 1).
  (await until(() => document.querySelector<HTMLElement>(`[data-tool-panel="case"] [data-slot="case-headline"][data-table="${base.id}"]`)))?.click();
  const started = performance.now();
  // The case shown is the Base table's, not the one the tab opened on first.
  const opened = await until(() => document.querySelector(`[data-tool-panel="case"][data-case="${base.id}"]`) && caseRow('Roof loss'));
  expect('the tab opens at rom_loss with the whole case', !!opened, { ms: Math.round(performance.now() - started), row: opened });
  const picked = document.querySelector('[data-tool-panel="case"] [data-slot="case-step"].bg-selection')?.getAttribute('data-step');
  expect('rom_loss is the step picked', picked === 'rom_loss', picked);
  expect('the band says no ground is made or lost', (document.querySelector('[data-slot="case-band"]')?.textContent ?? '').includes('Balanced'));
  expect('roof loss at 0.1 m is 166,169 m³', (opened?.[1] ?? '').includes('166,169'), opened?.[1]);
  // The flow is the step list (Case tab layout plan, decision 5): its stages name their steps.
  const stages = [...document.querySelectorAll<HTMLElement>('[data-slot="flow-stage"]')].filter((e) => e.querySelector('[data-slot="case-step"]')).map((e) => e.dataset.stage);
  expect('the steps go by stage', stages.includes('In situ') && stages.includes('ROM') && stages.includes('Final'), stages);
  const blocks = document.querySelectorAll('[data-slot="map-block"]').length;
  expect('the block map has every block column', blocks === 90, blocks);
  expect('the strip log shows the picked block', !!document.querySelector('[data-slot="strip-log-after"]'));
  // The settings tables a step reads are named on its Reads line, lit (decision 4: the input cards' place).
  const litCards = () => [...document.querySelectorAll('[data-slot="case-reads-table"]')].map((e) => e.textContent);
  const lit = (await until(() => (litCards().length ? litCards() : null), 10_000)) ?? [];
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

  // The units table has its own column filling the height (Case tab layout plan, decision 6), so it's in the picture
  // as it stands.
  const unitsPicture = async (name: string) => {
    await picture?.(name);
  };
  // The units table (decision 12, workstream 5): B5 at rom_loss, its U's pieces adding to what U was, the groups'
  // subtotals the block's, the pieces that left listed red, a field set kept and taken away, a piece's journey.
  document.querySelector<HTMLElement>('[data-tool-panel="case"] [data-slot="case-step"][data-step="rom_loss"]')?.click();
  const b5 = await until(() => document.querySelector<HTMLElement>('[data-slot="map-block"][data-block="Pit 3 B5"]'));
  b5?.click();
  const table = await until(() => {
    const head = document.querySelector('[data-slot="units-table"]')?.textContent ?? '';
    return head.includes('Pit / 3 / B5 before and after rom_loss') && document.querySelectorAll('[data-slot="units-row"]').length > 0 ? head : null;
  }, 20_000);
  expect('the units table shows B5 before and after rom_loss', !!table);

  // The working area by the dock's width (Case tab layout plan, decisions 6 and 8): the tab given the window, then
  // narrowed to 1,399 and 999 px. Step, units table and block map side by side to the foot; under 1,400 the block map
  // under the step; under 1,000 the units table under both and the header's figures in the pill's tip.
  const caseTab = document.querySelector<HTMLElement>('[data-tool-panel="case"]');
  const casePanel = api.getPanel('case');
  if (casePanel && !api.hasMaximizedGroup()) api.maximizeGroup(casePanel);
  const boxOf = (slot: string) => {
    const r = document.querySelector<HTMLElement>(`[data-tool-panel="case"] [data-slot="${slot}"]`)?.getBoundingClientRect();
    return r ? { left: Math.round(r.left), top: Math.round(r.top), right: Math.round(r.right), bottom: Math.round(r.bottom) } : null;
  };
  const layoutAt = async (px: number | null) => {
    if (caseTab) caseTab.style.width = px === null ? '' : `${px}px`;
    await wait(400);
    const units = boxOf('case-units');
    const inSight = [...document.querySelectorAll<HTMLElement>('[data-slot="units-row"]')].filter((r) => {
      const b = r.getBoundingClientRect();
      return units && b.top >= units.top && b.bottom <= units.bottom;
    }).length;
    return {
      width: caseTab?.clientWidth ?? 0,
      height: caseTab?.clientHeight ?? 0,
      fold: document.querySelector<HTMLElement>('[data-slot="case-columns"]')?.dataset.fold,
      step: boxOf('case-step-pane'),
      units,
      blocks: boxOf('case-blocks'),
      tab: caseTab ? { bottom: Math.round(caseTab.getBoundingClientRect().bottom) } : null,
      figures: !!document.querySelector('[data-tool-panel="case"] [data-slot="case-figures"]'),
      rows_in_sight: inSight,
      // Anything on the flow cut off: wider than its box, or ending in an ellipsis (the plan's "nothing cut off").
      cut: [...document.querySelectorAll<HTMLElement>('[data-tool-panel="case"] [data-slot="flow-stage"] *')]
        .filter((e) => e.children.length === 0 && (e.scrollWidth > e.clientWidth + 1 || (e.textContent ?? '').trim().endsWith('…')))
        .map((e) => e.textContent ?? ''),
    };
  };
  const wide = await layoutAt(null);
  expect(
    'wide, step | units table | block map side by side, filling the height',
    wide.fold === 'three' && !!wide.step && !!wide.units && !!wide.blocks && wide.step.right <= wide.units.left + 1 && wide.units.right <= wide.blocks.left + 1 && wide.step.top === wide.units.top && wide.units.top === wide.blocks.top && Math.abs((wide.units.bottom ?? 0) - (wide.tab?.bottom ?? 0)) <= 2 && wide.figures,
    wide,
  );
  expect('wide, nothing on the flow cut off', wide.cut.length === 0, wide.cut);
  expect('wide, the units table shows at least 10 rows', wide.rows_in_sight >= 10, { rows_in_sight: wide.rows_in_sight, height: wide.height });
  await picture?.('case-wide');
  const middle = await layoutAt(1399);
  expect(
    'under 1,400 px, the block map under the step, the units table beside both',
    middle.fold === 'two' && !!middle.step && !!middle.blocks && !!middle.units && middle.blocks.top >= middle.step.bottom - 1 && middle.blocks.left === middle.step.left && middle.units.left >= middle.step.right - 1 && middle.figures,
    middle,
  );
  expect('under 1,400 px, nothing on the flow cut off', middle.cut.length === 0, middle.cut);
  await picture?.('case-1399');
  const narrow = await layoutAt(999);
  expect(
    "under 1,000 px, the units table under both and the figures in the pill's tip",
    narrow.fold === 'one' && !!narrow.step && !!narrow.blocks && !!narrow.units && narrow.blocks.top >= narrow.step.bottom - 1 && narrow.units.top >= narrow.blocks.bottom - 1 && !narrow.figures,
    narrow,
  );
  await picture?.('case-999');
  await layoutAt(null);
  // The picked unit at full width, for the picture (Case setup and unit plan, decisions 5 to 8): a loss piece.
  document.querySelector<HTMLElement>('[data-slot="units-row"][data-left="yes"]')?.click();
  if (await until(() => document.querySelector('[data-slot="picked-piece"]'), 5_000)) {
    await picture?.('case-picked-unit');
    document.querySelector<HTMLElement>('[aria-label="Close the picked unit (Esc)"]')?.click();
    await until(() => !document.querySelector('[data-slot="picked-piece"]'), 5_000);
  }
  if (api.hasMaximizedGroup()) api.exitMaximizedGroup();
  await wait(300);
  const rows = [...document.querySelectorAll<HTMLElement>('[data-slot="units-row"]')];
  const num = (e: HTMLElement, key: string) => (e.dataset[key] ? Number(e.dataset[key]) : 0);
  const kept = rows.find((r) => r.dataset.horizon === 'U' && r.dataset.left === 'no' && r.dataset.volumeBefore);
  if (kept) {
    const id = kept.dataset.from;
    const pieces = rows.filter((r) => r.dataset.left === 'no' && r.dataset.from === id);
    const was = num(kept, 'volumeBefore');
    const sum = pieces.reduce((s, r) => s + num(r, 'volumeAfter'), 0);
    expect("U's pieces add to the volume U was", Math.abs(sum - was) <= 1e-9 * Math.max(1, was), { pieces: pieces.length, was, sum });
  } else expect("U's pieces add to the volume U was", false, 'no U row');
  // Each piece once (a piece that left is listed twice), against the groups' subtotals.
  const unique = new Map(rows.map((r) => [r.dataset.row, r]));
  const block = (side: 'volumeBefore' | 'volumeAfter') => [...unique.values()].reduce((s, r) => s + num(r, side), 0);
  const groups = [...document.querySelectorAll<HTMLElement>('[data-slot="units-group"] [data-field="volume"]')];
  const subtotal = (side: 'before' | 'after') => groups.reduce((s, c) => s + num(c, side), 0);
  for (const [side, key] of [['before', 'volumeBefore'], ['after', 'volumeAfter']] as const) {
    const want = block(key);
    expect(`the groups' volumes add to the block's ${side} the step`, Math.abs(subtotal(side) - want) <= 1e-9 * Math.max(1, want), { groups: subtotal(side), block: want });
  }
  const left = rows.filter((r) => r.dataset.left === 'yes').length;
  expect('the pieces that left their section are listed there in red', left > 0, left);
  await unitsPicture('case-units');
  // A field set of one's own, kept with the project and taken away again.
  const save = [...document.querySelectorAll<HTMLButtonElement>('[data-slot="units-table"] button')].find((b) => b.textContent === 'Save as a set');
  save?.click();
  const name = await until(() => document.querySelector<HTMLInputElement>('input[aria-label="The new field set\'s name"]'));
  if (name) {
    Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value')?.set?.call(name, 'Check set');
    name.dispatchEvent(new Event('input', { bubbles: true }));
    await wait(100);
    [...document.querySelectorAll<HTMLButtonElement>('[data-slot="units-table"] button')].find((b) => b.textContent === 'Save')?.click();
  }
  const listed = await until(() => [...document.querySelectorAll<HTMLOptionElement>('select[aria-label="The field set shown"] option')].some((o) => o.value === 'Check set'), 10_000);
  expect('a field set saved with the project shows in the list', !!listed);
  [...document.querySelectorAll<HTMLButtonElement>('[data-slot="units-table"] button')].find((b) => b.textContent === 'Delete Check set')?.click();
  const gone = await until(() => ![...document.querySelectorAll<HTMLOptionElement>('select[aria-label="The field set shown"] option')].some((o) => o.value === 'Check set'), 10_000);
  expect('and deleted, it is gone', !!gone);
  // The picked piece: a loss piece, made at rom_loss.
  const lossRow = rows.find((r) => r.dataset.left === 'yes');
  const lossKey = lossRow?.dataset.row;
  lossRow?.click();
  const journey = await until(() => document.querySelector('[data-slot="journey"]')?.textContent, 10_000);
  expect("a piece's journey says the step made it", !!journey && journey.includes('rom_loss: made here'), journey);
  // The picked unit (Case setup and unit plan, decisions 5 to 8): under its row, across the table, said plainly.
  const sheetRow = document.querySelector<HTMLTableRowElement>('[data-slot="units-table"] [data-slot="picked-piece-row"]');
  const above = sheetRow?.previousElementSibling as HTMLElement | null;
  const across = (sheetRow?.querySelector('td')?.colSpan ?? 0) === document.querySelectorAll('[data-slot="units-grid"] thead th').length;
  expect('the picked unit opens under its row, across the units table', !!sheetRow && above?.dataset.slot === 'units-row' && above.dataset.row === lossKey && across, { under: above?.dataset.row, picked: lossKey, across });
  const sheetTitle = document.querySelector('[data-slot="picked-piece-title"]')?.textContent ?? '';
  expect('its title names the unit, its block and the step', sheetTitle.includes('B5') && sheetTitle.includes('rom_loss') && /step \d+/.test(sheetTitle), sheetTitle);
  const sheet = document.querySelector<HTMLElement>('[data-slot="picked-piece"]');
  const writers = [...(sheet?.querySelectorAll<HTMLElement>('[data-slot="piece-field"]') ?? [])].map((e) => e.dataset.writer);
  expect(
    'a unit the step made has no Before, every field written by this step',
    sheet?.dataset.made === 'yes' && !sheet.querySelector('[data-slot="piece-before-head"]') && writers.length > 0 && writers.every((w) => w === 'this step'),
    { made: sheet?.dataset.made, writers: [...new Set(writers)] },
  );
  const names = [...(sheet?.querySelectorAll<HTMLElement>('[data-slot="piece-field-name"]') ?? [])];
  const tall = names.filter((n) => n.getBoundingClientRect().height > 22 || (n.textContent ?? '').includes('\\')).map((n) => n.textContent);
  const pathFields = [...(sheet?.querySelectorAll<HTMLElement>('[data-slot="piece-field"]') ?? [])].filter((e) => (e.dataset.field ?? '').includes('\\')).length;
  const headings = sheet?.querySelectorAll('[data-slot="piece-group-path"]').length ?? 0;
  expect('no field name wraps, and one named as a path sits under its heading', names.length > 0 && tall.length === 0 && (pathFields === 0 || headings > 0), { names: names.length, tall, pathFields, headings });
  sheet?.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
  const sheetGone = await until(() => !document.querySelector('[data-slot="picked-piece"]'), 5_000);
  expect('Esc closes the picked unit', !!sheetGone);
  [...document.querySelectorAll<HTMLButtonElement>('[data-slot="units-table"] button')].find((b) => b.textContent === 'Field map')?.click();
  const map = await until(() => document.querySelector('[data-slot="field-map"]')?.textContent, 10_000);
  expect('the field map shows what each step writes', !!map && map.includes('W'), map?.slice(0, 80));
  await unitsPicture('case-units-picked');

  // From the Dataflow tab (workstream 7): rom_loss unit by unit, the unit followed's workings; and Run again on B5
  // alone, into the table.
  document.querySelector<HTMLElement>('[data-slot="strip-log-before"] button[aria-label^="Follow U"]')?.click();
  await wait(300);
  document.querySelector<HTMLElement>('[data-slot="case-unit-by-unit"] button')?.click();
  const workings = await until(() => {
    const w = [...document.querySelectorAll<HTMLElement>('[data-slot="case-unit-by-unit"] [data-slot="working"]')].map((e) => e.dataset.what ?? '');
    return w.length ? w : null;
  }, 15_000);
  expect('rom_loss unit by unit shows the unit followed’s workings', !!workings && workings.some((w) => w.includes('loss')), workings ?? document.querySelector('[data-slot="case-unit-by-unit"]')?.textContent?.slice(0, 300));
  document.querySelector<HTMLElement>('[data-slot="case-unit-by-unit"] button')?.click();
  [...document.querySelectorAll<HTMLButtonElement>('[data-slot="case-run-again"] button')].find((b) => b.textContent?.trim() === 'this block')?.click();
  const reran = await until(() => document.querySelector('[data-slot="case-said"]')?.textContent, 60_000);
  expect('Run again on the block runs B5 alone into the table', !!reran && reran.startsWith('Pit › 3 › B5 run again into Reserves Base'), reran);

  // The flow (decision 10, workstream 6): the case through every stage, each stage's coal what the one before left less
  // what it took, ROM taking coal and letting rock in; a step's tick goes there.
  const stagesShown = () =>
    [...document.querySelectorAll<HTMLElement>('[data-slot="flow-stage"]')].map((e) => ({ name: e.dataset.stage ?? '', coal: Number(e.dataset.coal), lost: Number(e.dataset.lost), letIn: Number(e.dataset.letIn), coalM3: Number(e.dataset.coalM3), lostM3: Number(e.dataset.lostM3) }));
  const flow = await until(() => (stagesShown().length > 3 ? stagesShown() : null), 10_000);
  expect('the flow shows the blocks and every stage', !!flow && flow[0].name === 'Blocks' && flow.some((s) => s.name === 'ROM'), flow?.map((s) => s.name));
  if (flow) {
    const rom = flow.find((s) => s.name === 'ROM')!;
    expect('ROM takes coal and lets rock in', rom.lost > 0 && rom.letIn > 0, rom);
    // Coal carried on: each stage's coal is the one before less what it took (m³, 1e-9).
    const carried = flow.slice(1).every((s, i) => Math.abs(flow[i].coalM3 - s.lostM3 - s.coalM3) <= 1e-9 * flow[0].coalM3);
    expect('the blocks and in situ take no coal', flow[1].lost === 0, flow.slice(0, 2));
    expect("each stage's coal is what came in less what it took", carried, flow);
  }
  const tick = document.querySelector<HTMLElement>('[data-slot="flow-stage"] [data-slot="case-step"][data-n]:not([data-n="0"])');
  const tickStep = tick?.dataset.n;
  tick?.click();
  const moved = await until(() => document.querySelector('[data-tool-panel="case"] [data-slot="case-step"].bg-selection') && document.querySelector('[data-slot="case-step-pane"] .text-sm')?.textContent?.startsWith(`${tickStep} ·`), 10_000);
  expect("a step's tick in the flow goes to it", !!moved, tickStep);
  document.querySelector<HTMLElement>('[data-tool-panel="case"] [data-slot="case-step"][data-step="rom_loss"]')?.click();
  await wait(800);
  await picture?.('case-flow');

  // Open another dataflow (decisions 4 and 6): a row dataflow added to the case, Haulage on strips 1 to 3, out of date
  // until it runs; Run what's out of date runs it alone; opened, its rows before and after its steps, a distance typed.
  const made = (r: Awaited<ReturnType<typeof runCommand>>) => r.record.outputs.find((o) => o.change === 'created')?.id ?? '';
  try {
    const distances = made(await runCommand('parameters.save', { name: 'Check haul distances', csv: 'when strip,distance (km)\n1-2,2\n,3\n' }));
    const content = {
      rows: true,
      fields: [
        { name: 'distance', kind: 'other', add_up: { by: 'max' }, from: 'worked' },
        { name: 'tkm', kind: 'other', add_up: { by: 'sum' }, from: 'worked' },
      ],
      units: { distance: 'km', tkm: 't.km' },
      steps: [
        { step: 'distance', formula: { script: 'def formula(row, p):\n    row.distance = p.distance\n' } },
        { step: 'tkm', formula: { script: 'def formula(row, p):\n    t = row["ROM\\\\Coal\\\\tonnes"]\n    row.tkm = None if t == None else t * row.distance\n' } },
      ],
      parameters: [distances],
    };
    const haul = made(await runCommand('dataflow.save', { name: 'Check haulage', content }));
    const setup = (await tableSetupView(base.id)).setup;
    const reply = await runCommand('table.setup', { table: base.id, setup: { ...setup, row_dataflows: [{ dataflow: haul, range: { strip: '1-3' } }] } });
    await show(reply.summary);
  } catch (e) {
    expect('a row dataflow added to the case', false, String(e));
    return { ...results, failures };
  }
  const listRow = (name: string) => document.querySelector<HTMLElement>(`[data-slot="case-dataflow-row"][data-dataflow="${CSS.escape(name)}"]`);
  const switcher = await until(() => document.querySelector<HTMLElement>('[data-slot="case-switch"]'));
  const opened2 = async () => {
    if (!document.querySelector('[data-slot="case-dataflows"]')) switcher?.click();
    return until(() => listRow('Check haulage'), 10_000);
  };
  const row = await opened2();
  expect('Open another dataflow lists the case in run order', !!row && [...document.querySelectorAll<HTMLElement>('[data-slot="case-dataflow-row"]')].map((e) => e.dataset.dataflow).join(',') === 'Reserves,Check haulage', [...document.querySelectorAll<HTMLElement>('[data-slot="case-dataflow-row"]')].map((e) => e.dataset.dataflow));
  expect('the new row dataflow is out of date: it has not run', row?.dataset.upToDate === 'no' && (row.textContent ?? '').includes("hasn't run"), row?.textContent);
  expect('its range and settings are said', (row?.textContent ?? '').includes('strip 1-3') && (row?.textContent ?? '').includes('Check haul distances'), row?.textContent);
  await picture?.('case-dataflows');
  document.querySelector<HTMLButtonElement>('[data-slot="case-run-stale"]')?.click();
  const ranSaid = await until(() => {
    const t = document.querySelector('[data-slot="case-said"]')?.textContent ?? '';
    return t.startsWith('Ran ') || t.startsWith('Nothing') ? t : null;
  }, 30_000);
  expect("Run what's out of date runs the row dataflow alone", !!ranSaid && ranSaid.startsWith('Ran Check haulage on'), ranSaid);
  const upToDate = await until(() => {
    const r = listRow('Check haulage');
    return r?.dataset.upToDate === 'yes' && listRow('Reserves')?.dataset.upToDate === 'yes' ? r : null;
  }, 15_000);
  expect('then both are up to date', !!upToDate);
  // The case's dataflows are a chain on the header (decision 3): a click on one opens it.
  document.querySelector<HTMLElement>('[data-slot="case-band"] [data-slot="case-chain-link"][data-dataflow="Check haulage"]')?.click();
  const rowView = await until(() => document.querySelector('[data-slot="row-dataflow"]'), 15_000);
  expect('Open shows the row dataflow', !!rowView && (document.querySelector('[data-slot="case-dataflow"]')?.textContent ?? '').includes('Check haulage'));
  const rowStep = (name: string) => document.querySelector<HTMLElement>(`[data-slot="row-dataflow"] [data-slot="case-step"][data-step="${name}"]`)?.click();
  rowStep('tkm');
  const tkm = () => document.querySelector<HTMLElement>('[data-slot="row-totals"] tr[data-column$="tkm"]');
  const ranTkm = await until(() => {
    const r = tkm();
    return r && r.dataset.after ? r : null;
  }, 15_000);
  const cells = ranTkm ? [...ranTkm.querySelectorAll('td')].map((td) => td.textContent ?? '') : [];
  expect('at tkm, its tonne-kilometres after the step are what its last run wrote', !!ranTkm && cells[1] === '—' && cells[2] === cells[3] && cells[2] !== '—', cells);
  const was = Number(ranTkm?.dataset.after);
  // Strips 1 and 2 typed from 2 to 4 km at the distance step: the tonne-kilometres run again, not saved.
  rowStep('distance');
  const km = await until(() => [...document.querySelectorAll<HTMLInputElement>('[data-slot="row-dataflow"] [data-slot="settings-by-seam"] input')].find((i) => i.value === '2'));
  if (km) {
    km.focus();
    Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value')?.set?.call(km, '4');
    km.dispatchEvent(new Event('input', { bubbles: true }));
    await wait(100);
    rowStep('tkm');
    const now = await until(() => {
      const v = Number(tkm()?.dataset.after);
      return v && Math.abs(v - was) > 1 ? v : null;
    }, 15_000);
    expect('typed 4 km on strips 1 and 2, the tonne-kilometres run again', !!now && now > was, { was, now });
    await picture?.('case-row-dataflow');
    km.blur();
    [...document.querySelectorAll<HTMLButtonElement>('[data-slot="row-dataflow"] button')].find((b) => b.textContent?.includes('Discard'))?.click();
    const back = await until(() => (Math.abs(Number(tkm()?.dataset.after) - was) <= 1e-6 * Math.max(1, was) ? true : null), 15_000);
    expect('discarded, as saved again', !!back);
  } else expect('the distance setting is there to type', false);

  // A way in (decision 13, workstream 7): the cases against Base; Copy case; Add a dataflow; the Setup under the band;
  // New case; and no Dataflow or Setup tab left.
  const tabs = [...document.querySelectorAll<HTMLElement>('.dv-tab')].map((t) => t.textContent?.trim() ?? '');
  expect('the Dataflow and Setup tabs are gone', !tabs.includes('Dataflow') && !tabs.includes('Setup'), tabs);
  const caseRows = () => [...document.querySelectorAll<HTMLElement>('[data-slot="case-headline"]')];
  const rowOf = (name: string) => caseRows().find((r) => r.dataset.case === name) ?? null;
  // The cases are pills on the header (decision 1), each carrying its four figures.
  const figure = (name: string, field: string) => Number(rowOf(name)?.getAttribute(`data-${field}`) ?? NaN);
  // New case, Copy case and Add a dataflow are under + (decision 2).
  const fromMore = async (slot: string) => {
    if (!document.querySelector('[data-slot="case-more-menu"]')) document.querySelector<HTMLElement>('[data-slot="case-more"]')?.click();
    (await until(() => document.querySelector<HTMLElement>(`[data-slot="case-more-menu"] [data-slot="${slot}"]`)))?.click();
  };
  await until(() => caseRows().length >= 2, 10_000);
  expect('the cases are listed, Base marked', rowOf('Reserves Base')?.dataset.base === 'yes' && !!rowOf('Reserves 50m'), caseRows().map((r) => r.dataset.case));
  expect("Base's ROM coal is the table's, 9,234,489 t", Math.abs(figure('Reserves Base', 'rom_coal') - 9_234_489) < 1, figure('Reserves Base', 'rom_coal'));
  const said7 = () => document.querySelector('[data-slot="cases-said"]')?.textContent ?? '';
  const typeIn = async (label: string, text: string) => {
    const box = await until(() => document.querySelector<HTMLInputElement>(`input[aria-label="${CSS.escape(label)}"]`));
    if (!box) return false;
    Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value')?.set?.call(box, text);
    box.dispatchEvent(new Event('input', { bubbles: true }));
    await wait(100);
    return true;
  };

  // Copy case: Base as Check copy, sharing its settings tables; its row dataflow copied, out of date until it runs.
  await fromMore('case-copy');
  await typeIn("The copy's name", 'Check copy');
  document.querySelector<HTMLElement>('[data-slot="copy-case-make"]')?.click();
  const copied = await until(() => rowOf('Check copy'), 90_000);
  expect('Copy case makes Check copy from Base, its figures the same', !!copied && Math.abs(figure('Check copy', 'rom_coal') - figure('Reserves Base', 'rom_coal')) < 1e-6 * figure('Reserves Base', 'rom_coal'), { said: said7(), rom: figure('Check copy', 'rom_coal') });
  const openNow = await until(() => rowOf('Check copy')?.classList.contains('bg-selection'), 20_000);
  expect('the copy is the case open', !!openNow);
  if (!document.querySelector('[data-slot="case-dataflows"]')) document.querySelector<HTMLElement>('[data-slot="case-switch"]')?.click();
  const copiedRow = await until(
    () => (document.querySelector('[data-slot="case-dataflows"]')?.textContent ?? '').startsWith('Check copy') && document.querySelector<HTMLElement>('[data-slot="case-dataflow-row"][data-dataflow="Check haulage"]'),
    15_000,
  );
  expect("the copy keeps Base's row dataflow, out of date until it runs", copiedRow?.dataset.upToDate === 'no', copiedRow?.textContent?.slice(0, 120));
  document.querySelector<HTMLElement>('[data-slot="case-switch"]')?.click();

  // Add a dataflow: a first step to write over, after the copy's, opened.
  await fromMore('case-add-dataflow');
  await typeIn("The dataflow's name", 'Check added');
  document.querySelector<HTMLElement>('[data-slot="add-dataflow-make"]')?.click();
  // Its steps to write: the Parameters tab opens it, checked as a row dataflow, not a reserves one.
  const editing = await until(() => {
    const p = document.querySelector<HTMLElement>('[data-tool-panel="parameters"]');
    return p && (p.textContent ?? '').includes('Check added') ? p : null;
  }, 30_000);
  await wait(800);
  const refused = editing?.textContent?.includes('volume field') ?? true;
  expect('Add a dataflow opens Check added in the Parameters tab to write, with no reserves field asked for', !!editing && !refused, editing?.textContent?.slice(0, 160));
  showPanel(api, 'case');
  const added = await until(() => (document.querySelector('[data-slot="case-dataflow"]')?.textContent ?? '').includes('Check added') && document.querySelector('[data-slot="row-dataflow"]'), 30_000);
  expect('Add a dataflow puts Check added after the case’s and opens it in the case', !!added, said7());

  // The Setup under the band: the dataflows after the reserves one, in run order.
  document.querySelector<HTMLElement>('[data-slot="case-setup"]')?.click();
  const afterIt = await until(() => {
    const items = [...document.querySelectorAll<HTMLElement>('[data-slot="setup-row-dataflows"] [data-item]')].map((e) => e.dataset.item);
    return items.length >= 2 ? items : null;
  }, 15_000);
  expect("the case's Setup lists its dataflows after the reserves one, in run order", JSON.stringify(afterIt) === JSON.stringify(['Check haulage', 'Check added']), afterIt);
  await picture?.('case-way-in');

  // The Setup as its own view (Case setup and unit plan, decisions 1 to 4).
  const setupBox = document.querySelector<HTMLElement>('[data-tool-panel="case"] [data-slot="setup"]');
  const inPanel = (slot: string) => !!document.querySelector(`[data-tool-panel="case"] [data-slot="${slot}"]`);
  expect(
    'the Setup is its own view: framed with Done, the flow, the working area and the row dataflow out of sight',
    !!setupBox && inPanel('setup-title') && inPanel('setup-done') && !inPanel('case-flow') && !inPanel('case-columns') && !inPanel('row-dataflow'),
    { title: inPanel('setup-title'), flow: inPanel('case-flow'), columns: inPanel('case-columns'), row: inPanel('row-dataflow') },
  );
  const setupRuns = [...(setupBox?.querySelectorAll('button') ?? [])].map((b) => b.textContent?.trim() ?? '').filter((t) => /\bRun\b/.test(t));
  expect('the Setup has no Run of its own', setupRuns.length === 0, setupRuns);
  const sections = [...(setupBox?.querySelectorAll<HTMLElement>('[data-section]') ?? [])];
  const numbered = sections.map((e) => Number(e.dataset.section));
  expect(
    "the Setup's sections go 1 to 6 in the order a run uses them, each saying what it decides",
    JSON.stringify(numbered) === '[1,2,3,4,5,6]' && sections.every((e) => (e.querySelector('[data-slot="setup-line"]')?.textContent ?? '').length > 20),
    numbered,
  );
  // A settings table read for more than twelve settings folds to its count, opened on a click.
  const folds = [...(setupBox?.querySelectorAll<HTMLElement>('[data-slot="setup-tables"] [data-slot="setup-names"]') ?? [])];
  const fold = folds[0];
  const count = Number(fold?.dataset.count ?? 0);
  fold?.querySelector('button')?.click();
  const namesOpen = await until(() => fold?.querySelector('[data-slot="setup-names-open"]'), 3_000);
  const namesListed = namesOpen?.querySelectorAll('span').length ?? 0;
  expect('a settings table read for more than 12 settings folds to its count, the names on a click', !!fold && count > 12 && (fold.textContent ?? '').includes(`${count} settings`) && namesListed === count, { count, listed: namesListed });
  // One place to run the whole case: the reserves dataflow's Run in Run ▾ is every block again; the map keeps its parts.
  document.querySelector<HTMLElement>('[data-tool-panel="case"] [data-slot="case-switch"]')?.click();
  const everyBlock = await until(() => {
    const row = [...document.querySelectorAll<HTMLElement>('[data-slot="case-dataflow-row"]')][0];
    return row?.querySelector('[data-slot="case-run-one"]')?.textContent?.includes('Every block again') ? row : null;
  }, 5_000);
  expect("Run's reserves dataflow runs every block again", !!everyBlock);
  await picture?.('case-setup-view');
  document.querySelector<HTMLElement>('[data-tool-panel="case"] [data-slot="case-switch"]')?.click();
  // Done: back to the case as it was, the row dataflow open as before.
  document.querySelector<HTMLElement>('[data-slot="setup-done"]')?.click();
  const back = await until(() => !inPanel('setup') && inPanel('row-dataflow'), 5_000);
  expect('Done leaves the Setup for the case as it was', !!back);
  // Esc does the same.
  document.querySelector<HTMLElement>('[data-slot="case-setup"]')?.click();
  await until(() => inPanel('setup-title'), 5_000);
  const doneButton = document.querySelector<HTMLElement>('[data-slot="setup-done"]');
  doneButton?.focus();
  doneButton?.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
  const escaped = await until(() => !inPanel('setup') && inPanel('row-dataflow'), 5_000);
  expect('Esc leaves the Setup too', !!escaped);

  // New case: every block on the Base table's layer, the demo's reserves dataflow.
  await fromMore('case-new');
  await typeIn("The new case's name", 'Check new');
  const flowPick = await until(() => {
    const sel = document.querySelector<HTMLSelectElement>('select[aria-label="The reserves dataflow it runs"]');
    return sel && [...sel.options].some((o) => o.textContent === 'Reserves') ? sel : null;
  }, 15_000);
  const layerPick = document.querySelector<HTMLSelectElement>('select[aria-label="The layer of blocks it runs on"]');
  const blocksLayer = [...(layerPick?.options ?? [])].find((o) => o.textContent === '9 blocks');
  for (const [sel, value] of [
    [layerPick, blocksLayer?.value],
    [flowPick, [...(flowPick?.options ?? [])].find((o) => o.textContent === 'Reserves')?.value],
  ] as const) {
    if (!sel || !value) continue;
    Object.getOwnPropertyDescriptor(HTMLSelectElement.prototype, 'value')?.set?.call(sel, value);
    sel.dispatchEvent(new Event('change', { bubbles: true }));
    await wait(300);
  }
  const blocksSaid = await until(() => {
    const t = document.querySelector('[data-slot="new-case-blocks"]')?.textContent ?? '';
    return t.startsWith('0 ') ? null : t;
  }, 15_000);
  document.querySelector<HTMLElement>('[data-slot="new-case-make"]')?.click();
  const fresh = await until(() => rowOf('Check new'), 180_000);
  expect('New case runs the dataflow on the blocks picked and lists Check new', !!fresh && figure('Check new', 'rom_coal') > 0, { blocks: blocksSaid, said: said7(), rom: figure('Check new', 'rom_coal') });
  return { ...results, failures };
}
