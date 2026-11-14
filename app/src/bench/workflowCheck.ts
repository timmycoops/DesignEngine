// `--workflow`: the Run log's Capture and the Workflow panel (decision 0065) driven as Tim would, through their own
// buttons. A made-up pit 100 m by 60 m from RL 0 to 100 made between two surfaces and cut at RL 40 (240,000 and 360,000
// m³); the two runs ticked in the Run log and captured; the Workflow panel shows the steps and their rules; Run makes the
// same two pieces again as their next revisions; the Cut solids panel's button reads Add to workflow while adding; a
// pause added and run to; one undo takes the run back.
import type { DockviewApi } from 'dockview-react';
import { objectDetails, type ProjectSummary, projectSummary, runCommand, undo } from '../bridge';
import { showPanel } from '../workspace/Workspace';
import type { Viewport } from '../viewport/Viewport';

const wait = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));

type Show = (summary: ProjectSummary) => Promise<void> | undefined;

export async function checkWorkflow(_viewport: Viewport, log: (line: string) => void, show: Show, dock: () => DockviewApi | null, selection: () => string[]) {
  const api = dock();
  if (!api) return { error: 'no dock' };
  const results: Record<string, unknown> = {};
  const failures: string[] = [];
  const expect = (what: string, ok: boolean, detail?: unknown) => {
    results[what] = detail === undefined ? ok : { ok, detail };
    if (!ok) failures.push(what);
    log(`workflow: ${what}: ${ok ? 'yes' : 'NO'}${detail === undefined ? '' : ` (${JSON.stringify(detail)})`}`);
  };
  const buttons = () => [...document.querySelectorAll<HTMLButtonElement>('button')];
  const button = (text: string) => buttons().find((b) => b.textContent?.trim() === text);
  const labelled = (label: string, nth = 1) => document.querySelectorAll<HTMLElement>(`[aria-label="${label}"]`)[nth - 1] ?? null;
  const panelText = (panel: string) => document.querySelector(`[data-tool-panel="${panel}"]`)?.textContent ?? '';
  const typeInto = (label: string, value: string) => {
    const input = document.querySelector<HTMLInputElement>(`input[aria-label="${label}"]`);
    if (!input) return false;
    Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value')?.set?.call(input, value);
    input.dispatchEvent(new Event('input', { bubbles: true }));
    return true;
  };
  /** Clicks and waits while a run goes: how long it took, or null if there was nothing to click. */
  const press = async (element: HTMLElement | null | undefined) => {
    if (!element || (element as HTMLButtonElement).disabled) return null;
    const started = performance.now();
    element.click();
    await wait(150);
    for (let k = 0; k < 300 && buttons().some((b) => b.disabled && (b.textContent?.trim().endsWith('…') ?? false)); k++) await wait(100);
    await wait(500);
    return Math.round(performance.now() - started);
  };
  const solid = async (id: string) => {
    const d = await objectDetails(id);
    return { volume: d.measures.kind === 'solid' ? d.measures.volume : NaN, revision: d.history[d.history.length - 1]?.revision ?? 0 };
  };
  let workflowId = '';
  const captureWorkflowId = () => workflowId;
  const E = 650000;
  const N = 7450000;

  log('workflow: a made-up pit between two surfaces, cut at RL 40');
  const flat = (name: string, rl: number) =>
    runCommand('surface.from_mesh', { name, layer: 'surfaces', vertices: [[E - 100, N - 100, rl], [E + 300, N - 100, rl], [E + 300, N + 300, rl], [E - 100, N + 300, rl]], triangles: [[0, 1, 2], [0, 2, 3]] });
  const top = (await flat('Top', 100)).record.outputs[0].id;
  const bottom = (await flat('Bottom', 0)).record.outputs[0].id;
  const outline = (await runCommand('string.create', { name: 'Outline', layer: 'outline', closed: true, points: [[E, N, 0], [E + 100, N, 0], [E + 100, N + 60, 0], [E, N + 60, 0]] })).record.outputs[0].id;
  const pit = (await runCommand('solid.between_surfaces', { top, bottom, string: outline, layer: 'pit' })).record.outputs[0].id;
  const cut = await runCommand('solid.cut_by_surface', { solids: [pit], rl: 40, keep: 'both', layer: 'cut' });
  await show(cut.summary);
  const pieces = cut.record.outputs.map((o) => o.id);
  const before = await Promise.all(pieces.map(solid));
  const volumes = before.map((p) => p.volume).sort((a, b) => a - b);
  expect('pieces are 240,000 and 360,000 m3', Math.abs(volumes[0] - 240000) < 1e-6 && Math.abs(volumes[1] - 360000) < 1e-6, volumes);

  try {
    // Capture from the Run log: the solid's run and the cut's, the last two, ticked by a click and a Shift-click.
    showPanel(api, 'runlog');
    await wait(600);
    const boxes = () => [...document.querySelectorAll<HTMLInputElement>('input[aria-label^="Capture run "]')];
    const numbers = boxes()
      .map((b) => Number(b.getAttribute('aria-label')?.slice('Capture run '.length)))
      .sort((a, b) => a - b);
    expect('every applied run has a tick box', numbers.length >= 5, numbers);
    const box = (n: number) => labelled(`Capture run ${n}`);
    const [solidRun, cutRun] = numbers.slice(-2);
    box(solidRun)?.click();
    await wait(200);
    box(cutRun)?.dispatchEvent(new MouseEvent('click', { bubbles: true, shiftKey: true }));
    await wait(200);
    expect('ticking says how many', document.body.textContent?.includes('2 runs ticked') ?? false);
    typeInto('Workflow name', 'Box pit');
    await wait(200);
    const captured = await press(button('Capture as a workflow'));
    expect('Capture pressed', captured !== null, captured);
    await wait(800);
    workflowId = (await projectSummary()).objects.find((o) => o.kind === 'workflow')?.id ?? '';
    const workflowPanel = panelText('workflow');
    expect('the Workflow panel opens on it with its two steps', workflowPanel.includes('Steps (2)'), workflowPanel.slice(0, 200));
    expect('the cut takes what step 1 made', workflowPanel.includes('solids: the objects step 1 made'), workflowPanel.slice(0, 400));
    expect("the solid's surfaces are found by name", workflowPanel.includes("top: the surface named 'Top'"));

    // Run it again: the same pieces, at their next revisions, to 1e-6 m³.
    const ran = await press(button('Run 2 steps'));
    expect('Run pressed', ran !== null, ran);
    const after = await Promise.all(pieces.map(solid));
    expect('the same pieces made again as their next revisions', after.every((p, k) => p.revision === before[k].revision + 1 && Math.abs(p.volume - before[k].volume) < 1e-6), after);
    expect('the panel says what ran', panelText('workflow').includes('2 steps run from step 1'), panelText('workflow').slice(-300));
    const runlog = document.body.textContent ?? '';
    expect('the run log has the workflow run', runlog.includes('Box pit: 2 steps run from step 1, finished'));

    // Tidy after a run (on by default): the pit, which only step 1 made, hidden; the pieces, the result, on show; the
    // surfaces and the outline, drawn before, left as they were.
    const shown = async () => Object.fromEntries((await projectSummary()).layers.map((l) => [l.name, l.visible]));
    let layers = await shown();
    expect('tidy: the result on show, the step in between hidden, the inputs as they were', layers.cut === true && layers.pit === false && layers.surfaces === true && layers.outline === true, layers);

    // A click on step 1 selects what it made, the pit, and zooms to it.
    await press(labelled('Show step 1'));
    expect('a click on a step selects what it made', JSON.stringify(selection()) === JSON.stringify([pit]), selection());

    // Its eye shows only that: the pit's layer on, every other layer off; again puts them back.
    await press(document.querySelector<HTMLElement>('[aria-label^="Only step 1"]'));
    layers = await shown();
    const on = Object.entries(layers).filter(([, v]) => v).map(([k]) => k);
    expect('only step 1: its layer alone on show', JSON.stringify(on) === JSON.stringify(['pit']), on);
    expect('the panel says so', panelText('workflow').includes('Showing only step 1'));
    await press(button('Show all again'));
    layers = await shown();
    expect('show all again puts the layers back', layers.cut === true && layers.pit === false && layers.surfaces === true && layers.outline === true, layers);

    // Add to workflow: while adding, the Cut solids panel's button adds a step instead of running.
    await press(labelled('Add steps from the tool panels: their button reads Add to workflow'));
    showPanel(api, 'cut');
    await wait(600);
    expect("the Cut solids panel's button reads Add to Box pit", buttons().some((b) => b.textContent?.trim() === 'Add to Box pit'));
    showPanel(api, 'workflow');
    await wait(400);
    await press(labelled('Stop adding from the tool panels'));
    showPanel(api, 'cut');
    await wait(400);
    expect('and back to its own once adding stops', !buttons().some((b) => b.textContent?.trim() === 'Add to Box pit'));

    // A pause at the end: the run stops there.
    showPanel(api, 'workflow');
    await wait(400);
    await press(labelled('Add a pause at the end'));
    await wait(600);
    expect('the pause is the third step', panelText('workflow').includes('Steps (3)'));
    await press(button('Run 2 steps'));
    expect('the run stops at the pause', panelText('workflow').includes('Paused at step 3'), panelText('workflow').slice(-300));

    // One undo takes the run back.
    const revisions = (await Promise.all(pieces.map(solid))).map((p) => p.revision);
    await show(await undo());
    const undone = (await Promise.all(pieces.map(solid))).map((p) => p.revision);
    expect('one undo takes the run back', undone.every((r, k) => r === revisions[k] - 1), { revisions, undone });
    await show(await projectSummary());

    // Cases (decision 0066): the pause taken out, a case "60" added and its cut set to RL 60 under the step, then
    // Run all cases: Base's pieces again on 'cut', the case's on 'cut 60', 360,000 below RL 60 and 240,000 above, tagged
    // with its case; one undo takes both back.
    await press(labelled('Take this step out', 3));
    expect('the pause taken out', panelText('workflow').includes('Steps (2)'));
    typeInto('New case name', '60');
    await wait(200);
    await press(button('Add'));
    const caseList = document.querySelector<HTMLSelectElement>('select[aria-label="Case"]');
    expect('the new case is picked', caseList?.value === '60', caseList?.value);
    labelled('Settings of step 2')?.click();
    await wait(200);
    const cell = document.querySelector<HTMLInputElement>('input[aria-label="60: step 2 rl"]');
    expect("step 2's settings open under it as the case has them, its box reading as Base", cell !== null && cell.value === '' && cell.placeholder === '40', cell?.placeholder);
    if (cell) {
      cell.focus();
      Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value')?.set?.call(cell, '60');
      cell.dispatchEvent(new Event('input', { bubbles: true }));
      // Leaving the cell, as a Tab or a click elsewhere does. Sent as the event itself: with the window not in front
      // (the screen locked, another window active), focus() and blur() don't move focus, and no blur comes.
      cell.dispatchEvent(new FocusEvent('focusout', { bubbles: true }));
      await wait(800);
    }
    const saved = await objectDetails(captureWorkflowId());
    const settings = saved.measures.kind === 'workflow' ? saved.measures.workflow.cases?.[0]?.settings : undefined;
    expect('the box saved as what the case changes', JSON.stringify(settings) === JSON.stringify({ 2: { rl: 60 } }), settings);
    const beforeCases = await projectSummary();
    const casesRan = await press(buttons().find((b) => b.textContent?.trim().startsWith('Run all 2 cases')));
    expect('Run all cases pressed', casesRan !== null, casesRan);
    const now = await projectSummary();
    const layerId = (name: string) => now.layers.find((l) => l.name === name)?.id;
    const solidsOn = (name: string) => now.objects.filter((o) => o.layer === layerId(name) && o.kind === 'solid');
    const casePieces = await Promise.all(solidsOn('cut 60').map((o) => objectDetails(o.id)));
    const volumes60 = casePieces.map((d) => (d.measures.kind === 'solid' ? d.measures.volume : NaN)).sort((a, b) => a - b);
    expect("the case's pieces on 'cut 60': 240,000 and 360,000 m3", volumes60.length === 2 && Math.abs(volumes60[0] - 240000) < 1e-6 && Math.abs(volumes60[1] - 360000) < 1e-6, volumes60);
    const below = casePieces.find((d) => d.name.includes('below'));
    expect('below RL 60 is 360,000 m3', below?.measures.kind === 'solid' && Math.abs(below.measures.volume - 360000) < 1e-6, below?.name);
    expect('tagged with its case', casePieces.every((d) => d.attributes.case === '60'), casePieces.map((d) => d.attributes.case));
    const basePieces = await Promise.all(solidsOn('cut').map((o) => objectDetails(o.id)));
    expect("Base's pieces still on 'cut', tagged Base", basePieces.length === 2 && basePieces.every((d) => d.attributes.case === 'Base'), basePieces.map((d) => d.attributes.case));
    await show(await undo());
    const undoneAll = await projectSummary();
    expect('one undo takes both cases back', !undoneAll.objects.some((o) => o.layer === layerId('cut 60')) && undoneAll.objects.length === beforeCases.objects.length, { before: beforeCases.objects.length, undoneAll: undoneAll.objects.length });

    // The report (decision 0068): the bench's project isn't saved, so the Report section says to save first, its button off.
    showPanel(api, 'workflow');
    await wait(300);
    expect('the Report section asks for the project to be saved first', panelText('workflow').includes('Save the project first: reports go in a folder beside it'));
    expect('and its button is off till then', buttons().find((b) => b.textContent?.trim() === 'Write report')?.disabled === true);
  } catch (e) {
    expect('ran without an error', false, String(e));
  }
  results.failures = failures;
  return results;
}
