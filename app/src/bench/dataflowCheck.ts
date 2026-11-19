// `--dataflow <project.zdp>`: Properties' views of a parameter table, a dataflow and a results table (decisions 0072,
// 0073), read as Tim would see them. The project (the demo pit saved by testdata/synthetic/workflow_demo.py, its
// dataflow the starter's) is opened, each object picked in its own tab's list (the layer tree has no row for them,
// Filtering plan L4), and what Properties shows read back.
import { EditorView } from '@codemirror/view';
import type { DockviewApi } from 'dockview-react';
import { concealed, objectDetails, objectsAttributes, openProject, projectSummary, type ProjectSummary, type ReservesNode, reservesBreakdown, reservesNodeOf, reservesSolids, reservesTree, undo, writePicture } from '../bridge';
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
        pick = document.querySelector<HTMLSelectElement>('[data-tool-panel="reserves-table"] select[aria-label="Reserves table"]');
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
  }
  await checkParameters(summary, api, expect, show, picture);
  await checkFormulas(expect, picture, api);
  await checkLive(summary, expect, picture);
  await checkRunAgain(summary, expect, show, picture, api);
  await checkBeside(expect, api);
  return { ...results, failures };
}

type Expect = (what: string, ok: boolean, detail?: unknown) => void;

/**
 * Open its column (decision 0075, 5b) on block 3_B1 of the Base table, in the Dataflow tab (decision 0084): the strip a
 * step a chip, the step picked shown in the step pane beside Before and After. `densities` shows its formula with the
 * lines that ran on the unit followed tinted and rd_is set, reads RD lit gold and changes rd_is underlined blue;
 * `rom_dilution` shows the four dilution settings it read for it, each with its table and column, its workings, and its
 * dilution as new units in After, thickness, volume, area and length lit. Whole column at rom_dilution: the units sent
 * to U's ROM coal section add up to the table's 10,840.1 m³, more than after rom_loss (← steps back). Dataflow goes
 * back; a row clicked follows its unit; → steps on; a formula's Edit opens it in the Parameters tab.
 */
