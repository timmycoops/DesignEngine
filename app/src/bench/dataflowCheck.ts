// `--dataflow <project.zdp>`: Properties' views of a parameter table, a dataflow and a results table (decisions 0072,
// 0073), read as Tim would see them. The project (the demo pit saved by testdata/synthetic/workflow_demo.py, its
// dataflow the starter's) is opened, each object clicked in the Layers panel, and what Properties shows read back.
import type { DockviewApi } from 'dockview-react';
import { objectDetails, openProject, type ProjectSummary, reservesBreakdown, reservesNodeOf, reservesSolids, reservesTree, undo, writePicture } from '../bridge';
import type { Viewport } from '../viewport/Viewport';
import { windowPicture } from './windowPicture';
import { showPanel } from '../workspace/Workspace';

const wait = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));

type Show = (summary: ProjectSummary) => Promise<void> | undefined;

/** With `--pictures <folder>`, the window as it stands, saved there as `<name>.png`. */
type Picture = ((name: string) => Promise<void>) | null;

export async function checkDataflow(path: string, log: (line: string) => void, show: Show, dock: () => DockviewApi | null, viewport: Viewport | null = null) {
  const picture: Picture = viewport
    ? async (name) => {
        await wait(600);
        log(`dataflow: ${await writePicture(name, await windowPicture(viewport))}`);
      }
    : null;
  const api = dock();
  if (!api) return { error: 'no dock' };
  showPanel(api, 'layers');
  showPanel(api, 'properties');
  const results: Record<string, unknown> = {};
  const failures: string[] = [];
  const expect = (what: string, ok: boolean, detail?: unknown) => {
    results[what] = detail === undefined ? ok : { ok, detail };
    if (!ok) failures.push(what);
    log(`dataflow: ${what}: ${ok ? 'yes' : 'NO'}${detail === undefined ? '' : ` (${JSON.stringify(detail)})`}`);
  };
  log(`dataflow: opening ${path}`);
  const summary = await openProject(path);
  await show(summary);
  await wait(800);
  const kinds = new Map(summary.objects.map((o) => [o.name, o.kind]));
  expect('the project holds a parameter table, a dataflow and two results tables', kinds.get('Reserves assumptions') === 'parameter_table' && kinds.get('Reserves') === 'dataflow' && kinds.get('Reserves Base') === 'table' && kinds.get('Reserves 50m') === 'table');

  const rows = () => [...document.querySelectorAll<HTMLElement>('[data-slot="tree-row"]')];
  /** Clicks an object's row in the Layers panel, once it's drawn, and gives back the page's text once Properties has it. */
  const shown = async (name: string, waitFor: string) => {
    let row: HTMLElement | undefined;
    for (let k = 0; k < 100 && !row; k++) {
      // Its name, then any revision badge (r2).
      row = rows().find((r) => {
        const text = r.textContent?.trim() ?? '';
        return text === name || (text.startsWith(name) && /^r\d+$/.test(text.slice(name.length)));
      });
      if (!row) await wait(100);
    }
    if (!row) {
      results[`rows seen looking for ${name}`] = rows().map((r) => r.textContent?.trim()).slice(0, 40);
      return null;
    }
    row.click();
    for (let k = 0; k < 50 && !(document.body.textContent ?? '').includes(waitFor); k++) await wait(100);
    return document.body.textContent ?? '';
  };

  const table = await shown('Reserves assumptions', 'Read top down');
  expect('a parameter table shows its filters, values and rows', !!table && table.includes('Filters on') && table.includes('when material') && table.includes('min_parting (m)') && table.includes('Save as CSV'));
  const flow = await shown('Reserves', 'Adds up');
  expect(
    'a dataflow shows its steps, its parameter table by name and its fields',
    // The starter's formulas after initialise, qualities weighed by air-dried tonnes Spry's way (decision 0077, 7c).
    !!flow && flow.includes('small_volumes, initialise, fill_size, limits, densities, in_situ, join_benches, discard, working_sections') && flow.includes('rom, rom_qualities, final') && flow.includes('Reserves assumptions') && flow.includes('by tonnes (rd_adt)') && flow.includes('attribute mis'),
  );
  // The Layers list draws only the rows in view: the other layers closed, as Tim would, so the results layer's rows show.
  for (let k = 0; k < 100; k++) {
    // Only the layers' own open and close buttons in the tree, never a panel's.
    const open = [...document.querySelectorAll<HTMLElement>('[aria-expanded="true"][aria-label^="Close "]')].find((b) => b.getAttribute('aria-label') !== 'Close 10 reserves');
    if (!open) break;
    open.click();
    await wait(60);
  }
  await wait(300);
  const base = await shown('Reserves Base', 'Insitu by seam');
  for (let k = 0; k < 50 && !(document.body.textContent ?? '').includes('Working section by seam'); k++) await wait(100);
  const parts = ['Reserves table', 'Imported by seam', 'Insitu by seam', 'Insitu by bench', 'Working section by seam', 'Loss by seam', 'Dilution by seam', 'ROM by seam', 'Scheduled by seam', 'Clean coal by seam', 'Total', '6,461,308', 'tonnes ad t'].map((w) => [w, !!(document.body.textContent ?? '').includes(w)] as const);
  expect('a reserves table shows each top folder by seam and by bench, with its totals', parts.every(([, ok]) => ok), { found: !!base, parts, around: base ? base.slice(Math.max(0, base.indexOf('Reserves table') - 50), base.indexOf('Reserves table') + 400) : null });

  // The Reserves table tab (decision 0075): fields as rows, a node's children across, as Tim reads it.
  showPanel(api, 'restable');
  const tables = summary.objects.filter((o) => o.kind === 'table');
  const baseTable = tables.find((t) => t.name === 'Reserves Base');
  const fiftyTable = tables.find((t) => t.name === 'Reserves 50m');
  const panel = () => document.querySelector<HTMLElement>('[data-tool-panel="reserves-table"]');
  for (let k = 0; k < 50 && !panel(); k++) await wait(100);
  const pick = panel()?.querySelector<HTMLSelectElement>('select[aria-label="Reserves table"]');
  if (pick && baseTable) {
    // React reads a select's value through its own setter.
    Object.getOwnPropertyDescriptor(HTMLSelectElement.prototype, 'value')?.set?.call(pick, baseTable.id);
    pick.dispatchEvent(new Event('change', { bubbles: true }));
  }
  const nodeRow = (path: string) => panel()?.querySelector<HTMLElement>(`[data-slot="reserves-node"][data-path="${path}"]`) ?? null;
  for (let k = 0; k < 50 && !nodeRow('Pit > 3'); k++) await wait(100);
  nodeRow('Pit > 3')?.querySelector<HTMLButtonElement>('button')?.click();
  for (let k = 0; k < 50 && !nodeRow('Pit > 3 > B1'); k++) await wait(100);
  nodeRow('Pit > 3 > B1')?.click();
  const heads = () => [...(panel()?.querySelectorAll<HTMLElement>('[data-slot="reserves-breakdown"] th[data-col]') ?? [])].map((h) => h.dataset.col ?? '');
  for (let k = 0; k < 50 && heads()[0] !== 'Pit > 3 > B1'; k++) await wait(100);
  expect("a block's fields run down the side, its seams across", JSON.stringify(heads()) === JSON.stringify(['Pit > 3 > B1', 'Pit > 3 > B1 > U', 'Pit > 3 > B1 > M', 'Pit > 3 > B1 > L']), heads());
  const cells = (field: string) => [...(panel()?.querySelectorAll<HTMLElement>(`tr[data-field="${CSS.escape(field)}"] td`) ?? [])].slice(1).map((c) => c.textContent ?? '');
  expect("its ROM coal is the engine's: 64,519.3 m³, its seams 10,840.1, 18,541.2 and 35,137.9", JSON.stringify(cells('ROM\\Coal\\volume')) === JSON.stringify(['64,519.3', '10,840.1', '18,541.2', '35,137.9']), cells('ROM\\Coal\\volume'));
  expect('each stage folder is a group of rows', ['Imported', 'Insitu', 'Loss', 'Dilution', 'ROM', 'Scheduled', 'Clean coal'].every((f) => !!panel()?.querySelector(`tr[data-folder="${f}"]`)));
  // A seam opened to its benches.
  panel()?.querySelector<HTMLButtonElement>('button[aria-label="Open U"]')?.click();
  for (let k = 0; k < 50 && heads().length < 8; k++) await wait(100);
  expect('a seam opens to its benches, top down', JSON.stringify(heads().slice(1, 6)) === JSON.stringify(['Pit > 3 > B1 > U', 'Pit > 3 > B1 > U > 130', 'Pit > 3 > B1 > U > 120', 'Pit > 3 > B1 > U > 110', 'Pit > 3 > B1 > U > 100']), heads());
  // Across leaves (Tim, 16 November): every leaf top of the stack to the bottom, no seam subtotals, adding up to the block.
  [...(panel()?.querySelectorAll<HTMLButtonElement>('button') ?? [])].find((b) => b.textContent === 'Leaves')?.click();
  for (let k = 0; k < 50 && heads().includes('Pit > 3 > B1 > U'); k++) await wait(100);
  const leafHeads = heads();
  const leafLabels = [...(panel()?.querySelectorAll<HTMLElement>('[data-slot="reserves-breakdown"] th[data-col]') ?? [])].map((h) => h.textContent ?? '');
  const leafVolumes = cells('ROM\\Coal\\volume');
  const leafSum = leafVolumes.slice(1).reduce((s, v) => s + (Number(v.replaceAll(',', '')) || 0), 0);
  expect(
    'across leaves: the U benches first, then M then L, no seam subtotals, adding up to the block',
    leafHeads.length > 4 && leafHeads.slice(1).every((h) => h.split(' > ').length === 5) && leafLabels[1] === 'U 130' && leafLabels[leafLabels.length - 1]?.startsWith('L') === true && Math.abs(leafSum - 64_519.3) < 0.05 * leafHeads.length,
    { leafLabels, leafSum },
  );
  [...(panel()?.querySelectorAll<HTMLButtonElement>('button') ?? [])].find((b) => b.textContent === 'Children')?.click();
  for (let k = 0; k < 50 && !heads().includes('Pit > 3 > B1 > U'); k++) await wait(100);
  // Its solids picked from the table, and one picked in the view found in it.
  [...(panel()?.querySelectorAll<HTMLButtonElement>('button') ?? [])].find((b) => b.textContent?.includes('Select its solids'))?.click();
  for (let k = 0; k < 50 && !(panel()?.textContent ?? '').includes('solids picked'); k++) await wait(100);
  const picked = /([\d,]+) solids? picked/.exec(panel()?.textContent ?? '')?.[1];
  expect("Select its solids picks the block's 21 solids", picked === '21', picked);
  if (baseTable) {
    const ids = await reservesSolids(baseTable.id, ['Pit', '4', 'B2', 'M']);
    const where = ids.length ? await reservesNodeOf(baseTable.id, ids[0]) : null;
    expect('a solid is found where the run put it', !!where && where.slice(0, 4).join('>') === 'Pit>4>B2>M', where);
  }
  // Timings on the 50m case's 3,027 leaves: the tree and the pit's breakdown, then a block's.
  if (fiftyTable) {
    let t = performance.now();
    const tree = await reservesTree(fiftyTable.id);
    const treeMs = performance.now() - t;
    t = performance.now();
    const pit = await reservesBreakdown(fiftyTable.id, ['Pit'], []);
    const pitMs = performance.now() - t;
    t = performance.now();
    await reservesBreakdown(fiftyTable.id, ['Pit', '3', 'B1'], [['Pit', '3', 'B1', 'U']]);
    const blockMs = performance.now() - t;
    t = performance.now();
    const pitLeaves = await reservesBreakdown(fiftyTable.id, ['Pit'], [], true);
    const pitLeavesMs = performance.now() - t;
    expect('the pit across leaves shows the first 500 and counts the rest', pitLeaves.columns.length === 501 && pitLeaves.hidden === tree.leaves - 500, { columns: pitLeaves.columns.length, hidden: pitLeaves.hidden, leavesMs: Math.round(pitLeavesMs) });
    const vol = pit.fields.find((f) => f.column === 'ROM\\Coal\\volume');
    const sum = vol ? pit.columns.reduce((s, c, i) => (c.depth === 1 ? s + (vol.values[i] ?? 0) : s), 0) : NaN;
    expect('the pit equals its strips added up', !!vol && Math.abs((vol.values[0] ?? 0) - sum) <= 1e-9 * Math.abs(sum), { pit: vol?.values[0], strips: sum });
    expect('the 50m table opens in under 200 ms and a block in under 100 ms', treeMs + pitMs < 200 && blockMs < 100, { leaves: tree.leaves, treeMs: Math.round(treeMs), pitMs: Math.round(pitMs), blockMs: Math.round(blockMs) });
  }
  await checkParameters(summary, api, expect, show, picture);
  return { ...results, failures };
}

