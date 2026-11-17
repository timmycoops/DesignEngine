// `--dataflow <project.zdp>`: Properties' views of a parameter table, a dataflow and a results table (decisions 0072,
// 0073), read as Tim would see them. The project (the demo pit saved by testdata/synthetic/workflow_demo.py, its
// dataflow the starter's) is opened, each object clicked in the Layers panel, and what Properties shows read back.
import { EditorView } from '@codemirror/view';
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
  // Across is remembered on the machine: Children first, whatever the last session left.
  for (let k = 0; k < 50 && !panel()?.querySelector('button'); k++) await wait(100);
  [...(panel()?.querySelectorAll<HTMLButtonElement>('button') ?? [])].find((b) => b.textContent === 'Children')?.click();
  const nodeRow = (path: string) => panel()?.querySelector<HTMLElement>(`[data-slot="reserves-node"][data-path="${path}"]`) ?? null;
  for (let k = 0; k < 50 && !nodeRow('Pit > 3'); k++) await wait(100);
  nodeRow('Pit > 3')?.querySelector<HTMLButtonElement>('button')?.click();
  for (let k = 0; k < 50 && !nodeRow('Pit > 3 > B1'); k++) await wait(100);
  nodeRow('Pit > 3 > B1')?.click();
  const heads = () => [...(panel()?.querySelectorAll<HTMLElement>('[data-slot="reserves-breakdown"] th[data-col]') ?? [])].map((h) => h.dataset.col ?? '');
  for (let k = 0; k < 50 && heads()[0] !== 'Pit > 3 > B1'; k++) await wait(100);
  // Folded folders are remembered on the machine too: every one opened, whatever the last session left.
  for (let k = 0; k < 20; k++) {
    const folded = panel()?.querySelector<HTMLButtonElement>('tr[data-folder] button[aria-label^="Open "]');
    if (!folded) break;
    folded.click();
    await wait(50);
  }
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
  await checkColumn(panel, expect, picture);
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
  await checkFormulas(expect, picture, api);
  await checkBeside(expect, api);
  return { ...results, failures };
}

type Expect = (what: string, ok: boolean, detail?: unknown) => void;

/**
 * Open its column (decision 0075, 5b) on block 3_B1 of the Base table, as a notebook (decision 0081): a cell a step, the
 * dataflow's. `densities` shows its formula with the lines that ran on the unit in focus tinted and rd_is set;
 * `rom_dilution` shows the four dilution settings it read for it, each with its table and column, and its dilution as new
 * units. Whole column at rom_dilution: the units sent to U's ROM coal section add up to the table's 10,840.1 m³, more than
 * after rom_loss (← steps back). Notebook goes back; a formula's Edit opens it in the Parameters tab.
 */