async function checkColumn(panel: () => HTMLElement | null, expect: Expect, picture: Picture) {
  const buttonIn = (root: HTMLElement | null | undefined, label: string) => [...(root?.querySelectorAll<HTMLButtonElement>('button') ?? [])].find((b) => b.textContent?.trim() === label) ?? null;
  buttonIn(panel(), 'Open its column')?.click();
  const nb = () => document.querySelector<HTMLElement>('[data-tool-panel="notebook"]');
  const strip = () => nb()?.querySelector<HTMLElement>('[data-slot="dataflow-strip"]') ?? null;
  const chip = (step: string) => strip()?.querySelector<HTMLButtonElement>(`[data-strip-step="${step}"]`) ?? null;
  const chips = () => [...(strip()?.querySelectorAll<HTMLElement>('[data-strip-step]') ?? [])].map((c) => c.dataset.stripStep);
  const pane = () => nb()?.querySelector<HTMLElement>('[data-slot="step-pane"]') ?? null;
  const before = () => nb()?.querySelector<HTMLElement>('[data-slot="before-table"]') ?? null;
  const after = () => nb()?.querySelector<HTMLElement>('[data-slot="after-table"]') ?? null;
  const heads = (t: HTMLElement | null, flag: 'read' | 'wrote') => [...(t?.querySelectorAll<HTMLElement>(`th[data-${flag}="yes"]`) ?? [])].map((h) => h.dataset.field);
  const pick = async (step: string) => {
    chip(step)?.click();
    for (let k = 0; k < 50 && pane()?.dataset.step !== step; k++) await wait(50);
  };
  for (let k = 0; k < 100 && chips().length === 0; k++) await wait(100);
  const want = ['as input', 'small_volumes', 'initialise', 'fill_size', 'limits', 'densities', 'in_situ', 'join_benches', 'discard', 'working_sections', 'rom_loss', 'rom_dilution', 'rom_ash_cutoff', 'rom', 'rom_qualities', 'final'];
  expect("Open its column opens the Dataflow tab, a chip for each of the dataflow's steps in run order", JSON.stringify(chips()) === JSON.stringify(want), chips());
  const tabs = [...document.querySelectorAll<HTMLElement>('.dv-tab')].map((t) => t.textContent?.trim() ?? '');
  expect('the tab is called Dataflow, a layout saved with the Notebook in it too', tabs.includes('Dataflow') && !tabs.includes('Notebook'), tabs);
  expect('a step is picked as it opens, in the step pane beside Before and After, the cells gone', !!pane() && !!before() && !!after() && !nb()?.querySelector('[data-slot="notebook-cell"]'), pane()?.dataset.step);

  // densities: its formula, the lines that ran on the unit followed tinted, rd_is set; RD read, rd_is changed.
  await pick('densities');
  for (let k = 0; k < 100 && !pane()?.querySelector('[data-ran="yes"]'); k++) await wait(100);
  const ran = pane()?.querySelectorAll('[data-slot="cell-code"] [data-ran="yes"]').length ?? 0;
  const code = pane()?.querySelector('[data-slot="cell-code"]')?.textContent ?? '';
  expect('densities shows its formula, the lines that ran on the unit followed and rd_is set', ran > 3 && code.includes('in_situ_rd(') && /rd_is = [\d.]+/.test(code), { ran });
  const dRead = heads(after(), 'read');
  const dWrote = heads(after(), 'wrote');
  expect(
    'densities: RD and the moistures lit as read, rd_is underlined as changed, in Before and After',
    ['rd', 'mad', 'mis'].every((f) => dRead.includes(f)) && dWrote.includes('rd_is') && !dRead.includes('rd_is') && JSON.stringify(heads(before(), 'read')) === JSON.stringify(dRead),
    { read: dRead, wrote: dWrote },
  );

  // rom_dilution: the settings it read for the unit followed, with where each came from; its workings; its dilution new.
  await pick('rom_dilution');
  const reads = [...(pane()?.querySelectorAll<HTMLElement>('[data-slot="cell-read"]') ?? [])].map((r) => r.textContent ?? '');
  const named = ['roof_dilution', 'floor_dilution', 'edge_dilution', 'other_dilution'].every((n) => reads.some((r) => r.startsWith(n) && /\(Reserves assumptions, column \d+\)/.test(r)));
  const rule = pane()?.querySelector('[data-slot="cell-rule"]')?.textContent ?? '';
  const workings = [...(pane()?.querySelectorAll<HTMLElement>('[data-slot="working"]') ?? [])].map((w) => w.dataset.what);
  expect('rom_dilution shows its rule, the four dilution settings it read and where each came from, and its workings', named && rule.includes('cuts each waste interval') && workings.some((w) => w?.endsWith('dilution')), { reads: reads.length, workings });
  const made = after()?.querySelectorAll('tr[data-mark="new"]').length ?? 0;
  const dilRead = heads(after(), 'read');
  const changed = after()?.querySelectorAll('td[data-changed="yes"]').length ?? 0;
  expect(
    'rom_dilution: its dilution new in After, values it changed blue, thickness, volume, area and length lit',
    made > 0 && changed > 0 && JSON.stringify(dilRead) === JSON.stringify(['volume', 'thickness', 'area', 'length']) && heads(after(), 'wrote').includes('volume'),
    { made, changed, read: dilRead },
  );

  // Workings for the rest of the built-in steps (decision 0082, workstream 4): in_situ, join_benches and final, for U.
  const whats = async (step: string) => {
    await pick(step);
    return [...(pane()?.querySelectorAll<HTMLElement>('[data-slot="working"]') ?? [])].map((w) => w.dataset.what ?? '');
  };
  const rest = { in_situ: await whats('in_situ'), join_benches: await whats('join_benches'), final: await whats('final') };
  expect(
    'in_situ, join_benches and final show their workings for the unit followed',
    rest.in_situ.some((w) => w.startsWith('in situ')) && rest.join_benches.includes('join') && rest.final.includes('ROM coal') && rest.final.includes('clean coal'),
    rest,
  );
  if (picture) {
    document.querySelector<HTMLButtonElement>('button[aria-label="Fill the window"]')?.click();
    await wait(300);
    await picture('workings-final');
    document.querySelector<HTMLButtonElement>('button[aria-label="Put the tables back under the view"]')?.click();
    await wait(300);
  }
  await pick('rom_dilution');

  // The strip (decision 0084): what each wrote under it, units on the arrows, the picked step's table and column lit.
  const writesOf = (step: string) => [...(chip(step)?.querySelectorAll('[data-slot="step-writes"] > span') ?? [])].map((w) => (w.textContent ?? '').split(' ')[0]);
  expect(
    'the strip shows Imported writing Imported, rom_dilution Dilution and final ROM',
    writesOf('as input').includes('Imported') && writesOf('rom_dilution').includes('Dilution') && writesOf('final').includes('ROM'),
    { imported: writesOf('as input'), dilution: writesOf('rom_dilution'), final: writesOf('final') },
  );
  const arrows = [...(strip()?.querySelectorAll('[data-slot="step-units"]') ?? [])].map((a) => Number(a.textContent?.replace('→', '')));
  expect('each arrow carries the units passed on', arrows.length === want.length - 1 && arrows.every((n) => n > 0), arrows);
  const lit = [...(strip()?.querySelectorAll<HTMLElement>('[data-slot="dataflow-inputs"] [data-lit="yes"]') ?? [])].map((t) => t.textContent ?? '');
  expect('rom_dilution picked is pressed in the strip, Reserves assumptions and its column lit in Inputs', chip('rom_dilution')?.getAttribute('aria-pressed') === 'true' && lit.length === 1 && /^Reserves assumptions · column \d+/.test(lit[0]), lit);

  // A row clicked follows its unit: OB in Before, the In line then OB's.
  const followedIn = (t: HTMLElement | null) => t?.querySelector<HTMLElement>('tr[data-followed="yes"] td')?.textContent ?? '';
  [...(before()?.querySelectorAll<HTMLElement>('tbody tr') ?? [])].find((r) => r.querySelector('td')?.textContent?.startsWith('OB'))?.click();
  for (let k = 0; k < 50 && !followedIn(before()).startsWith('OB'); k++) await wait(50);
  const inLine = pane()?.querySelector('[data-slot="cell-in"]')?.textContent ?? '';
  expect('a row clicked in Before follows its unit: its row marked, In says what was read for it', followedIn(before()).startsWith('OB') && inLine.startsWith('InOB'), inLine.slice(0, 40));

  // → steps on to the next step, ← back.
  nb()?.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowRight', bubbles: true }));
  for (let k = 0; k < 50 && pane()?.dataset.step !== 'rom_ash_cutoff'; k++) await wait(50);
  const stepped = pane()?.dataset.step;
  nb()?.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowLeft', bubbles: true }));
  for (let k = 0; k < 50 && pane()?.dataset.step !== 'rom_dilution'; k++) await wait(50);
  expect('→ picks the next step, ← the one before', stepped === 'rom_ash_cutoff' && pane()?.dataset.step === 'rom_dilution', stepped);
  // Back to following U's coal in its working section, not a loss cut from it.
  [...(before()?.querySelectorAll<HTMLElement>('tbody tr') ?? [])].find((r) => /^U[^·]*coal · section/.test(r.querySelector('td')?.textContent ?? ''))?.click();
  for (let k = 0; k < 50 && !followedIn(before()).startsWith('U'); k++) await wait(50);

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
  for (let k = 0; k < 50 && pane()?.dataset.step !== 'working_sections'; k++) await wait(50);
  expect('a step clicked in the graph is picked, in the step pane', pane()?.dataset.step === 'working_sections');
  buttonIn(nb(), 'Show as strip')?.click();
  for (let k = 0; k < 50 && !strip(); k++) await wait(50);
  await pick('rom_dilution');
  if (picture) {
    document.querySelector<HTMLButtonElement>('button[aria-label="Fill the window"]')?.click();
    await wait(300);
    await picture('dataflow');
    await pick('densities');
    for (let k = 0; k < 100 && !pane()?.querySelector('[data-ran="yes"]'); k++) await wait(100);
    await picture('dataflow-formula');
    document.querySelector<HTMLButtonElement>('button[aria-label="Put the tables back under the view"]')?.click();
    await wait(300);
    await pick('rom_dilution');
  }

  // Whole column at rom_dilution: the grid of every unit, U's ROM coal the table's.
  buttonIn(pane(), 'Whole column')?.click();
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
  buttonIn(nb(), 'Dataflow')?.click();
  for (let k = 0; k < 50 && !pane(); k++) await wait(100);
  expect('Dataflow goes back to the strip and the step pane, the step still picked', pane()?.dataset.step === 'rom_dilution' && !!strip());

  // A formula's Edit opens it in the Parameters tab.
  await pick('densities');
  buttonIn(pane(), 'Edit in the formula panel')?.click();
  const params = () => document.querySelector<HTMLElement>('[data-tool-panel="parameters"]');
  for (let k = 0; k < 100 && !params()?.querySelector('[data-formula="yes"][data-step="densities"].bg-selection'); k++) await wait(100);
  expect("a formula step's Edit opens it in the Parameters tab", !!params()?.querySelector('[data-formula="yes"][data-step="densities"].bg-selection') && !!params()?.querySelector('[data-slot="script-editor"]'));
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

  // Live (decision 0082, points 4 to 6): rd_is doubled and not saved runs the Dataflow tab's column (3_B1, opened above)
  // on the edit as it's typed. Show in Dataflow brings the tab forward on densities, marked edited, the worked examples
  // it breaks flagged, not refused, and rd_is moved against the saved run; the Parameters tab still holds the edit, and
  // Discard there runs the column on the saved formula again.
  const at2 = back.indexOf('rd_is = in_situ_rd(');
  view?.dispatch({ changes: { from: at2 + 'rd_is = '.length, to: at2 + 'rd_is = '.length, insert: '2 * ' } });
  button('Show in Dataflow')?.click();
  const nb = () => document.querySelector<HTMLElement>('[data-tool-panel="notebook"]');
  const dens = () => nb()?.querySelector<HTMLElement>('[data-slot="step-pane"][data-step="densities"]') ?? null;
  const ranEdit = () => (dens()?.querySelector('[data-slot="cell-code"]')?.textContent ?? '').includes('2 * in_situ_rd(') && !!dens()?.querySelector('[data-ran="yes"]');
  for (let k = 0; k < 100 && !(nb()?.querySelector('[data-slot="notebook-edited"]') && ranEdit()); k++) await wait(100);
  const code = dens()?.querySelector('[data-slot="cell-code"]')?.textContent ?? '';
  const failing = nb()?.querySelector('[data-slot="notebook-failing"]')?.textContent ?? '';
  const badge = [...(dens()?.querySelectorAll('span') ?? [])].some((e) => e.textContent === 'edited');
  const movedRdIs = [...(nb()?.querySelectorAll<HTMLElement>('[data-slot="after-table"] td[data-moved="yes"]') ?? [])].length;
  expect(
    'an edited formula runs live in the Dataflow tab: densities picked, marked edited, its failing worked examples flagged, not refused',
    code.includes('2 * in_situ_rd(') && /rd_is = [\d.]+/.test(code) && badge && failing.includes('the formula step densities, test'),
    { badge, failing: failing.slice(0, 120) },
  );
  expect('what the edit moved is marked against the saved run', movedRdIs > 0 && (nb()?.querySelector('[data-slot="notebook-edited"]')?.textContent ?? '').includes('Reserves'), movedRdIs);
  if (picture) {
    document.querySelector<HTMLButtonElement>('button[aria-label="Fill the window"]')?.click();
    await picture('dataflow-live-formula');
    document.querySelector<HTMLButtonElement>('button[aria-label="Put the tables back under the view"]')?.click();
    await wait(300);
  }
  showPanel(api, 'params');
  await wait(300);
  const kept = (view && panel()?.contains(view.dom) ? view : null)?.state.doc.toString() ?? '';
  expect('the Parameters tab, put behind the Dataflow tab, still holds the edit', kept.includes('rd_is = 2 * in_situ_rd('));
  button('Discard')?.click();
  showPanel(api, 'notebook');
  for (let k = 0; k < 100 && (nb()?.querySelector('[data-slot="notebook-edited"]') || !dens()?.querySelector('[data-ran="yes"]')); k++) await wait(100);
  const savedCode = dens()?.querySelector('[data-slot="cell-code"]')?.textContent ?? '';
  expect('Discard in the Parameters tab runs the column on the saved formula again', !nb()?.querySelector('[data-slot="notebook-edited"]') && savedCode.includes('rd_is = in_situ_rd(') && !savedCode.includes('2 * in_situ_rd('));
}

