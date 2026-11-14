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

export async function checkWorkflow(_viewport: Viewport, log: (line: string) => void, show: Show, dock: () => DockviewApi | null) {
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
  const labelled = (label: string) => document.querySelector<HTMLElement>(`[aria-label="${label}"]`);
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
  } catch (e) {
    expect('ran without an error', false, String(e));
  }
  results.failures = failures;
  return results;
}