async function checkColumn(panel: () => HTMLElement | null, expect: Expect, picture: Picture) {
  const buttonIn = (root: HTMLElement | null | undefined, label: string) => [...(root?.querySelectorAll<HTMLButtonElement>('button') ?? [])].find((b) => b.textContent?.trim() === label) ?? null;
  buttonIn(panel(), 'Open its column')?.click();
  const nb = () => document.querySelector<HTMLElement>('[data-tool-panel="notebook"]');
  const cells = () => [...(nb()?.querySelectorAll<HTMLElement>('[data-slot="notebook-cell"]') ?? [])];
  for (let k = 0; k < 100 && cells().length === 0; k++) await wait(100);
  const steps = cells().map((c) => c.dataset.step);
  const want = ['as input', 'small_volumes', 'initialise', 'fill_size', 'limits', 'densities', 'in_situ', 'join_benches', 'discard', 'working_sections', 'rom_loss', 'rom_dilution', 'rom_ash_cutoff', 'rom', 'rom_qualities', 'final'];
  expect("Open its column opens the Notebook, a cell for each of the dataflow's steps", JSON.stringify(steps) === JSON.stringify(want), steps);
  const cell = (step: string) => nb()?.querySelector<HTMLElement>(`[data-slot="notebook-cell"][data-step="${step}"]`) ?? null;
  const folded = cells().filter((c) => c.dataset.open === 'no').map((c) => `${c.dataset.step}: ${c.querySelector('[data-slot="cell-summary"]')?.textContent}`);
  expect('steps that changed nothing are folded to one line', folded.every((f) => f.endsWith('nothing changed in this column')), folded);

  // densities: its formula, the lines that ran on the unit in focus tinted, rd_is set.
  for (let k = 0; k < 100 && !cell('densities')?.querySelector('[data-ran="yes"]'); k++) await wait(100);
  const ran = cell('densities')?.querySelectorAll('[data-slot="cell-code"] [data-ran="yes"]').length ?? 0;
  const code = cell('densities')?.querySelector('[data-slot="cell-code"]')?.textContent ?? '';
  const inLine = cell('densities')?.querySelector('[data-slot="cell-in"]')?.textContent ?? '';
  expect('densities shows its formula, the lines that ran on the unit in focus and rd_is set', ran > 3 && code.includes('in_situ_rd(') && /rd_is = [\d.]+/.test(code), { ran, in: inLine.replace(/[\d.]+/g, '#') });

  // rom_dilution: the settings it read for the unit in focus, with where each came from; its dilution new.
  const dil = cell('rom_dilution');
  const reads = [...(dil?.querySelectorAll<HTMLElement>('[data-slot="cell-read"]') ?? [])].map((r) => r.textContent ?? '');
  const named = ['roof_dilution', 'floor_dilution', 'edge_dilution', 'other_dilution'].every((n) => reads.some((r) => r.startsWith(n) && /\(Reserves assumptions, column \d+\)/.test(r)));
  const made = dil?.querySelectorAll('[data-slot="cell-out-table"] th[data-unit]').length ?? 0;
  const rule = dil?.querySelector('[data-slot="cell-rule"]')?.textContent ?? '';
  expect("rom_dilution shows its rule, the four dilution settings it read and where each came from, and what it made", named && made > 0 && rule.includes('cuts each waste interval'), { reads, made });

  // The Dataflow view's strip (decision 0084): a step a chip in run order, what each wrote under it; rom_dilution
  // clicked is picked, its cell marked, and the table and column it read lit in Inputs.
  const strip = () => nb()?.querySelector<HTMLElement>('[data-slot="dataflow-strip"]') ?? null;
  const chip = (step: string) => strip()?.querySelector<HTMLButtonElement>(`[data-strip-step="${step}"]`) ?? null;
  const chips = [...(strip()?.querySelectorAll<HTMLElement>('[data-strip-step]') ?? [])].map((c) => c.dataset.stripStep);
  const writesOf = (step: string) => [...(chip(step)?.querySelectorAll('[data-slot="step-writes"] > span') ?? [])].map((w) => (w.textContent ?? '').split(' ')[0]);
  expect(
    'the strip has a step a chip in run order, Imported writing Imported, rom_dilution Dilution and final ROM',
    JSON.stringify(chips) === JSON.stringify(want) && writesOf('as input').includes('Imported') && writesOf('rom_dilution').includes('Dilution') && writesOf('final').includes('ROM'),
    { chips, imported: writesOf('as input'), dilution: writesOf('rom_dilution'), final: writesOf('final') },
  );
  const arrows = [...(strip()?.querySelectorAll('[data-slot="step-units"]') ?? [])].map((a) => Number(a.textContent?.replace('→', '')));
  expect('each arrow carries the units passed on', arrows.length === want.length - 1 && arrows.every((n) => n > 0), arrows);
  chip('rom_dilution')?.click();
  for (let k = 0; k < 50 && cell('rom_dilution')?.dataset.picked !== 'yes'; k++) await wait(50);
  const lit = [...(strip()?.querySelectorAll<HTMLElement>('[data-slot="dataflow-inputs"] [data-lit="yes"]') ?? [])].map((t) => t.textContent ?? '');
  expect(
    'rom_dilution clicked is picked, its cell marked, and the table and column it read lit in Inputs',
    chip('rom_dilution')?.getAttribute('aria-pressed') === 'true' && cell('rom_dilution')?.dataset.picked === 'yes' && lit.length === 1 && /^Reserves assumptions · column \d+/.test(lit[0]),
    lit,
  );
  // Show as graph: the same as nodes, a step clicked there picked; and back.
  buttonIn(nb(), 'Show as graph')?.click();
  const graphNodes = () => [...(nb()?.querySelectorAll<HTMLElement>('[data-slot="dataflow-graph"] .react-flow__node') ?? [])];
  for (let k = 0; k < 50 && graphNodes().length === 0; k++) await wait(100);
  const stepNodes = graphNodes().filter((n) => n.dataset.id?.startsWith('step:'));
  const wroteNodes = graphNodes().filter((n) => n.dataset.id?.startsWith('wrote:'));
  const readEdges = nb()?.querySelectorAll('[data-slot="dataflow-graph"] .react-flow__edge[data-id^="read:"]').length ?? 0;
  expect('Show as graph draws a node a step, a node a write, and the tables wired to what read them', stepNodes.length === want.length && wroteNodes.length >= 4 && readEdges > 1, { steps: stepNodes.length, writes: wroteNodes.length, reads: readEdges });
  if (picture) await picture('dataflow-graph');
  stepNodes.find((n) => n.dataset.id === `step:${want.indexOf('working_sections')}`)?.click();
  for (let k = 0; k < 50 && cell('working_sections')?.dataset.picked !== 'yes'; k++) await wait(50);
  expect('a step clicked in the graph is picked', cell('working_sections')?.dataset.picked === 'yes');
  buttonIn(nb(), 'Show as strip')?.click();
  for (let k = 0; k < 50 && !strip(); k++) await wait(50);
  chip('rom_dilution')?.click();
  await wait(100);
  if (picture) {
    // A picture doesn't keep the scroll: every cell folded but densities and rom_dilution, so they're what it shows.
    buttonIn(nb(), 'Fold all')?.click();
    await wait(100);
    for (const step of ['densities', 'rom_dilution']) cell(step)?.querySelector<HTMLButtonElement>('button[aria-expanded]')?.click();
    for (let k = 0; k < 100 && !cell('densities')?.querySelector('[data-ran="yes"]'); k++) await wait(100);
    document.querySelector<HTMLButtonElement>('button[aria-label="Fill the window"]')?.click();
    await picture('notebook');
    document.querySelector<HTMLButtonElement>('button[aria-label="Put the tables back under the view"]')?.click();
    buttonIn(nb(), 'Open all')?.click();
    await wait(300);
  }

  // Whole column at rom_dilution: the grid of every unit, U's ROM coal the table's.
  buttonIn(dil, 'Whole column')?.click();
  const grid = () => nb()?.querySelector<HTMLElement>('[data-slot="column-grid"]') ?? null;
  for (let k = 0; k < 100 && !grid(); k++) await wait(100);
  for (let k = 0; k < 50 && nb()?.querySelector('[data-step="rom_dilution"]')?.getAttribute('aria-selected') !== 'true'; k++) await wait(50);
  // U's ROM coal, by the units' heads: those a step sent to its ROM coal section (dilution), and its coal still in its
  // working section, sent nowhere else; their volumes after the step (the cell's own line, not what it was).
  const romCoalU = () => {
    const heads = [...(grid()?.querySelectorAll<HTMLElement>('thead th[data-unit]') ?? [])];
    const vols = [...(grid()?.querySelectorAll<HTMLElement>('tr[data-field="volume"] td') ?? [])].slice(1);
    const toU = (head: string) => /ROM \d* ?coal U$/.test(head) || (/section \d+ coal U$/.test(head) && !head.includes('ROM'));
    return heads.reduce((sum, h, i) => (toU(h.textContent ?? '') ? sum + (Number((vols[i]?.childNodes[0]?.textContent ?? '').replaceAll(',', '')) || 0) : sum), 0);
  };
  await wait(100);
  const afterDilution = romCoalU();
  nb()?.querySelector<HTMLElement>('[data-slot="column-steps"]')?.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowLeft', bubbles: true }));
  for (let k = 0; k < 50 && nb()?.querySelector('[data-step="rom_loss"]')?.getAttribute('aria-selected') !== 'true'; k++) await wait(50);
  await wait(100);
  const afterLoss = romCoalU();
  expect(
    "Whole column at rom_dilution: U's ROM coal is the table's 10,840.1 m³, up from after rom_loss",
    // Each unit's volume is read as shown, to 0.1 m³: five of them may be out by 0.25 m³ added up.
    Math.abs(afterDilution - 10_840.1) < 0.3 && afterDilution > afterLoss,
    { afterLoss: Math.round(afterLoss * 10) / 10, afterDilution: Math.round(afterDilution * 10) / 10 },
  );
  buttonIn(nb(), 'Notebook')?.click();
  for (let k = 0; k < 50 && cells().length === 0; k++) await wait(100);
  expect('Notebook goes back to the cells', cells().length === want.length);

  // A formula's Edit opens it in the Parameters tab.
  buttonIn(cell('densities'), 'Edit in the formula panel')?.click();
  const params = () => document.querySelector<HTMLElement>('[data-tool-panel="parameters"]');
  for (let k = 0; k < 100 && !params()?.querySelector('[data-formula="yes"][data-step="densities"].bg-selection'); k++) await wait(100);
  expect("a formula cell's Edit opens it in the Parameters tab", !!params()?.querySelector('[data-formula="yes"][data-step="densities"].bg-selection') && !!params()?.querySelector('[data-slot="script-editor"]'));
}

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