/**
 * Live on a parameter table (decision 0082, points 4 to 6), the Parameters beside the Dataflow tab: the coal's roof
 * dilution doubled and not saved runs 3_B1's column again in about a third of a second, rom_dilution's Dilution write and
 * the units it moved marked against the saved run, the table marked edited in Inputs. A word typed in the cell doesn't
 * check: the last run stays, and why is said. Discard puts the saved run back.
 */
async function checkLive(summary: ProjectSummary, expect: Expect, picture: Picture) {
  const assumptions = summary.objects.find((o) => o.name === 'Reserves assumptions');
  if (!assumptions) return;
  const nb = () => document.querySelector<HTMLElement>('[data-tool-panel="notebook"]');
  const params = () => document.querySelector<HTMLElement>('[data-tool-panel="parameters"]');
  const pressIn = (root: HTMLElement | null, label: string) => [...(root?.querySelectorAll<HTMLButtonElement>('button') ?? [])].find((b) => b.textContent?.trim() === label)?.click();
  pressIn(nb(), 'Parameters beside');
  for (let k = 0; k < 50 && !params(); k++) await wait(100);
  params()?.querySelector<HTMLButtonElement>('[data-slot="parameters-item"][data-name="Reserves assumptions"]')?.click();
  for (let k = 0; k < 50 && !params()?.querySelector('[data-slot="parameters-grid"]'); k++) await wait(100);
  nb()?.querySelector<HTMLButtonElement>('[data-strip-step="rom_dilution"]')?.click();
  await wait(200);
  const details = await objectDetails(assumptions.id);
  if (details.measures.kind !== 'parameter_table') return expect('the assumptions open as a parameter table', false);
  const t = details.measures.parameters;
  const ri = t.rows.findIndex((r) => typeof r.values.roof_dilution === 'number' && r.values.roof_dilution > 0);
  if (ri < 0) return expect('the assumptions give a roof dilution', false);
  const roof = t.rows[ri].values.roof_dilution as number;
  const cell = () => params()?.querySelector<HTMLInputElement>(`input[aria-label="Column ${ri + 1}, roof_dilution"]`) ?? null;
  const type = (text: string) => {
    const input = cell();
    if (!input) return;
    Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value')?.set?.call(input, text);
    input.dispatchEvent(new Event('input', { bubbles: true }));
  };
  // A setting clicked in In, then one named in Workings, opens its cell (decision 0082, point 7): in sight, focused, ringed.
  const asked = () => params()?.querySelector<HTMLInputElement>('input[data-asked="yes"]') ?? null;
  const askedFor = async (name: string) => {
    for (let k = 0; k < 50 && !asked()?.getAttribute('aria-label')?.endsWith(`, ${name}`); k++) await wait(50);
    const a = asked();
    return { cell: a?.getAttribute('aria-label') ?? null, focused: !!a && document.activeElement === a };
  };
  nb()?.querySelector<HTMLButtonElement>('[data-slot="step-pane"] button[data-slot="cell-read"][data-name="roof_dilution"]')?.click();
  const fromIn = await askedFor('roof_dilution');
  nb()?.querySelector<HTMLButtonElement>('[data-slot="step-pane"] button[data-slot="working-setting"][data-name="floor_dilution"]')?.click();
  const fromWorkings = await askedFor('floor_dilution');
  const rowOf = (name: string) => t.rows.findIndex((r) => typeof r.values[name] === 'number' && r.values[name] > 0);
  expect(
    "a setting clicked in In or named in Workings opens its cell in the Parameters tab, focused and ringed",
    fromIn.focused && fromIn.cell === `Column ${ri + 1}, roof_dilution` && fromWorkings.focused && fromWorkings.cell === `Column ${rowOf('floor_dilution') + 1}, floor_dilution`,
    { fromIn, fromWorkings },
  );
  const banner = () => nb()?.querySelector('[data-slot="notebook-edited"]')?.textContent ?? '';
  const movedWrite = () => nb()?.querySelector('[data-strip-step="rom_dilution"] [data-moved="yes"]') ?? null;
  const started = performance.now();
  type(String(roof * 2));
  for (let k = 0; k < 200 && !movedWrite(); k++) await wait(25);
  const ms = Math.round(performance.now() - started);
  const moved = nb()?.querySelectorAll('[data-slot="after-table"] td[data-moved="yes"]').length ?? 0;
  const chip = nb()?.querySelector('[data-input^="table:"][data-lit="yes"]')?.textContent ?? '';
  expect(
    "a parameter edited and not saved runs the column again live: rom_dilution's write and the units it moved marked, the table edited in Inputs",
    !!movedWrite() && moved > 0 && banner().includes('Reserves assumptions') && chip.includes('edited') && ms < 3000,
    { ms, moved, write: movedWrite()?.textContent },
  );
  if (picture) {
    document.querySelector<HTMLButtonElement>('button[aria-label="Fill the window"]')?.click();
    await wait(300);
    await picture('dataflow-live');
    document.querySelector<HTMLButtonElement>('button[aria-label="Put the tables back under the view"]')?.click();
    await wait(300);
  }
  type('3O');
  for (let k = 0; k < 50 && !nb()?.querySelector('[data-slot="live-why"]'); k++) await wait(50);
  expect('an edit that does not check keeps the last run that worked, and says why', !!nb()?.querySelector('[data-slot="live-why"]') && !!movedWrite() && banner() !== '');
  pressIn(params(), 'Discard');
  for (let k = 0; k < 100 && (banner() || movedWrite() || nb()?.querySelector('[data-slot="live-why"]')); k++) await wait(50);
  const left = { banner: banner().slice(0, 60), write: movedWrite()?.textContent ?? null, why: nb()?.querySelector('[data-slot="live-why"]')?.textContent?.slice(0, 120) ?? null, moved: nb()?.querySelectorAll('td[data-moved="yes"]').length ?? 0 };
  expect('Discard puts the saved run back', !left.banner && !left.write && !left.why && left.moved === 0, left);
  pressIn(nb(), 'Parameters as a tab');
  await wait(300);
}

