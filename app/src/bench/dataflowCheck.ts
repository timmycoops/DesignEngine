// `--dataflow <project.zdp>`: Properties' views of a parameter table, a dataflow and a results table (decisions 0072,
// 0073), read as Tim would see them. The project (the demo pit saved by testdata/synthetic/workflow_demo.py, its
// dataflow the starter's) is opened, each object picked in its own tab's list (the layer tree has no row for them,
// Filtering plan L4), and what Properties shows read back.
import { completionStatus, currentCompletions, startCompletion } from '@codemirror/autocomplete';
import { EditorView } from '@codemirror/view';
import type { DockviewApi } from 'dockview-react';
import { calcTrace, concealed, objectDetails, objectsAttributes, openProject, projectSummary, type ProjectSummary, type ReservesNode, reservesBreakdown, reservesChanged, reservesColumn, reservesNodeOf, reservesRerunParams, reservesSolids, reservesStepGrid, reservesTree, recordsPreview, runCommand, solidsTable, tableGrid, undo, writePicture } from '../bridge';
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

  /**
   * Picks an object in its own tab's list (the Parameters tab's for a table or dataflow, the Reserves table tab's for a
   * results table), then Show in Properties on its menu, and gives back the page's text once Properties has it.
   */
  const shown = async (name: string, waitFor: string) => {
    const object = summary.objects.find((o) => o.name === name && ['parameter_table', 'dataflow', 'table'].includes(o.kind));
    if (!object) return null;
    if (object.kind === 'table') {
      showPanel(api, 'restable');
      let pick: HTMLSelectElement | null = null;
      for (let k = 0; k < 50 && !pick; k++) {
        pick = document.querySelector<HTMLSelectElement>('select[aria-label="Table"]');
        if (!pick) await wait(100);
      }
      if (!pick) return null;
      // React reads a select's value through its own setter.
      Object.getOwnPropertyDescriptor(HTMLSelectElement.prototype, 'value')?.set?.call(pick, object.id);
      pick.dispatchEvent(new Event('change', { bubbles: true }));
    } else {
      showPanel(api, 'params');
      let item: HTMLElement | null = null;
      for (let k = 0; k < 50 && !item; k++) {
        item = document.querySelector<HTMLElement>(`[data-slot="parameters-item"][data-name="${CSS.escape(name)}"]`);
        if (!item) await wait(100);
      }
      if (!item) return null;
      item.click();
    }
    // The menu beside it: in the Parameters list, on the row picked once it's lit; for a results table, beside its select.
    const triggerOf = () =>
      object.kind === 'table'
        ? document.querySelector<HTMLElement>('[data-tool-panel="reserves-table"] [data-slot="object-menu"]')
        : (document.querySelector<HTMLElement>(`[data-slot="parameters-item"][data-name="${CSS.escape(name)}"]`)?.parentElement?.querySelector<HTMLElement>('[data-slot="object-menu"]') ?? null);
    let trigger: HTMLElement | null = null;
    for (let k = 0; k < 50 && !trigger; k++) {
      trigger = triggerOf();
      if (!trigger) await wait(100);
    }
    // The last menu gone first, so the one found is this one's; then it opens on the press, as a person's would.
    const menus = () => document.querySelectorAll('[data-slot="object-menu-items"]').length;
    for (let k = 0; k < 30 && menus() > 0; k++) await wait(100);
    trigger?.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, cancelable: true, button: 0, pointerType: 'mouse' }));
    let showIt: HTMLElement | undefined;
    for (let k = 0; k < 30 && !showIt; k++) {
      showIt = [...document.querySelectorAll<HTMLElement>('[data-slot="object-menu-items"] [role="menuitem"]')].filter((m) => m.textContent?.trim() === 'Show in Properties').at(-1);
      if (!showIt) await wait(100);
    }
    if (!showIt) return null;
    showIt.click();
    for (let k = 0; k < 30 && menus() > 0; k++) await wait(100);
    for (let k = 0; k < 50 && !(document.body.textContent ?? '').includes(waitFor); k++) await wait(100);
    return document.body.textContent ?? '';
  };

  const table = await shown('Reserves assumptions', 'Read top down');
  expect('a parameter table shows its filters, values and rows', !!table && table.includes('Filters on') && table.includes('when material') && table.includes('min_parting (m)') && table.includes('Save as CSV'));
  const flow = await shown('Reserves', 'Adds up');
  expect(
    'a dataflow shows its steps, its parameter table by name and its fields',
    // The starter's formulas after initialise, qualities weighed by air-dried tonnes Spry's way (decision 0077, 7c).
    !!flow && flow.includes('small_volumes, initialise, fill_size, limits, densities, in_situ, join_benches, discard, working_sections') && flow.includes('rom, rom_qualities, final') && flow.includes('Reserves assumptions') && flow.includes('by tonnes (tonnes_ad_per_m3)') && flow.includes('attribute mis'),
  );
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
  const pick = panel()?.querySelector<HTMLSelectElement>('select[aria-label="Table"]');
  if (pick && baseTable) {
    // React reads a select's value through its own setter.
    Object.getOwnPropertyDescriptor(HTMLSelectElement.prototype, 'value')?.set?.call(pick, baseTable.id);
    pick.dispatchEvent(new Event('change', { bubbles: true }));
  }
  // Across and the view are remembered on the machine: the Breakdown by Children first, whatever the last session left.
  for (let k = 0; k < 50 && !panel()?.querySelector('button'); k++) await wait(100);
  [...(panel()?.querySelectorAll<HTMLButtonElement>('button') ?? [])].find((b) => b.textContent === 'Breakdown')?.click();
  await wait(100);
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
  // The leaves filtered (decision 0088, workstream 4): seam U alone, B1's total is its U, 10,840.1 m³, its only column.
  const filterBox = () => panel()?.querySelector<HTMLInputElement>('input[aria-label="Filter the leaves"]') ?? null;
  const typeFilter = async (text: string) => {
    const box = filterBox();
    if (!box) return;
    Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value')?.set?.call(box, text);
    box.dispatchEvent(new Event('input', { bubbles: true }));
    await wait(600);
  };
  await typeFilter('seam: U');
  for (let k = 0; k < 50 && heads().length !== 2; k++) await wait(100);
  const kept = panel()?.querySelector('[data-slot="reserves-filter-kept"]')?.textContent ?? '';
  const filteredHeads = heads();
  const filteredCoal = cells('ROM\\Coal\\volume');
  await typeFilter('');
  for (let k = 0; k < 50 && heads().length < 4; k++) await wait(100);
  expect(
    "seam: U in the leaves' filter: B1 adds up its U alone, 10,840.1 m³, its one column; cleared, its three seams again",
    !!filterBox() && JSON.stringify(filteredHeads) === JSON.stringify(['Pit > 3 > B1', 'Pit > 3 > B1 > U']) && filteredCoal[0] === '10,840.1' && kept.includes(' of ') && heads().length === 4,
    { found: !!filterBox(), heads: filteredHeads, coal: filteredCoal, kept, back: heads().length },
  );
  // Suggestions as it's typed: `se` offers seam, Tab puts it in, then its words with their counts; a field by its
  // folders is offered too; Esc closes the list, a second clears the box.
  const offered = () => [...document.querySelectorAll<HTMLElement>('[data-slot="filter-suggestions"] [role="option"]')].map((o) => o.textContent ?? '');
  const press = async (key: string) => {
    filterBox()?.dispatchEvent(new KeyboardEvent('keydown', { key, bubbles: true, cancelable: true }));
    await wait(300);
  };
  await typeFilter('se');
  const names = offered();
  await press('Tab');
  const afterTab = filterBox()?.value ?? '';
  const seams = offered();
  await typeFilter('ROM\\Coal\\vol');
  const fieldsOffered = offered();
  await press('Escape');
  const closed = offered().length === 0 && filterBox()?.value === 'ROM\\Coal\\vol';
  await press('Escape');
  const emptied = filterBox()?.value ?? null;
  await typeFilter('');
  expect(
    "the leaves' filter suggests as it's typed: se offers seam, Tab puts in 'seam: ', then U, M and L with their counts; ROM\\Coal\\vol offers the field; Esc closes the list, again clears the box",
    names[0]?.startsWith('seam') === true && afterTab === 'seam: ' && ['U', 'M', 'L'].every((s) => seams.some((o) => o.startsWith(s))) && fieldsOffered.some((o) => o.startsWith('ROM\\Coal\\volume')) && closed && emptied === '',
    { names: names.slice(0, 3), afterTab, seams, fields: fieldsOffered.slice(0, 3), closed, emptied },
  );
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
  // Isolate its solids: the block's 21 alone on show; again, everything as it was.
  const onShow = (s: ProjectSummary) => {
    const on = new Set(s.layers.filter((l) => l.visible).map((l) => l.id));
    return s.objects.filter((o) => ['string', 'surface', 'solid', 'grid'].includes(o.kind) && on.has(o.layer) && !concealed(o)).map((o) => o.id).sort();
  };
  const state = (s: ProjectSummary) => JSON.stringify({ layers: s.layers.map((l) => [l.id, l.visible]), hidden: s.objects.filter((o) => o.hidden).map((o) => o.id).sort() });
  const before = await projectSummary();
  const button = (text: string) => [...(panel()?.querySelectorAll<HTMLButtonElement>('button') ?? [])].find((b) => b.textContent?.includes(text));
  button('Isolate its solids')?.click();
  for (let k = 0; k < 50 && !(panel()?.textContent ?? '').includes('alone on show'); k++) await wait(100);
  await wait(200);
  const alone = onShow(await projectSummary());
  const block = baseTable ? (await reservesSolids(baseTable.id, ['Pit', '3', 'B1'])).sort() : [];
  const lit = button('Show what was shown') !== undefined;
  button('Show what was shown')?.click();
  for (let k = 0; k < 50 && !(panel()?.textContent ?? '').includes('Shown as it was'); k++) await wait(100);
  await wait(200);
  const after = await projectSummary();
  expect(
    "Isolate its solids shows the block's 21 solids alone, and Show what was shown puts the layers and objects back as they were",
    alone.length === 21 && JSON.stringify(alone) === JSON.stringify(block) && lit && state(after) === state(before),
    { alone: alone.length, block: block.length, lit, putBack: state(after) === state(before) },
  );
  // The table's solids' layer named, and Show puts it alone on screen; Back puts the layers back (decision 0090).
  if (baseTable) {
    const all = await reservesSolids(baseTable.id, []);
    const summaryNow = await projectSummary();
    const layerOf = new Map(summaryNow.objects.map((o) => [o.id, o.layer]));
    const theirs = [...new Set(all.map((id) => layerOf.get(id)))].sort();
    const names = summaryNow.layers.filter((l) => theirs.includes(l.id)).map((l) => l.name);
    const row = panel()?.querySelector<HTMLElement>('[data-slot="reserves-layers"]');
    const rowButton = () => row?.querySelector<HTMLButtonElement>('button');
    const named = names.length > 0 && names.every((n) => row?.textContent?.includes(n));
    const before2 = await projectSummary();
    rowButton()?.click();
    for (let k = 0; k < 50 && rowButton()?.textContent !== 'Back'; k++) await wait(100);
    await wait(200);
    const shownLayers = (await projectSummary()).layers.filter((l) => l.visible).map((l) => l.id).sort();
    rowButton()?.click();
    for (let k = 0; k < 50 && rowButton()?.textContent !== 'Show'; k++) await wait(100);
    await wait(200);
    expect(
      "the table names its solids' layer, Show puts it alone on screen, and Back puts the layers back",
      named && JSON.stringify(shownLayers) === JSON.stringify(theirs) && state(await projectSummary()) === state(before2),
      { layers: names.length, shown: shownLayers.length, theirs: theirs.length },
    );
    // Show, then Isolate its solids without Back first: only the block's solids on show, then everything as before Show.
    const before3 = await projectSummary();
    rowButton()?.click();
    for (let k = 0; k < 50 && rowButton()?.textContent !== 'Back'; k++) await wait(100);
    await wait(200);
    button('Isolate its solids')?.click();
    for (let k = 0; k < 50 && !button('Show what was shown'); k++) await wait(100);
    await wait(300);
    const afterShowIso = onShow(await projectSummary());
    const showFreed = rowButton()?.textContent === 'Show';
    button('Show what was shown')?.click();
    for (let k = 0; k < 50 && !button('Isolate its solids'); k++) await wait(100);
    await wait(300);
    expect(
      "Show then Isolate its solids: only the block's 21 solids on show, no layer Show put back left on; then everything as before Show",
      JSON.stringify(afterShowIso) === JSON.stringify(block) && showFreed && state(await projectSummary()) === state(before3),
      { onShow: afterShowIso.length, block: block.length, showFreed },
    );

    // Select and Isolate take the filter: B1's U solids only.
    await typeFilter('seam: U');
    for (let k = 0; k < 50 && !heads().includes('Pit > 3 > B1 > U'); k++) await wait(100);
    const uOnly = (await reservesSolids(baseTable.id, ['Pit', '3', 'B1'], { seam: 'U' })).sort();
    button('Select its solids')?.click();
    for (let k = 0; k < 50 && !(panel()?.textContent ?? '').includes(`${uOnly.length} solid`); k++) await wait(100);
    const pickedU = /([\d,]+) solids? picked/.exec(panel()?.textContent ?? '')?.[1];
    const uAttrs = await objectsAttributes(uOnly);
    expect(
      "with the filter on, Select its solids picks only the solids of the leaves it keeps (B1's U)",
      uOnly.length > 0 && uOnly.length < 21 && pickedU === String(uOnly.length) && uAttrs.every((a) => String(a.seam) === 'U'),
      { picked: pickedU, kept: uOnly.length },
    );

    // Filter the view too: the table's solids on the leaves it leaves out aren't drawn; Coal solids only takes the waste too.
    const tick = (label: string) => panel()?.querySelector<HTMLButtonElement>(`button[aria-label="${label}"]`);
    const keptAll = new Set(await reservesSolids(baseTable.id, [], { seam: 'U' }));
    tick('Filter the view too')?.click();
    for (let k = 0; k < 50 && (await projectSummary()).display.reserves?.table !== baseTable.id; k++) await wait(100);
    const filteredNow = (s: ProjectSummary) => new Set(s.objects.filter((o) => o.filtered).map((o) => o.id));
    const out1 = filteredNow(await projectSummary());
    const viewRight = all.every((id) => out1.has(id) === !keptAll.has(id));
    tick('Coal solids only')?.click();
    for (let k = 0; k < 50 && !(await projectSummary()).display.reserves?.coal_only; k++) await wait(100);
    const out2 = filteredNow(await projectSummary());
    const keptAttrs = await objectsAttributes([...keptAll]);
    const coal = new Set([...keptAll].filter((_, i) => String(keptAttrs[i].material).toLowerCase() !== 'waste'));
    const coalRight = all.every((id) => out2.has(id) === !coal.has(id));
    tick('Filter the view too')?.click();
    for (let k = 0; k < 50 && (await projectSummary()).display.reserves; k++) await wait(100);
    const cleared = filteredNow(await projectSummary());
    await typeFilter('');
    for (let k = 0; k < 50 && !heads().includes('Pit > 3 > B1 > M'); k++) await wait(100);
    expect(
      "Filter the view too leaves out the table's solids on the leaves the filter doesn't keep; Coal solids only the waste on those it does; unticked, they're all back",
      viewRight && coalRight && coal.size > 0 && coal.size < keptAll.size && all.every((id) => !cleared.has(id)),
      { table: all.length, kept: keptAll.size, coal: coal.size, viewRight, coalRight },
    );
  }
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

    // The Tables tab (Tables and scripts plan, decision 1): the table as a grid by pit and strip, each strip's subtotal
    // the Breakdown's strip node, field by field, both the engine's add-up of the same leaves.
    t = performance.now();
    const byStrip = await tableGrid({ of: 'table', table: fiftyTable.id }, { group: ['pit', 'strip'], open: [['Pit']] });
    const gridMs = performance.now() - t;
    const strips = byStrip.rows.filter((r) => r.kind === 'group' && r.depth === 2);
    let checked = 0;
    let worst = 0;
    for (const s of strips) {
      const b = await reservesBreakdown(fiftyTable.id, s.path, []);
      byStrip.columns.forEach((c, k) => {
        const f = b.fields.find((x) => x.column === c.name && !x.words);
        const mine = s.cells[k];
        if (!f || typeof mine === 'string') return;
        const theirs = f.values[0];
        checked += 1;
        if (mine === null || theirs === null) worst = mine === theirs ? worst : Infinity;
        else worst = Math.max(worst, Math.abs(mine - theirs) / Math.max(1, Math.abs(theirs)));
      });
    }
    expect("the grid's strips are the Breakdown's strip nodes, every field", strips.length > 1 && checked > strips.length * 20 && worst <= 1e-12, { strips: strips.length, checked, worst, gridMs: Math.round(gridMs) });
  }
  if (baseTable) {
    // Objects as a table: the Base case's blocks by material, coal's volume their solids' own volumes added.
    const ids = await reservesSolids(baseTable.id, []);
    const own = await solidsTable(ids);
    const coal = own.reduce((s, r) => (r.attributes.material === 'coal' ? s + (r.volume ?? 0) : s), 0);
    const coalCount = own.filter((r) => r.attributes.material === 'coal').length;
    const byMaterial = await tableGrid({ of: 'objects', objects: ids, density: 'rd' }, { group: ['material'], columns: ['volume', 'tonnes'] });
    const coalRow = byMaterial.rows.find((r) => r.kind === 'group' && r.label === 'coal');
    const gridCoal = typeof coalRow?.cells[0] === 'number' ? coalRow.cells[0] : NaN;
    expect("Objects by material: coal is its blocks' own volumes added, and counts them", !!coalRow && coalRow.leaves === coalCount && Math.abs(gridCoal - coal) <= 1e-9 * coal && typeof byMaterial.rows[0].cells[1] === 'number', {
      blocks: ids.length,
      coal: coalCount,
      grid: gridCoal,
      solids: coal,
    });
    // In the tab: Grid draws the leaves grouped; the Table list goes to Objects and back.
    showPanel(api, 'restable');
    for (let k = 0; k < 50 && !panel(); k++) await wait(100);
    const press = (words: string) => [...(panel()?.querySelectorAll<HTMLButtonElement>('button') ?? [])].find((b) => b.textContent === words)?.click();
    press('Grid');
    const grouped = () => panel()?.querySelectorAll('[data-slot="reserves-grid"] tr[data-row="group"]').length ?? 0;
    for (let k = 0; k < 50 && grouped() === 0; k++) await wait(100);
    const groupsShown = grouped();
    press('Breakdown');
    const pickTable = (id: string) => {
      const list = document.querySelector<HTMLSelectElement>('select[aria-label="Table"]');
      if (!list) return;
      Object.getOwnPropertyDescriptor(HTMLSelectElement.prototype, 'value')?.set?.call(list, id);
      list.dispatchEvent(new Event('change', { bubbles: true }));
    };
    pickTable('objects');
    for (let k = 0; k < 50 && !document.querySelector('[data-tool-panel="blocks"]'); k++) await wait(100);
    const objectsShown = !!document.querySelector('[data-tool-panel="blocks"]');
    pickTable(baseTable.id);
    for (let k = 0; k < 50 && !panel(); k++) await wait(100);
    expect('the Tables tab draws the reserves table as a grid, grouped, and lists Objects too', groupsShown > 0 && objectsShown && !!panel(), { groups: groupsShown, objects: objectsShown });

    // Every block at a step (Tables and scripts plan, decision 10). As input, every block's units add up to the blocks'
    // own volumes; at working_sections, a block's units are its own column's at that step, Open its column's.
    let started = performance.now();
    const asInput = await reservesStepGrid(baseTable.id, 0, { columns: ['volume'] });
    const inputMs = performance.now() - started;
    const allVolume = own.reduce((s, r) => s + (r.volume ?? 0), 0);
    const inputVolume = typeof asInput.grid.rows[0].cells[0] === 'number' ? asInput.grid.rows[0].cells[0] : NaN;
    expect("Every block as input: the Base blocks' own volumes, a unit a block", asInput.stopped.length === 0 && asInput.grid.leaves === ids.length && Math.abs(inputVolume - allVolume) <= 1e-9 * allVolume, {
      units: asInput.grid.leaves,
      blocks: ids.length,
      grid: inputVolume,
      solids: allVolume,
      columns: asInput.columns,
      ms: Math.round(inputMs),
    });
    const blocksTo = [['Pit', '3', 'B1'], ['Pit', '4', 'B2']];
    const one = await reservesColumn(baseTable.id, blocksTo[0]);
    const sections = one.steps.findIndex((x) => x.step === 'working_sections');
    started = performance.now();
    const atSections = await reservesStepGrid(baseTable.id, sections, { group: ['pit', 'strip', 'block'], open: [['Pit'], ['Pit', '3'], ['Pit', '4'], ...blocksTo], columns: ['volume'] });
    const sectionsMs = performance.now() - started;
    const matches = await Promise.all(
      blocksTo.map(async (path) => {
        const theirs = (await reservesColumn(baseTable.id, path)).steps[sections].units;
        const mine = atSections.grid.rows.filter((r) => r.kind === 'leaf' && r.path.join('>') === path.join('>'));
        const sum = (xs: number[]) => xs.reduce((a, b) => a + b, 0);
        const theirVolume = sum(theirs.map((u) => u.values.volume ?? 0));
        const myVolume = sum(mine.map((r) => (typeof r.cells[0] === 'number' ? r.cells[0] : 0)));
        return { block: path.join(' '), units: [mine.length, theirs.length], volume: [myVolume, theirVolume], ok: mine.length === theirs.length && Math.abs(myVolume - theirVolume) <= 1e-9 * Math.max(1, theirVolume) };
      }),
    );
    expect('Every block at working_sections: a block\'s units are its own column\'s there', sections > 0 && atSections.step === 'working_sections' && matches.every((m) => m.ok), { matches, ms: Math.round(sectionsMs), columns: atSections.columns });

    // Entered values and blocks' attributes (Tables and scripts plan, decisions 2 to 4). Two blocks' attributes set in one
    // run: the table says two blocks changed since its run; one undo and it says nothing.
    const two = ids.slice(0, 2);
    await runCommand('object.set_attributes', { cells: two.map((object) => ({ object, attribute: 'checked', value: 'yes' })) });
    const changedTwo = await reservesChanged(baseTable.id);
    await undo();
    const changedNone = await reservesChanged(baseTable.id);
    // The demo's dataflow was renamed as it opened (0077, 7c), so it has changed since the table's run either way.
    expect("two blocks' attributes set: the table says two blocks changed since its run, and after one undo none", changedTwo.blocks === 2 && changedNone.blocks === 0 && changedNone.others.join() === changedTwo.others.join(), { set: changedTwo, undone: changedNone });
    // A comment entered on two leaves stays on them when the whole table runs again; two undos put it all back.
    const leaves = await tableGrid({ of: 'table', table: baseTable.id }, { columns: ['Insitu\\Coal\\volume'], limit: 3 });
    const picked = leaves.rows.filter((r) => r.kind === 'leaf').slice(0, 2);
    await runCommand('table.enter', { table: baseTable.id, column: 'comment', words: true, cells: picked.map((r, i) => ({ row: r.row, value: `leaf ${i + 1}` })) });
    const wordsOf = (r: { words: string[] | null }) => (r.words ?? []).join(' ');
    const want = new Map(picked.map((r, i) => [wordsOf(r), `leaf ${i + 1}`]));
    await runCommand('reserves.run', await reservesRerunParams(baseTable.id, []));
    const after = await tableGrid({ of: 'table', table: baseTable.id }, { columns: ['comment'], limit: 100_000 });
    const found = after.rows.filter((r) => r.kind === 'leaf' && r.cells[0] !== null).map((r) => [wordsOf(r), r.cells[0]] as const);
    const kept = found.length === 2 && found.every(([w, c]) => want.get(w) === c);
    const entered = after.columns[0]?.kind;
    await undo();
    await undo();
    const gone = (await tableGrid({ of: 'table', table: baseTable.id }, { columns: [], limit: 1 })).every.every((c) => c.name !== 'comment');
    expect('a comment entered on two leaves stays on them when the table runs again; two undos take it out', kept && entered === 'entered' && gone, { found, entered, gone });

    // Update records (decision 5). Three Base blocks by name take an attribute, a fourth name in the file only; then a
    // reserves leaf by block and seam takes a period in months. Each report is what its run writes; undone.
    const named = (await projectSummary()).objects.filter((o) => ids.slice(0, 3).includes(o.id));
    const byName = { target: { of: 'objects' as const, objects: ids }, csv: `name,checked\n${named.map((o) => `${o.name},yes`).join('\n')}\nno such block,yes\n`, key: ['name'], mode: 'update' as const };
    const nameReport = await recordsPreview(byName);
    const nameRun = await runCommand('table.update_records', byName);
    const checked = (await objectsAttributes(named.map((o) => o.id))).map((a) => a.checked);
    await undo();
    const unchecked = (await objectsAttributes(named.map((o) => o.id))).map((a) => a.checked);
    expect('a CSV by name sets an attribute on three blocks, says the fourth name is in the file only, and undoes', nameReport.matched === 3 && nameReport.targets === 3 && nameReport.changed === 3 && nameReport.file_only.join() === 'no such block' && checked.every((c) => c === 'yes') && unchecked.every((c) => c === undefined), {
      report: { matched: nameReport.matched, targets: nameReport.targets, changed: nameReport.changed, fileOnly: nameReport.file_only },
      said: nameRun.record.messages,
      checked,
    });
    const leaf = (await tableGrid({ of: 'table', table: baseTable.id }, { columns: [], limit: 1 })).rows.find((r) => r.kind === 'leaf');
    const levels = (await tableGrid({ of: 'table', table: baseTable.id }, { columns: [], limit: 1 })).levels;
    const at = (level: string) => leaf?.words?.[levels.indexOf(level)] ?? '';
    const byLeaf = { target: { of: 'table' as const, table: baseTable.id }, csv: `strip,block,seam,bench,period (months)\n${at('strip')},${at('block')},${at('seam')},${at('bench')},4\n`, key: ['strip', 'block', 'seam', 'bench'], mode: 'update' as const };
    const leafReport = await recordsPreview(byLeaf);
    await runCommand('table.update_records', byLeaf);
    const period = await tableGrid({ of: 'table', table: baseTable.id }, { columns: ['period'], limit: 100_000 });
    const periods = period.rows.filter((r) => r.kind === 'leaf' && r.cells[0] !== null).map((r) => [(r.words ?? []).join(' '), r.cells[0]] as const);
    const months = period.columns[0]?.unit;
    await undo();
    // A calculated column (decision 6): waste over coal by volume, from each group's own inputs. Every strip's is its
    // own waste volume over its coal volume, the total the pit's; traced on a leaf it sets itself; one undo takes it off.
    const ratioScript = 'def formula(row):\n    row.waste_to_coal = row["Insitu\\\\Waste\\\\volume"] / row["Insitu\\\\Coal\\\\volume"]\n';
    const ratioColumn = { name: 'waste_to_coal', unit: 'm3/m3', script: ratioScript, adds: { by: 'from_inputs' as const } };
    await runCommand('table.calculate', { table: baseTable.id, column: ratioColumn });
    started = performance.now();
    const byStripCalc = await tableGrid({ of: 'table', table: baseTable.id }, { group: ['pit', 'strip'], open: [['Pit']], columns: ['waste_to_coal', 'Insitu\\Waste\\volume', 'Insitu\\Coal\\volume'] });
    const calcMs = performance.now() - started;
    const calcRows = byStripCalc.rows.filter((r) => r.kind !== 'leaf');
    const calcWorst = calcRows.reduce((w, r) => {
      const [ratio, waste, coal] = r.cells as (number | null)[];
      return ratio === null || waste === null || coal === null || coal === 0 ? Infinity : Math.max(w, Math.abs(ratio - waste / coal) / Math.abs(waste / coal));
    }, 0);
    const leafRow = (await tableGrid({ of: 'table', table: baseTable.id }, { columns: [], limit: 1 })).rows.find((r) => r.kind === 'leaf');
    const traced = leafRow && leafRow.row !== null ? await calcTrace(baseTable.id, ratioColumn, leafRow.row, 'a leaf') : null;
    await undo();
    const calcGone = (await tableGrid({ of: 'table', table: baseTable.id }, { columns: [], limit: 1 })).every.every((c) => c.name !== 'waste_to_coal');
    expect("a calculated waste-to-coal column: every strip's is its own waste over its coal, traced on a leaf it sets itself, one undo takes it off", calcRows.length > 2 && calcWorst <= 1e-12 && !!traced?.sets.some(([k]) => k === 'waste_to_coal') && calcGone, {
      rows: calcRows.length,
      worst: calcWorst,
      said: byStripCalc.said,
      traced: traced?.sets ?? null,
      gone: calcGone,
      ms: Math.round(calcMs),
    });
    // A script of the project (decision 9) loaded by a calculated column: every strip's ratio its own waste over coal;
    // in the Parameters tab, + New script opens one whose starter checks clean. Two undos take it all off.
    await runCommand('script.save', { name: 'waste', source: 'def ratio(w, c):\n    return w / c if c else None\n' });
    const loadingColumn = { name: 'by_script', script: 'load("waste", "ratio")\ndef formula(row):\n    row.by_script = ratio(row["Insitu\\\\Waste\\\\volume"], row["Insitu\\\\Coal\\\\volume"])\n', adds: { by: 'from_inputs' as const } };
    await runCommand('table.calculate', { table: baseTable.id, column: loadingColumn });
    const byScript = await tableGrid({ of: 'table', table: baseTable.id }, { group: ['pit', 'strip'], open: [['Pit']], columns: ['by_script', 'Insitu\\Waste\\volume', 'Insitu\\Coal\\volume'] });
    const scriptRows = byScript.rows.filter((r) => r.kind !== 'leaf');
    const scriptWorst = scriptRows.reduce((w, r) => {
      const [ratio, waste, coal] = r.cells as (number | null)[];
      return ratio === null || waste === null || !coal ? Infinity : Math.max(w, Math.abs(ratio - waste / coal) / Math.abs(waste / coal));
    }, 0);
    await undo();
    await undo();
    showPanel(api, 'params');
    for (let k = 0; k < 50 && !document.querySelector('[data-slot="new-script"]'); k++) await wait(100);
    document.querySelector<HTMLButtonElement>('[data-slot="new-script"]')?.click();
    for (let k = 0; k < 50 && !document.querySelector('[data-slot="project-script"]'); k++) await wait(100);
    await wait(800);
    const starterOpen = !!document.querySelector('[data-slot="project-script"]');
    const starterSource = (() => {
      const box = document.querySelector<HTMLElement>('[data-slot="project-script"] .cm-editor');
      const view = box ? EditorView.findFromDOM(box) : null;
      return view?.state.doc.toString() ?? '';
    })();
    const starterProblems = document.querySelector('[data-slot="script-problems"]')?.textContent ?? '';
    // Completions (decision 0099): in the new script's box made a table script, `t.` offers the table's calls and
    // `t["` the newest table's columns, each `\` doubled; the guide opens at the table script's section.
    const offeredAt = async (source: string) => {
      const box = document.querySelector<HTMLElement>('[data-slot="project-script"] .cm-editor');
      const view = box ? EditorView.findFromDOM(box) : null;
      if (!view) return [] as string[];
      view.dispatch({ changes: { from: 0, to: view.state.doc.length, insert: source }, selection: { anchor: source.length } });
      view.focus();
      for (let k = 0; k < 30 && completionStatus(view.state) !== null; k++) await wait(50);
      startCompletion(view);
      for (let k = 0; k < 60 && completionStatus(view.state) !== 'active'; k++) await wait(50);
      return currentCompletions(view.state).map((c) => c.label);
    };
    await wait(600);
    const tableCalls = await offeredAt('def run(t):\n    return t.');
    await wait(600);
    const columnsOffered = await offeredAt('def run(t):\n    return t["ROM');
    const libraryOffered = await offeredAt('def run(t):\n    x = in_si');
    const tablesOffered = await offeredAt('def run(t):\n    x = lookup("');
    const guide = document.querySelector('[data-slot="project-script"] [data-slot="script-guide"] summary')?.textContent ?? '';
    await offeredAt(starterSource);
    expect('a table script box offers t.\'s calls, t["…"]\'s columns and the library by name, and its guide is the table scripts\'', ['group', 'where', 'join', 'total', 'with_column'].every((c) => tableCalls.includes(c)) && columnsOffered.some((c) => c.startsWith('ROM\\\\Coal\\\\')) && libraryOffered.includes('in_situ_rd') && tablesOffered.length > 0 && guide === 'How table scripts are written', {
      calls: tableCalls.slice(0, 20),
      columns: columnsOffered.slice(0, 5),
      library: libraryOffered.slice(0, 5),
      lookupTables: tablesOffered.length,
      guide,
    });
    expect('a script loaded by a calculated column gives every strip its own waste over coal; + New script opens one that checks clean', scriptRows.length > 2 && scriptWorst <= 1e-12 && starterOpen && starterProblems === '', {
      rows: scriptRows.length,
      worst: scriptWorst,
      said: byScript.said.map((x) => x.slice(0, 80)),
      open: starterOpen,
      problems: starterProblems,
    });
    // A table script (decision 0099, workstream 7) grouping Base by pit and strip, run as a task: each strip's numbers
    // equal the grid's strip subtotals (the Breakdown's, checked above), and one undo takes the new table away.
    const scriptStarted = performance.now();
    const scripted = await runCommand('table.script', { table: baseTable.id, source: 'def run(t):\n    return t.group(["pit", "strip"])\n', name: 'Base by strip (check)' });
    const scriptMs = performance.now() - scriptStarted;
    const madeId = scripted.record.outputs?.[0]?.id ?? '';
    const strips = await tableGrid({ of: 'table', table: baseTable.id }, { group: ['pit', 'strip'], open: [['Pit']] });
    const made = await tableGrid({ of: 'table', table: madeId }, {});
    const stripAt = made.columns.findIndex((c) => c.name === 'strip');
    const madeAt = (strip: string) => made.rows.find((r) => r.kind === 'leaf' && r.cells[stripAt] === strip);
    let scriptStripWorst = 0;
    let scriptCompared = 0;
    for (const r of strips.rows.filter((r) => r.kind === 'group' && r.depth === 2)) {
      const row = madeAt(r.label);
      if (!row) {
        scriptStripWorst = Infinity;
        continue;
      }
      strips.columns.forEach((c, i) => {
        const want = r.cells[i];
        const at = made.columns.findIndex((m) => m.name === c.name);
        const got = at >= 0 ? row.cells[at] : null;
        if (typeof want !== 'number' || typeof got !== 'number') return;
        scriptCompared++;
        scriptStripWorst = Math.max(scriptStripWorst, Math.abs(got - want) / Math.max(1, Math.abs(want)));
      });
    }
    await undo();
    expect('a table script grouping Base by strip gives each strip the grid\'s subtotals, and undoes', scriptCompared > 100 && scriptStripWorst <= 1e-9, { compared: scriptCompared, worst: scriptStripWorst, ms: Math.round(scriptMs), rows: made.rows.length });
    showPanel(api, 'restable');
    expect('a CSV by strip, block, seam and bench enters a period in months on that one leaf, and undoes', leafReport.targets === 1 && leafReport.changed === 1 && periods.length === 1 && periods[0][1] === 4 && months === 'months', { report: { targets: leafReport.targets, changed: leafReport.changed }, periods, unit: months });

    // A block's attribute typed in Objects (decision 4): a double-click opens the box, Enter keeps it, one undo puts it back.
    showPanel(api, 'restable');
    for (let k = 0; k < 50 && !document.querySelector('select[aria-label="Table"]'); k++) await wait(100);
    const list = document.querySelector<HTMLSelectElement>('select[aria-label="Table"]');
    const tableWas = list?.value ?? '';
    if (list) {
      Object.getOwnPropertyDescriptor(HTMLSelectElement.prototype, 'value')?.set?.call(list, 'objects');
      list.dispatchEvent(new Event('change', { bubbles: true }));
    }
    const objectsPanel = () => document.querySelector<HTMLElement>('[data-tool-panel="blocks"]');
    // The Base blocks' layer, not whichever layer of solids comes first.
    for (let k = 0; k < 50 && !objectsPanel()?.querySelector('select[aria-label="Layer of solids"]'); k++) await wait(100);
    const blocksLayer = (await projectSummary()).objects.find((o) => o.id === ids[0])?.layer ?? '';
    const layerList = objectsPanel()?.querySelector<HTMLSelectElement>('select[aria-label="Layer of solids"]');
    if (layerList && blocksLayer) {
      Object.getOwnPropertyDescriptor(HTMLSelectElement.prototype, 'value')?.set?.call(layerList, blocksLayer);
      layerList.dispatchEvent(new Event('change', { bubbles: true }));
    }
    const cellOf = () => objectsPanel()?.querySelector<HTMLElement>('[data-slot="cell-editable"][aria-label^="block of "]') ?? null;
    for (let k = 0; k < 50 && !cellOf(); k++) await wait(100);
    const cell = cellOf();
    const blockName = cell?.getAttribute('aria-label')?.replace(/^block of /, '').replace(/: double-click to type$/, '') ?? '';
    cell?.dispatchEvent(new MouseEvent('dblclick', { bubbles: true }));
    await wait(100);
    const box = objectsPanel()?.querySelector<HTMLInputElement>('[data-slot="cell-input"]');
    if (box) {
      Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value')?.set?.call(box, 'B99');
      box.dispatchEvent(new Event('input', { bubbles: true }));
      box.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }));
    }
    // A solid's attributes are the engine's; the summary doesn't carry them.
    const typedId = (await projectSummary()).objects.find((o) => o.name === blockName)?.id ?? '';
    const blockOf = async () => (typedId ? (await objectsAttributes([typedId]))[0]?.block : undefined);
    let typedValue = await blockOf();
    for (let k = 0; k < 50 && typedValue !== 'B99'; k++) {
      await wait(100);
      typedValue = await blockOf();
    }
    await undo();
    const backValue = await blockOf();
    // Update records in the tab: a CSV chosen, the report shown, the button writes it; one undo puts it back.
    [...(objectsPanel()?.querySelectorAll<HTMLButtonElement>('button') ?? [])].find((b) => b.textContent?.trim() === 'Update records…')?.click();
    for (let k = 0; k < 50 && !objectsPanel()?.querySelector('input[aria-label="The CSV to update from"]'); k++) await wait(100);
    const fileInput = objectsPanel()?.querySelector<HTMLInputElement>('input[aria-label="The CSV to update from"]');
    if (fileInput) {
      const dt = new DataTransfer();
      dt.items.add(new File([`name,zone\n${blockName},east\n`], 'records.csv', { type: 'text/csv' }));
      fileInput.files = dt.files;
      fileInput.dispatchEvent(new Event('change', { bubbles: true }));
    }
    const recordsReport = () => objectsPanel()?.querySelector('[data-slot="records-report"]')?.textContent ?? '';
    for (let k = 0; k < 50 && !recordsReport(); k++) await wait(100);
    const reportText = recordsReport();
    [...(objectsPanel()?.querySelectorAll<HTMLButtonElement>('[data-slot="update-records"] button') ?? [])].find((b) => b.textContent?.startsWith('Update 1 value'))?.click();
    let zone = typedId ? (await objectsAttributes([typedId]))[0]?.zone : undefined;
    for (let k = 0; k < 50 && zone !== 'east'; k++) {
      await wait(100);
      zone = typedId ? (await objectsAttributes([typedId]))[0]?.zone : undefined;
    }
    await undo();
    const zoneBack = typedId ? (await objectsAttributes([typedId]))[0]?.zone : undefined;
    expect('Update records in Objects: a CSV chosen, its report says 1 record matched 1 solid, and its button sets the value; one undo', reportText.startsWith('1 of 1 record matched 1 solid') && zone === 'east' && zoneBack === undefined, { report: reportText.slice(0, 120), zone, back: zoneBack });

    const shownCells = objectsPanel()?.querySelectorAll('[data-slot="cell-editable"]').length ?? 0;
    const firstCell = objectsPanel()?.querySelector('[data-slot="cell-editable"]')?.getAttribute('aria-label') ?? null;
    // The list is drawn again with Objects: found again to go back.
    const listNow = document.querySelector<HTMLSelectElement>('select[aria-label="Table"]');
    if (listNow && tableWas) {
      Object.getOwnPropertyDescriptor(HTMLSelectElement.prototype, 'value')?.set?.call(listNow, tableWas);
      listNow.dispatchEvent(new Event('change', { bubbles: true }));
    }
    for (let k = 0; k < 50 && !document.querySelector('[data-tool-panel="reserves-table"]'); k++) await wait(100);
    expect("a block's attribute typed in Objects is set on it, and one undo puts it back", !!box && typedValue === 'B99' && backValue !== 'B99' && !!backValue, {
      block: blockName,
      typed: typedValue,
      back: backValue,
      box: !!box,
      cells: shownCells,
      first: firstCell,
    });
  }
  await checkParameters(summary, api, expect, show, picture);
  await checkFormulas(expect, picture, api);
  await checkRunAgain(summary, expect, show, picture, api);
  return { ...results, failures };
}

