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

  // A picture leaves out scroll positions (windowPicture): the step view out of the way for a moment, so the units
  // table under it is in the picture.
  const unitsPicture = async (name: string) => {
    const above = document.querySelector<HTMLElement>('[data-slot="case-columns"]');
    if (above) above.style.display = 'none';
    await picture?.(name);
    if (above) above.style.display = '';
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
  rows.find((r) => r.dataset.left === 'yes')?.click();
  const journey = await until(() => document.querySelector('[data-slot="journey"]')?.textContent, 10_000);
  expect("a piece's journey says the step made it", !!journey && journey.includes('rom_loss: made here'), journey);
  [...document.querySelectorAll<HTMLButtonElement>('[data-slot="units-table"] button')].find((b) => b.textContent === 'Field map')?.click();
  const map = await until(() => document.querySelector('[data-slot="field-map"]')?.textContent, 10_000);
  expect('the field map shows what each step writes', !!map && map.includes('W'), map?.slice(0, 80));
  await unitsPicture('case-units-picked');

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
  const tick = document.querySelector<HTMLElement>('[data-slot="flow-step"][data-step]:not([data-step="0"])');
  const tickStep = tick?.dataset.step;
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
  const ranSaid = await until(() => document.querySelector('[data-slot="case-said"]')?.textContent, 30_000);
  expect("Run what's out of date runs the row dataflow alone", !!ranSaid && ranSaid.startsWith('Ran Check haulage on'), ranSaid);
  const upToDate = await until(() => {
    const r = listRow('Check haulage');
    return r?.dataset.upToDate === 'yes' && listRow('Reserves')?.dataset.upToDate === 'yes' ? r : null;
  }, 15_000);
  expect('then both are up to date', !!upToDate);
  (await opened2())?.querySelector<HTMLButtonElement>('[data-slot="case-open"]')?.click();
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
  return { ...results, failures };
}
