// `--dataflow <project.zdp>`: Properties' views of a parameter table, a dataflow and a results table (decisions 0072,
// 0073), read as Tim would see them. The project (the demo pit saved by testdata/synthetic/workflow_demo.py, its
// dataflow the starter's) is opened, each object clicked in the Layers panel, and what Properties shows read back.
import type { DockviewApi } from 'dockview-react';
import { openProject, type ProjectSummary } from '../bridge';
import { showPanel } from '../workspace/Workspace';

const wait = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));

type Show = (summary: ProjectSummary) => Promise<void> | undefined;

export async function checkDataflow(path: string, log: (line: string) => void, show: Show, dock: () => DockviewApi | null) {
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
    !!flow && flow.includes('small_volumes, initialise, in_situ, join_benches, working_sections') && flow.includes('Reserves assumptions') && flow.includes('by tonnes (rd)') && flow.includes('attribute mis'),
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
  const base = await shown('Reserves Base', 'in situ by horizon');
  for (let k = 0; k < 50 && !(document.body.textContent ?? '').includes('sections by horizon'); k++) await wait(100);
  const parts = ['Results table', 'in situ by horizon', 'in situ by bench', 'sections by horizon', 'Coal', '6,461,308', 'tonnes ad t'].map((w) => [w, !!(document.body.textContent ?? '').includes(w)] as const);
  expect('a results table shows its in situ rows by horizon and by bench and its working sections, coal and waste totalled', parts.every(([, ok]) => ok), { found: !!base, parts, around: base ? base.slice(Math.max(0, base.indexOf('Results table') - 50), base.indexOf('Results table') + 400) : null });
  return { ...results, failures };
}