type Expect = (what: string, ok: boolean, detail?: unknown) => void;

/**
 * Open in the case (decision 0075, 5b; the Reserves case tab since the Reserves case plan's workstream 7 took the Dataflow
 * tab out) on block 3_B1 of the Base table: the case tab opens on the Base table, the block picked on the map. This step
 * unit by unit (the Dataflow tab's step pane): `densities` shows its formula with the lines that ran on the unit followed
 * and rd_is set; `rom_dilution` the four dilution settings it read for it, each with its table and column, and its
 * workings; in_situ, join_benches and final their workings.
 */
async function checkColumn(panel: () => HTMLElement | null, expect: Expect, picture: Picture) {
  const buttonIn = (root: HTMLElement | null | undefined, label: string) => [...(root?.querySelectorAll<HTMLButtonElement>('button') ?? [])].find((b) => b.textContent?.trim() === label) ?? null;
  buttonIn(panel(), 'Open in the case')?.click();
  const tab = () => document.querySelector<HTMLElement>('[data-tool-panel="case"]');
  const picked = () => tab()?.querySelector<HTMLElement>('[data-slot="map-block"][data-picked="yes"]')?.dataset.block ?? '';
  for (let k = 0; k < 150 && !picked().startsWith('Pit 3 B1'); k++) await wait(100);
  const tabs = [...document.querySelectorAll<HTMLElement>('.dv-tab')].map((t) => t.textContent?.trim() ?? '');
  expect('Open in the case opens the Reserves case tab on the block, no Dataflow or Setup tab left', picked() === 'Pit 3 B1' && !tabs.includes('Dataflow') && !tabs.includes('Setup'), { picked: picked(), tabs });
  const pane = () => tab()?.querySelector<HTMLElement>('[data-slot="step-pane"]') ?? null;
  const pick = async (step: string) => {
    tab()?.querySelector<HTMLElement>(`[data-slot="case-step"][data-step="${step}"]`)?.click();
    for (let k = 0; k < 100 && pane()?.dataset.step !== step; k++) {
      if (!pane()) tab()?.querySelector<HTMLButtonElement>('[data-slot="case-unit-by-unit"] button')?.click();
      await wait(100);
    }
  };

  // densities: its formula, the lines that ran on the unit followed tinted, rd_is set.
  await pick('densities');
  for (let k = 0; k < 100 && !pane()?.querySelector('[data-ran="yes"]'); k++) await wait(100);
  const ran = pane()?.querySelectorAll('[data-slot="cell-code"] [data-ran="yes"]').length ?? 0;
  const code = pane()?.querySelector('[data-slot="cell-code"]')?.textContent ?? '';
  expect('densities unit by unit shows its formula, the lines that ran on the unit followed and rd_is set', ran > 3 && code.includes('in_situ_rd(') && /rd_is = [\d.]+/.test(code), { ran });

  // rom_dilution: the settings it read for the unit followed, with where each came from; its workings.
  await pick('rom_dilution');
  for (let k = 0; k < 100 && !pane()?.querySelector('[data-slot="working"]'); k++) await wait(100);
  const reads = [...(pane()?.querySelectorAll<HTMLElement>('[data-slot="cell-read"]') ?? [])].map((r) => r.textContent ?? '');
  const named = ['roof_dilution', 'floor_dilution', 'edge_dilution', 'other_dilution'].every((n) => reads.some((r) => r.startsWith(n) && /\(Reserves assumptions, column \d+\)/.test(r)));
  const workings = [...(pane()?.querySelectorAll<HTMLElement>('[data-slot="working"]') ?? [])].map((w) => w.dataset.what);
  expect('rom_dilution unit by unit shows the four dilution settings it read and where each came from, and its workings', named && workings.some((w) => w?.endsWith('dilution')), { reads: reads.length, workings });
  await picture?.('case-unit-by-unit');

  // Workings for the rest of the built-in steps (decision 0082, workstream 4): in_situ, join_benches and final.
  const whats = async (step: string) => {
    await pick(step);
    for (let k = 0; k < 50 && !pane()?.querySelector('[data-slot="working"]'); k++) await wait(100);
    return [...(pane()?.querySelectorAll<HTMLElement>('[data-slot="working"]') ?? [])].map((w) => w.dataset.what ?? '');
  };
  const inSitu = await whats('in_situ');
  const join = await whats('join_benches');
  const fin = await whats('final');
  expect('in_situ, join_benches and final show their workings unit by unit', inSitu.length > 0 && join.length > 0 && fin.length > 0, { inSitu, join, fin });
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

  // Live (decision 0082, points 4 to 6): rd_is doubled and not saved runs the case on the edit as it's typed. Show in
  // the case brings the Reserves case tab forward on densities, run on edits; Discard in the Parameters tab runs it on
  // the saved formula again.
  const at2 = back.indexOf('rd_is = in_situ_rd(');
  view?.dispatch({ changes: { from: at2 + 'rd_is = '.length, to: at2 + 'rd_is = '.length, insert: '2 * ' } });
  button('Show in the case')?.click();
  const tab = () => document.querySelector<HTMLElement>('[data-tool-panel="case"]');
  const onDensities = () => tab()?.querySelector('[data-slot="case-step"].bg-selection')?.getAttribute('data-step') === 'densities';
  const onEdits = () => (tab()?.querySelector('[data-slot="case-step-pane"]')?.textContent ?? '').includes('run on edits');
  for (let k = 0; k < 150 && !(onDensities() && onEdits()); k++) await wait(100);
  expect('Show in the case brings the case tab forward on densities, run on the formula as edited', onDensities() && onEdits(), { densities: onDensities(), edits: onEdits() });
  if (picture) await picture('case-live-formula');
  showPanel(api, 'params');
  await wait(300);
  const kept = (view && panel()?.contains(view.dom) ? view : null)?.state.doc.toString() ?? '';
  expect('the Parameters tab, put behind the case tab, still holds the edit', kept.includes('rd_is = 2 * in_situ_rd('));
  button('Discard')?.click();
  showPanel(api, 'case');
  for (let k = 0; k < 100 && onEdits(); k++) await wait(100);
  expect('Discard in the Parameters tab runs the case on the saved formula again', !onEdits());
}