/**
 * Run again (decision 0085), the Parameters beside the Dataflow tab on 3_B1: the coal's roof dilution doubled and saved,
 * the block's column says the assumptions changed since the table's numbers for it were made; Run again for the block
 * runs it into the table, the same table at its next revision, and the warning goes. Every block run again then
 * balances (decision 0086). Three undos put the demo back.
 */
async function checkRunAgain(summary: ProjectSummary, expect: Expect, show: Show, picture: Picture, api: DockviewApi) {
  const assumptions = summary.objects.find((o) => o.name === 'Reserves assumptions');
  if (!assumptions) return;
  const nb = () => document.querySelector<HTMLElement>('[data-tool-panel="notebook"]');
  const params = () => document.querySelector<HTMLElement>('[data-tool-panel="parameters"]');
  const pressIn = (root: HTMLElement | null, label: string) => [...(root?.querySelectorAll<HTMLButtonElement>('button') ?? [])].find((b) => b.textContent?.trim() === label)?.click();
  pressIn(nb(), 'Parameters beside');
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
  const changed = () => nb()?.querySelector('[data-slot="column-changed"]')?.textContent ?? '';
  for (let k = 0; k < 100 && !changed().includes('Reserves assumptions'); k++) await wait(100);
  const warned = changed().includes('Reserves assumptions');
  const tableName = summary.objects.find((o) => o.kind === 'table' && o.name.includes('Base'))?.name ?? '';
  const range = nb()?.querySelector<HTMLSelectElement>('[data-slot="run-again"] select');
  const said = () => nb()?.querySelector('[data-slot="run-again-said"]')?.textContent ?? '';
  pressIn(nb(), 'Run again');
  for (let k = 0; k < 200 && !said(); k++) await wait(100);
  for (let k = 0; k < 100 && changed(); k++) await wait(100);
  expect(
    'a parameter saved warns the block\'s numbers were made before it; Run again for the block runs it into the table and the warning goes',
    warned && range?.value === 'block' && said().endsWith(`run again into ${tableName}.`) && !changed(),
    { warned, range: range?.value, said: said().slice(0, 120), left: changed().slice(0, 80) },
  );
  if (picture) await picture('run-again');

  // Every block run again into the table (decision 0086): each block's balance of coal and waste, Imported › Insitu ›
  // ROM › Scheduled with what moved between them, leaves nothing unaccounted; ROM coal in the balance is ROM coal.
  const select = nb()?.querySelector<HTMLSelectElement>('[data-slot="run-again"] select');
  if (select) {
    Object.getOwnPropertyDescriptor(HTMLSelectElement.prototype, 'value')?.set?.call(select, 'all');
    select.dispatchEvent(new Event('change', { bubbles: true }));
  }
  await wait(200);
  pressIn(nb(), 'Run again');
  for (let k = 0; k < 300 && !said().startsWith('every block run again'); k++) await wait(100);
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
    said().startsWith('every block run again') && blocks > 0 && without === 0 && worst < 1e-9 && romMatches,
    { blocks, without, worst, romMatches },
  );
  // The Reserves table tab beside (decision 0086, workstream 2): the node's Balance, a row a stage with every check
  // holding; and back on Breakdown, its coal's thickness shown, which no add-up rule gave before.
  const rt = () => document.querySelector<HTMLElement>('[data-tool-panel="reserves-table"]');
  pressIn(nb(), 'Reserves table beside');
  for (let k = 0; k < 50 && !rt(); k++) await wait(100);
  pressIn(rt(), 'Balance');
  const balance = () => rt()?.querySelector<HTMLElement>('[data-slot="reserves-balance"]') ?? null;
  for (let k = 0; k < 50 && !balance()?.querySelector('tr[data-kind]'); k++) await wait(100);
  const stages = [...(balance()?.querySelectorAll<HTMLElement>('tr[data-kind="stage"]') ?? [])].map((r) => r.dataset.balance);
  const checks = [...(balance()?.querySelectorAll<HTMLElement>('tr[data-kind="check"]') ?? [])].map((r) => r.dataset.holds);
  // The Reserves table as a tab again, in front, filling the window for its picture.
  pressIn(nb(), 'Reserves table as a tab');
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

  showPanel(api, 'notebook');
  await wait(300);
  await show(await undo());
  await show(await undo());
  await show(await undo());
  await wait(300);
  pressIn(nb(), 'Parameters as a tab');
  await wait(300);
}