/**
 * The formula panel (decision 0077, 7d) on the demo's dataflow: its five starter formulas listed; `densities` opened,
 * checked with its worked examples holding; run on the picked block's unit (a solid of 3_B1, picked by Select its
 * solids), lines that ran tinted and rd_is set; a misspelt field shown on its line, Save off; Discard puts it back.
 */
async function checkFormulas(expect: Expect, picture: Picture, api: DockviewApi) {
  const panel = () => document.querySelector<HTMLElement>('[data-tool-panel="parameters"]');
  panel()?.querySelector<HTMLButtonElement>('[data-slot="parameters-item"][data-name="Reserves"]')?.click();
  for (let k = 0; k < 50 && !panel()?.querySelector('[data-formula="yes"]'); k++) await wait(100);
  const formulas = [...(panel()?.querySelectorAll<HTMLElement>('[data-formula="yes"]') ?? [])].map((e) => e.dataset.step);
  expect('the dataflow lists its starter formulas', JSON.stringify(formulas) === JSON.stringify(['fill_size', 'limits', 'densities', 'discard', 'rom_qualities']), formulas);
  panel()?.querySelector<HTMLButtonElement>('[data-formula="yes"][data-step="densities"] button[title="Open this formula"]')?.click();
  for (let k = 0; k < 50 && !panel()?.querySelector('[data-slot="formula-ok"]'); k++) await wait(100);
  const ok = panel()?.querySelector('[data-slot="formula-ok"]')?.textContent ?? '';
  expect('a formula opens checked, its worked examples holding', /\d+ worked examples hold/.test(ok), ok);
  const button = (label: string) => [...(panel()?.querySelectorAll<HTMLButtonElement>('button') ?? [])].find((b) => b.textContent?.trim() === label) ?? null;
  button('Check')?.click();
  for (let k = 0; k < 100 && !panel()?.querySelector('[data-slot="formula-trace"]'); k++) await wait(100);
  const sets = panel()?.querySelector('[data-slot="formula-sets"]')?.textContent ?? '';
  const ran = panel()?.querySelectorAll('[data-slot="script-editor"] .cm-ran').length ?? 0;
  // A line in a loop runs more than once; it's tinted once.
  const lines = new Set([...(panel()?.querySelectorAll<HTMLElement>('[data-slot="formula-trace"] [data-line]') ?? [])].map((e) => e.dataset.line)).size;
  expect('run on the picked block, the lines that ran are shown and tinted, and what it set', sets.includes('rd_is = ') && ran > 3 && ran === lines, { ran, lines, sets: sets.replace(/[0-9.]+/g, '#') });
  if (picture) {
    const press = (label: string) => document.querySelector<HTMLButtonElement>(`button[aria-label="${label}"]`)?.click();
    press('Fill the window');
    await picture('formula-panel');
    press('Put the tables back under the view');
    await wait(300);
  }
  // A misspelt field: shown on its line, underlined, and Save off.
  const dom = panel()?.querySelector<HTMLElement>('[data-slot="script-editor"] .cm-editor');
  const view = dom ? EditorView.findFromDOM(dom) : null;
  const text = view?.state.doc.toString() ?? '';
  const at = text.indexOf('unit.rd_is = rd_is');
  view?.dispatch({ changes: { from: at + 'unit.'.length, to: at + 'unit.rd_is'.length, insert: 'rd_si' } });
  for (let k = 0; k < 50 && !panel()?.querySelector('[data-slot="formula-problem"]'); k++) await wait(100);
  await wait(400);
  const problem = panel()?.querySelector('[data-slot="formula-problem"]')?.textContent ?? '';
  const underlined = panel()?.querySelectorAll('.cm-lintRange-error').length ?? 0;
  const save = button('Save');
  expect('a misspelt field is shown on its line and underlined, and Save is off', at >= 0 && /^line \d+: `unit\.rd_si` isn't a field/.test(problem) && underlined > 0 && !!save?.disabled, { problem, underlined, save: save?.disabled });
  button('Discard')?.click();
  await wait(400);
  const back = view?.state.doc.toString() ?? '';
  expect('Discard puts the formula back', back === text && !panel()?.querySelector('[data-slot="formula-problem"]'));

  // Run in notebook (decision 0081, point 7): rd_is doubled and not saved, run on the Notebook's column (3_B1, opened
  // above). Its densities cell runs the edit, marked edited, and the worked examples it breaks are flagged, not refused;
  // the Parameters tab, put behind, still holds the edit; back to the saved formula, then the edit discarded.
  const at2 = back.indexOf('rd_is = in_situ_rd(');
  view?.dispatch({ changes: { from: at2 + 'rd_is = '.length, to: at2 + 'rd_is = '.length, insert: '2 * ' } });
  await wait(600);
  button('Run in notebook')?.click();
  const nb = () => document.querySelector<HTMLElement>('[data-tool-panel="notebook"]');
  const dens = () => nb()?.querySelector<HTMLElement>('[data-slot="notebook-cell"][data-step="densities"]') ?? null;
  for (let k = 0; k < 100 && !(nb()?.querySelector('[data-slot="notebook-edited"]') && dens()?.querySelector('[data-ran="yes"]')); k++) await wait(100);
  const code = dens()?.querySelector('[data-slot="cell-code"]')?.textContent ?? '';
  const failing = nb()?.querySelector('[data-slot="notebook-failing"]')?.textContent ?? '';
  const badge = [...(dens()?.querySelectorAll('span') ?? [])].some((e) => e.textContent === 'edited');
  expect(
    'Run in notebook runs the edited formula on the column, marked edited, its failing worked examples flagged, not refused',
    code.includes('2 * in_situ_rd(') && /rd_is = [\d.]+/.test(code) && badge && failing.includes('the formula step densities, test'),
    { badge, failing: failing.slice(0, 120) },
  );
  if (picture) {
    document.querySelector<HTMLButtonElement>('button[aria-label="Fill the window"]')?.click();
    await picture('run-in-notebook');
    document.querySelector<HTMLButtonElement>('button[aria-label="Put the tables back under the view"]')?.click();
    await wait(300);
  }
  showPanel(api, 'params');
  await wait(300);
  const kept = (view && panel()?.contains(view.dom) ? view : null)?.state.doc.toString() ?? '';
  expect('the Parameters tab, put behind the Notebook, still holds the edit', kept.includes('rd_is = 2 * in_situ_rd('));
  showPanel(api, 'notebook');
  [...(nb()?.querySelectorAll<HTMLButtonElement>('button') ?? [])].find((b) => b.textContent === 'Back to the saved formula')?.click();
  for (let k = 0; k < 100 && nb()?.querySelector('[data-slot="notebook-edited"]'); k++) await wait(100);
  for (let k = 0; k < 100 && !dens()?.querySelector('[data-ran="yes"]'); k++) await wait(100);
  const savedCode = dens()?.querySelector('[data-slot="cell-code"]')?.textContent ?? '';
  expect('Back to the saved formula runs the saved one again', !nb()?.querySelector('[data-slot="notebook-edited"]') && savedCode.includes('rd_is = in_situ_rd(') && !savedCode.includes('2 * in_situ_rd('));
  showPanel(api, 'params');
  await wait(200);
  button('Discard')?.click();
  await wait(400);
}

/**
 * Beside the Notebook: the Parameters and the Reserves table each put in their own half of its room with a press, and
 * back as tabs with another; with the Reserves table beside, picking another block there runs its column in the Notebook
 * with no press of Open its column.
 */
async function checkBeside(expect: Expect, api: DockviewApi) {
  const nb = () => document.querySelector<HTMLElement>('[data-tool-panel="notebook"]');
  const rt = () => document.querySelector<HTMLElement>('[data-tool-panel="reserves-table"]');
  const press = (label: string) => [...(nb()?.querySelectorAll<HTMLButtonElement>('[data-slot="notebook-beside"] button') ?? [])].find((b) => b.textContent === label)?.click();
  const group = (id: string) => api.getPanel(id)?.group.id;
  showPanel(api, 'notebook');
  for (let k = 0; k < 50 && !nb(); k++) await wait(100);
  press('Parameters beside');
  press('Reserves table beside');
  await wait(400);
  const apart = new Set([group('notebook'), group('params'), group('restable')]).size === 3;
  expect('Parameters beside and Reserves table beside put each in its own half beside the Notebook, all three in sight', apart && !!nb() && !!rt() && !!document.querySelector('[data-tool-panel="parameters"]'));

  const shown = () => nb()?.querySelector('[aria-label="Column shown"]')?.textContent ?? '';
  const was = shown();
  const other = [...(rt()?.querySelectorAll<HTMLElement>('[data-slot="reserves-node"]') ?? [])].find((r) => {
    const path = (r.dataset.path ?? '').split(' > ');
    return path.length === 3 && path[0] === 'Pit' && !was.endsWith(`› ${path[2]}`) && !was.includes(`› ${path[2]} ›`);
  });
  other?.click();
  const want = (other?.dataset.path ?? '').split(' > ').join(' › ');
  for (let k = 0; k < 100 && !(shown() === want && nb()?.querySelector('[data-slot="notebook-cell"]')); k++) await wait(100);
  expect('the Notebook follows a block picked in the Reserves table beside it, with no press of Open its column', !!other && shown() === want, { was, now: shown() });

  press('Parameters as a tab');
  press('Reserves table as a tab');
  await wait(400);
  expect('pressed again, they go back as tabs behind the Notebook', group('params') === group('notebook') && group('restable') === group('notebook'));
}