type Expect = (what: string, ok: boolean, detail?: unknown) => void;

/**
 * The Parameters tab (decision 0075, 5d) on the demo pit's assumptions: the solids Select its solids picked mark the cells
 * their values come from, as the engine traces them; a word in a number column stops the save; an edit saves as the next
 * revision and an undo puts it back.
 */
async function checkParameters(summary: ProjectSummary, api: DockviewApi, expect: Expect, show: Show, picture: Picture) {
  const assumptions = summary.objects.find((o) => o.name === 'Reserves assumptions');
  if (!assumptions) return expect('the demo has its assumptions table', false);
  showPanel(api, 'params');
  const panel = () => document.querySelector<HTMLElement>('[data-tool-panel="parameters"]');
  for (let k = 0; k < 50 && !panel(); k++) await wait(100);
  panel()?.querySelector<HTMLButtonElement>('[data-slot="parameters-item"][data-name="Reserves assumptions"]')?.click();
  for (let k = 0; k < 50 && !panel()?.querySelector('[data-slot="parameters-grid"]'); k++) await wait(100);
  const details = await objectDetails(assumptions.id);
  if (details.measures.kind !== 'parameter_table') return expect('the assumptions open as a parameter table', false);
  const t = details.measures.parameters;
  // Turned: the filters then the parameters down the side, the table's rows across.
  const sides = [...(panel()?.querySelectorAll<HTMLElement>('[data-slot="parameters-grid"] tbody tr[data-param]') ?? [])].map((h) => h.dataset.param);
  expect("the grid shows the table's filters then its parameters down the side", JSON.stringify(sides) === JSON.stringify([...t.filters, ...t.columns.map((c) => c.name)]), sides.length);
  expect("a column across a row of the table", panel()?.querySelectorAll('[data-slot="parameters-grid"] thead th[data-row]').length === t.rows.length, t.rows.length);

  // The picked solid's values, in gold, where the engine says they come from.
  for (let k = 0; k < 50 && !panel()?.querySelector('[data-slot="parameters-picked"]'); k++) await wait(100);
  const gold = [...(panel()?.querySelectorAll<HTMLInputElement>('[data-slot="parameters-grid"] input[title$="where the picked solid takes its value"]') ?? [])].map((i) => i.getAttribute('aria-label'));
  const pickedLine = panel()?.querySelector('[data-slot="parameters-picked"]')?.textContent ?? '';
  expect('a picked solid marks the cell each value comes from, as the engine traces it', gold.length > 0 && pickedLine.includes('fits column'), { gold: gold.length, line: pickedLine.replace(/^\S+/, '…') });
  if (picture) {
    // The tab filling the window, as Tim would look along the parameters.
    const press = (label: string) => document.querySelector<HTMLButtonElement>(`button[aria-label="${label}"]`)?.click();
    press('Fill the window');
    await picture('parameters-turned');
    press('Put the tables back under the view');
    await wait(300);
  }

  // An edit: a word refused, then a number saved as the next revision, then undone.
  const ri = t.rows.findIndex((r) => t.columns.some((c) => !c.text && typeof r.values[c.name] === 'number'));
  const column = t.columns.find((c) => !c.text && typeof t.rows[ri]?.values[c.name] === 'number');
  if (ri < 0 || !column) return expect('the table has a number to edit', false);
  const before = t.rows[ri].values[column.name] as number;
  const cell = () => panel()?.querySelector<HTMLInputElement>(`input[aria-label="Column ${ri + 1}, ${column.name}"]`) ?? null;
  const type = (text: string) => {
    const input = cell();
    if (!input) return;
    Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value')?.set?.call(input, text);
    input.dispatchEvent(new Event('input', { bubbles: true }));
  };
  const saveButton = () => [...(panel()?.querySelectorAll<HTMLButtonElement>('button') ?? [])].find((b) => b.textContent?.trim() === 'Save') ?? null;
  type('3O');
  await wait(400);
  expect('a word in a number column is refused before saving', cell()?.getAttribute('aria-invalid') === 'true' && !!saveButton()?.disabled, { why: cell()?.title });
  type(String(before + 1));
  await wait(400);
  expect('a number edited shows as edited, and Save is on', !!cell()?.className.includes('text-edited') && saveButton()?.disabled === false);
  saveButton()?.click();
  let after: number | string | undefined;
  for (let k = 0; k < 50; k++) {
    await wait(100);
    const d = await objectDetails(assumptions.id);
    if (d.measures.kind === 'parameter_table') after = d.measures.parameters.rows[ri].values[column.name];
    if (after === before + 1) break;
  }
  const revisions = (await objectDetails(assumptions.id)).history.length;
  expect('Save writes the edit as the next revision', after === before + 1 && revisions === details.history.length + 1, { column: column.name, before, after, revisions });
  await show(await undo());
  const undone = await objectDetails(assumptions.id);
  expect('an undo puts the table back', undone.measures.kind === 'parameter_table' && undone.measures.parameters.rows[ri].values[column.name] === before && undone.history.length === details.history.length);
}