/**
 * Beside the Dataflow tab: the Parameters and the Reserves table each put in their own half of its room with a press, and
 * back as tabs with another; with the Reserves table beside, picking another block there runs its column in the Dataflow tab
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
  expect('Parameters beside and Reserves table beside put each in its own half beside the Dataflow tab, all three in sight', apart && !!nb() && !!rt() && !!document.querySelector('[data-tool-panel="parameters"]'));

  const shown = () => nb()?.querySelector('[aria-label="Column shown"]')?.textContent ?? '';
  const was = shown();
  const other = [...(rt()?.querySelectorAll<HTMLElement>('[data-slot="reserves-node"]') ?? [])].find((r) => {
    const path = (r.dataset.path ?? '').split(' > ');
    return path.length === 3 && path[0] === 'Pit' && !was.endsWith(`› ${path[2]}`) && !was.includes(`› ${path[2]} ›`);
  });
  other?.click();
  const want = (other?.dataset.path ?? '').split(' > ').join(' › ');
  for (let k = 0; k < 100 && !(shown() === want && nb()?.querySelector('[data-slot="step-pane"]')); k++) await wait(100);
  expect('the Dataflow tab follows a block picked in the Reserves table beside it, with no press of Open its column', !!other && shown() === want, { was, now: shown() });

  press('Parameters as a tab');
  press('Reserves table as a tab');
  await wait(400);
  expect('pressed again, they go back as tabs behind the Dataflow tab', group('params') === group('notebook') && group('restable') === group('notebook'));
}