/**
 * Run again (decision 0085), in the Reserves case tab on 3_B1: the coal's roof dilution doubled and saved in the
 * Parameters tab, the case's band says Reserves is out of date because the assumptions changed; Run again on this block
 * runs it into the table, the same table at its next revision, and the band says so no more. Every block run again then
 * balances (decision 0086). Three undos put the demo back.
 */
async function checkRunAgain(summary: ProjectSummary, expect: Expect, show: Show, picture: Picture, api: DockviewApi) {
  const assumptions = summary.objects.find((o) => o.name === 'Reserves assumptions');
  if (!assumptions) return;
  const nb = () => document.querySelector<HTMLElement>('[data-tool-panel="case"]');
  const params = () => document.querySelector<HTMLElement>('[data-tool-panel="parameters"]');
  const pressIn = (root: HTMLElement | null, label: string) => [...(root?.querySelectorAll<HTMLButtonElement>('button') ?? [])].find((b) => b.textContent?.trim() === label)?.click();
  showPanel(api, 'params');
  for (let k = 0; k < 50 && !params(); k++) await wait(100);
  params()?.querySelector<HTMLButtonElement>('[data-slot="parameters-item"][data-name="Reserves assumptions"]')?.click();
  for (let k = 0; k < 50 && !params()?.querySelector('[data-slot="parameters-grid"]'); k++) await wait(100);
  const details = await objectDetails(assumptions.id);
  if (details.measures.kind !== 'parameter_table') return;
  const t = details.measures.parameters;
  const ri = t.rows.findIndex((r) => typeof r.values.roof_dilution === 'number' && r.values.roof_dilution > 0);
  const input = params()?.querySelector<HTMLInputElement>(`input[aria-label="Column ${ri + 1}, roof_dilution"]`) ?? null;
  if (!input) return expect('the assumptions give a roof dilution to edit', false);
  Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value')?.set?.call(input, String((t.rows[ri].values.roof_dilution as number) * 2));
  input.dispatchEvent(new Event('input', { bubbles: true }));
  await wait(400);
  pressIn(params(), 'Save');
  await wait(300);
  showPanel(api, 'case');
  const changed = () => nb()?.querySelector('[data-slot="case-open-state"]')?.textContent ?? '';
  for (let k = 0; k < 100 && !changed().includes('Reserves assumptions'); k++) await wait(100);
  const warned = changed().includes('Reserves assumptions');
  const tableName = summary.objects.find((o) => o.kind === 'table' && o.name.includes('Base'))?.name ?? '';
  const said = () => nb()?.querySelector('[data-slot="case-said"]')?.textContent ?? '';
  pressIn(nb()?.querySelector<HTMLElement>('[data-slot="case-run-again"]') ?? null, 'this block');
  for (let k = 0; k < 200 && !said(); k++) await wait(100);
  for (let k = 0; k < 100 && changed(); k++) await wait(100);
  expect(
    'a parameter saved warns the block\'s numbers were made before it; Run again for the block runs it into the table and the warning goes',
    warned && said().endsWith(`run again into ${tableName}.`) && !changed(),
    { warned, said: said().slice(0, 120), left: changed().slice(0, 80) },
  );
  if (picture) await picture('run-again');

  // Every block run again into the table (decision 0086): each block's balance of coal and waste, Imported › Insitu ›
  // ROM › Scheduled with what moved between them, leaves nothing unaccounted; ROM coal in the balance is ROM coal.
  pressIn(nb()?.querySelector<HTMLElement>('[data-slot="case-run-again"]') ?? null, 'every block');
  for (let k = 0; k < 300 && !said().startsWith('Every block run again'); k++) await wait(100);
  const table = summary.objects.find((o) => o.kind === 'table' && o.name === tableName);
  let blocks = 0;
  let worst = 0;
  let romMatches = true;
  let without = 0;
  if (table) {
    const tree = await reservesTree(table.id);
    const under = (n: ReservesNode): ReservesNode[] => (n.path.length === 3 ? [n] : n.children.flatMap(under));
    for (const block of under(tree)) {
      const b = await reservesBreakdown(table.id, block.path, []);
      const at = (name: string) => b.fields.find((f) => f.column === name)?.values[0] ?? null;
      const checks = b.fields.filter((f) => f.column.startsWith('Summary\\Balance\\') && f.name.startsWith('unaccounted'));
      if (checks.length === 0) without++;
      const size = Math.max(1, at('Summary\\Balance\\Total\\imported') ?? 0);
      for (const c of checks) worst = Math.max(worst, Math.abs(c.values[0] ?? 0) / size);
      const rom = at('Summary\\Balance\\Coal\\rom');
      const romCoal = at('ROM\\Coal\\volume');
      if (rom !== null && romCoal !== null && Math.abs(rom - romCoal) > 1e-6 * Math.max(1, romCoal)) romMatches = false;
      blocks++;
    }
  }
  expect(
    'every block run again balances: coal and waste through Imported, Insitu, ROM and Scheduled leave nothing unaccounted',
    said().startsWith('Every block run again') && blocks > 0 && without === 0 && worst < 1e-9 && romMatches,
    { blocks, without, worst, romMatches },
  );
  // The Reserves table tab beside (decision 0086, workstream 2): the node's Balance, a row a stage with every check
  // holding; and back on Breakdown, its coal's thickness shown, which no add-up rule gave before.
  const rt = () => document.querySelector<HTMLElement>('[data-tool-panel="reserves-table"]');
  showPanel(api, 'restable');
  for (let k = 0; k < 50 && !rt(); k++) await wait(100);
  pressIn(rt(), 'Balance');
  const balance = () => rt()?.querySelector<HTMLElement>('[data-slot="reserves-balance"]') ?? null;
  for (let k = 0; k < 50 && !balance()?.querySelector('tr[data-kind]'); k++) await wait(100);
  const stages = [...(balance()?.querySelectorAll<HTMLElement>('tr[data-kind="stage"]') ?? [])].map((r) => r.dataset.balance);
  const checks = [...(balance()?.querySelectorAll<HTMLElement>('tr[data-kind="check"]') ?? [])].map((r) => r.dataset.holds);
  // The Reserves table as a tab again, in front, filling the window for its picture.
  pressIn(nb(), 'Tables as a tab');
  await wait(300);
  showPanel(api, 'restable');
  await wait(300);
  if (picture) {
    document.querySelector<HTMLButtonElement>('button[aria-label="Fill the window"]')?.click();
    await wait(300);
    await picture('reserves-balance');
    document.querySelector<HTMLButtonElement>('button[aria-label="Put the tables back under the view"]')?.click();
    await wait(300);
  }
  pressIn(rt(), 'Breakdown');
  for (let k = 0; k < 50 && !rt()?.querySelector('[data-slot="reserves-breakdown"]'); k++) await wait(100);
  const thick = [...(rt()?.querySelectorAll<HTMLElement>('tr[data-field]') ?? [])].find((r) => r.dataset.field?.endsWith('Coal\\thickness'));
  const thickness = thick?.querySelectorAll('td')[1]?.textContent?.trim() ?? '';
  expect(
    "the Reserves table tab's Balance shows Imported, Insitu, ROM and Scheduled with every check holding, and Breakdown shows the coal's thickness",
    JSON.stringify(stages) === JSON.stringify(['imported', 'insitu', 'rom', 'scheduled']) && checks.length === 3 && checks.every((h) => h === 'yes') && thickness !== '',
    { stages, checks, thickness },
  );
  // Blocks coloured by a reserves field (decision 0075, point 7): ROM coal volume from its row, each solid its leaf's,
  // then by block; Stop puts the layers' colours back.
  const legend = () => document.querySelector<HTMLElement>('[data-slot="field-legend"]');
  const legendCount = () => Number((legend()?.textContent ?? '').match(/([\d,]+) solids?/)?.[1]?.replaceAll(',', '') ?? 0);
  const romRow = [...(rt()?.querySelectorAll<HTMLElement>('tr[data-field]') ?? [])].find((r) => r.dataset.field === 'ROM\\Coal\\volume');
  romRow?.querySelector<HTMLButtonElement>('[data-slot="colour-by"]')?.click();
  for (let k = 0; k < 100 && !legend(); k++) await wait(100);
  const byLeaf = legendCount();
  const title = legend()?.textContent ?? '';
  legend()?.querySelector<HTMLButtonElement>('button[aria-label="By block"]')?.click();
  for (let k = 0; k < 100 && legend()?.querySelector('button[aria-label="By block"][data-state="checked"]') === null; k++) await wait(100);
  await wait(300);
  const byBlock = legendCount();
  if (picture) await picture('blocks-by-field');
  [...(legend()?.querySelectorAll<HTMLButtonElement>('button') ?? [])].find((b) => b.textContent?.trim() === 'Stop')?.click();
  for (let k = 0; k < 50 && legend(); k++) await wait(100);
  expect(
    'blocks coloured by a reserves field from its row: ROM coal volume, each solid its leaf\'s, then by block; Stop puts them back',
    !!romRow && byLeaf > 0 && title.includes('ROM › Coal › volume') && byBlock >= byLeaf && !legend(),
    { byLeaf, byBlock, title: title.slice(0, 60) },
  );

  showPanel(api, 'case');
  await wait(300);
  await show(await undo());
  await show(await undo());
  await show(await undo());
  await wait(300);
}
