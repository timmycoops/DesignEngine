// Unattended Phase 0 run (app started with --bench): loads the surfaces, then times
// Check 1 (orbit at increasing scene sizes), Check 3 (binary transfer) and Check 4 (drag with
// snapping), writes bench/out/app-results.json and exits. With --solids <zip or folder> it times that
// import through the app instead, panels showing, and writes bench/out/app-solids-results.json.
import * as THREE from 'three/webgpu';
import {
  type BenchConfig,
  buildSnapIndex,
  defaultTransport,
  exitApp,
  objectDetails,
  prepareSynthetic,
  projectSummary,
  type ProjectSummary,
  runCommand,
  runLog,
  setLayerVisible,
  setObjectsVisible,
  type SnapHit,
  type Vec3,
  snapRay,
  stageObject,
  type Transport,
  undo,
  writeResults,
} from '../bridge';
import { loadStaged } from '../viewport/load';
import { takePalettePictures, takePictures } from './pictures';
import { checkSelect } from './selectCheck';
import { checkShell } from './shellCheck';
import type { DockviewApi } from 'dockview-react';
import { PolylineTool } from '../viewport/PolylineTool';
import type { SectionView } from '../viewport/section';
import type { StringLayer } from '../viewport/strings';
import { FIELD_OF_VIEW_DEFAULT, type Viewport } from '../viewport/Viewport';

interface FrameSummary {
  frames: number;
  mean_fps: number;
  p50_ms: number;
  p95_ms: number;
  p99_ms: number;
  max_ms: number;
  frames_over_33ms: number;
}

const WARM_UP_FRAMES = 30;
/** Scroll positions the tree is drawn at, end to end. */
const TREE_SCROLL_STEPS = 120;

/** Shows a summary as the app does: panels, strings, geometry and the snap index. */
type Show = (summary: ProjectSummary) => Promise<void> | undefined;

export async function runChecks(
  viewport: Viewport,
  config: BenchConfig,
  log: (line: string) => void,
  show: Show,
  section: () => SectionView | null,
  strings: () => StringLayer | null,
  selection: () => string[],
  dock: () => DockviewApi | null,
): Promise<void> {
  const tool = new PolylineTool(viewport);
  const canvas = viewport.renderer.domElement;
  const results: Record<string, unknown> = {
    generated: new Date().toISOString(),
    backend: viewport.backend,
    adapter: viewport.adapter,
    canvas: { width: canvas.width, height: canvas.height, device_pixel_ratio: window.devicePixelRatio },
    user_agent: navigator.userAgent,
    default_transport: defaultTransport,
    // Every run at the default, whatever is saved on this machine, so runs compare.
    field_of_view_deg: FIELD_OF_VIEW_DEFAULT,
  };
  viewport.setFieldOfView(FIELD_OF_VIEW_DEFAULT, false);

  try {
    if (config.pictures) {
      const take = config.palettes ? takePalettePictures : takePictures;
      results.pictures = await take(viewport, config.surfaces, config.strings, log, show);
    } else if (config.design_lines) {
      results.design_lines = await checkDesignLines(viewport, log, show, strings);
    } else if (config.builder) {
      results.builder = await checkBuilder(viewport, log, show, strings);
    } else if (config.cut) {
      results.cut = await checkCut(viewport, log, show, strings);
    } else if (config.select) {
      results.select = await checkSelect(viewport, log, show, selection);
    } else if (config.shell) {
      results.shell = await checkShell(viewport, log, dock);
    } else if (config.solids) {
      results.solids = await checkSolids(viewport, config, config.solids, log, show, section, strings);
    } else {
      results.load = await loadSurfaces(viewport, config.surfaces, log, config.opacity);
      results.opacity = config.opacity ?? 1;
      viewport.fitView();
      results.check1 = await checkViewport(viewport, config, log);
      results.check3 = await checkTransfer(viewport, config.transfer_cells, log);
      results.check4 = await checkDrag(viewport, tool, config.drag_updates, log);
    }
  } catch (e) {
    results.error = String(e);
    log(`error: ${String(e)}`);
  }

  const path = await writeResults(JSON.stringify(results, null, 2));
  log(`results written to ${path}`);
  await exitApp();
}

/**
 * `--builder`: the Builder panel (decision 0048) as Tim would use it, through its own buttons. A toe drawn by hand at RL 100
 * picked; a wall to RL 115, a berm and a wall to RL 125 built from the panel, each new line picked as it's made. Step 1
 * clicked in the list picks its crest, lights up its row and frames it. The berm's line edited (a vertex moved 3 m out, as a
 * drag does): its step says edited, and the step above it says it was built before the edit. That step taken out with its
 * bin, the berm's line picked, and the wall built again: it stands on the edited line and says so.
 */
async function checkBuilder(viewport: Viewport, log: (line: string) => void, show: Show, stringLayer: () => StringLayer | null) {
  const layer = stringLayer();
  if (!layer) return { error: 'no string layer' };
  // By the clock, not by frames: with the panel in front of the 3D view no frames may be drawn.
  const wait = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));
  const buttons = () => [...document.querySelectorAll('button')];
  const button = (text: string) => buttons().find((b) => b.textContent?.trim() === text);
  const named = async (name: string) => (await projectSummary()).objects.find((o) => o.name === name);
  const selectedName = async () => (layer.selected ? ((await projectSummary()).objects.find((o) => o.id === layer.selected)?.name ?? layer.selected) : 'nothing');
  const typeInto = (label: string, value: string) => {
    const input = document.querySelector<HTMLInputElement>(`input[aria-label="${label}"]`);
    if (!input) return false;
    Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value')?.set?.call(input, value);
    input.dispatchEvent(new Event('input', { bubbles: true }));
    return true;
  };
  const panelText = () => document.querySelector('[data-tool-panel="builder"]')?.textContent ?? '';
  const after = (text: string, from: string, length: number) => {
    const at = text.indexOf(from);
    return at < 0 ? '' : text.slice(at, at + length);
  };
  log('builder: a toe at RL 100');
  const toe = await runCommand('string.create', { name: 'Toe B', layer: 'toes', closed: false, points: [[650000, 7451000, 100], [650300, 7451040, 100], [650600, 7451000, 100]] });
  await show(toe.summary);
  const open = button('Builder');
  open?.click();
  await wait(500);
  layer.select(toe.record.outputs[0].id);
  await wait(500);
  const startsFromToe = panelText().includes('FromToe B') && panelText().includes('Starting a build from this line');
  const typedRl = typeInto('What the wall goes up to: RL', '115');
  await wait(200);
  const steps: { button: string; ms: number; picked: string; said: string }[] = [];
  for (const label of ['Build wall', 'Build berm', 'Build wall']) {
    const started = performance.now();
    const go = button(label);
    if (!go) return { error: `no ${label} button`, steps, panel: panelText().slice(0, 400) };
    go.click();
    await wait(100);
    for (let k = 0; k < 40 && button('Building…'); k++) await wait(100);
    await wait(500);
    steps.push({ button: label, ms: Math.round(performance.now() - started), picked: await selectedName(), said: after(panelText(), 'Toe B step', 110) });
  }
  const rows = () => buttons().filter((b) => b.getAttribute('aria-label')?.startsWith('Step '));
  const rowCount = rows().length;
  // Step 1 clicked in the list: its crest picked, its row lit, the view on it.
  viewport.fitView();
  await wait(200);
  rows()[0]?.click();
  await wait(1200);
  const pickedFromList = await selectedName();
  const litRow = rows()[0]?.parentElement?.className.includes('bg-selection') ?? false;
  const crest1 = await named('Toe B crest 1');
  const xs = (crest1?.vertices_local ?? []).map((p) => p[0]);
  const ys = (crest1?.vertices_local ?? []).map((p) => p[1]);
  const centre = [(Math.min(...xs) + Math.max(...xs)) / 2, (Math.min(...ys) + Math.max(...ys)) / 2];
  const target = viewport.controls.target;
  const framedOff = Math.hypot(target.x - centre[0], target.y - centre[1]);
  // The berm's line edited, its last vertex 3 m further out, as a drag does.
  const toe2 = await named('Toe B toe 2');
  if (!toe2) return { error: 'no Toe B toe 2', steps };
  const details = await objectDetails(toe2.id);
  const points = details.measures.kind === 'string' ? details.measures.points : [];
  const last = points[points.length - 1];
  await show((await runCommand('string.move_vertex', { string: toe2.id, index: points.length - 1, to: [last[0], last[1] + 3, last[2]] })).summary);
  await wait(500);
  const rowText = (n: number) => rows()[n]?.textContent ?? '';
  const editedShown = rowText(1).includes('edited');
  const builtBeforeShown = rowText(2).includes('built before an edit');
  // Rebuild from here: the berm's line picked, its one button now Rebuild, step 3 built again on the edited line.
  const crest3Before = (await named('Toe B crest 3'))?.vertices_local ?? [];
  layer.select(toe2.id);
  await wait(500);
  const rebuildButton = button('Rebuild 1 step from here');
  const rebuildSays = after(panelText(), 'Step 3 was built before', 60);
  rebuildButton?.click();
  await wait(100);
  for (let k = 0; k < 40 && button('Rebuilding…'); k++) await wait(100);
  await wait(500);
  const rebuilt = {
    button: !!rebuildButton,
    said_before: rebuildSays,
    report: after(panelText(), '1 step built again', 80),
    stands_on_your_line: panelText().includes("isn't where the berm from Toe B crest 1 puts it now"),
    step_3_still_stale: rowText(2).includes('built before an edit'),
    crest_3_end_moved_m: 0,
  };
  const crest3After = (await named('Toe B crest 3'))?.vertices_local ?? [];
  const lastOf = (points: Vec3[]) => points[points.length - 1] ?? [0, 0, 0];
  rebuilt.crest_3_end_moved_m = Math.round(Math.hypot(lastOf(crest3After)[0] - lastOf(crest3Before)[0], lastOf(crest3After)[1] - lastOf(crest3Before)[1]) * 1000) / 1000;
  // Step 3 taken out with its bin, the berm's line picked, the wall built again on it.
  buttons().find((b) => b.getAttribute('aria-label') === 'Take out step 3')?.click();
  await wait(800);
  const rowsAfterTakingOut = rows().length;
  layer.select(toe2.id);
  await wait(500);
  button('Build wall')?.click();
  await wait(1500);
  const standsOnYourLine = panelText().includes("isn't where the berm from Toe B crest 1 puts it now");
  // A surface at RL 140 over the strip, picked in the panel's To list: a berm and a wall up to it.
  const surface = await runCommand('surface.from_mesh', {
    name: 'Bench surface',
    layer: 'surfaces',
    vertices: [[649800, 7450800, 140], [650800, 7450800, 140], [650800, 7451400, 140], [649800, 7451400, 140]],
    triangles: [[0, 1, 2], [0, 2, 3]],
  });
  await show(surface.summary);
  const crest3 = await named('Toe B crest 3');
  if (crest3) layer.select(crest3.id);
  await wait(500);
  button('Build berm')?.click();
  await wait(1500);
  const list = document.querySelector<HTMLSelectElement>('select[aria-label="What the wall goes up to"]');
  const listed = list ? [...list.querySelectorAll('optgroup')].map((g) => `${g.label}: ${[...g.querySelectorAll('option')].map((o) => o.textContent).join(', ')}`) : [];
  if (list) {
    Object.getOwnPropertyDescriptor(HTMLSelectElement.prototype, 'value')?.set?.call(list, surface.record.outputs[0].id);
    list.dispatchEvent(new Event('change', { bubbles: true }));
  }
  await wait(300);
  // Ctrl+Enter in the panel presses its button.
  list?.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', ctrlKey: true, bubbles: true }));
  await wait(1500);
  const crest5 = await named('Toe B crest 5');
  const origin = (await projectSummary()).origin ?? [0, 0, 0];
  const crest5Rls = [...new Set((crest5?.vertices_local ?? []).map((p) => Math.round((p[2] + origin[2]) * 1000) / 1000))];
  // Join into one face: the build's sheets as one face, by its name (decision 0049).
  button('Join into one face')?.click();
  await wait(1500);
  const joinedFace = await named('Toe B face');
  const result = {
    panel_opened_from_toolbar: !!open,
    starts_from_toe: startsFromToe,
    typed_rl: typedRl,
    steps,
    rows_listed: rowCount,
    picked_from_list: pickedFromList,
    row_lit: litRow,
    crest_1_framed_off_m: Math.round(framedOff * 1000) / 1000,
    edited_shown_on_step_2: editedShown,
    built_before_an_edit_shown_on_step_3: builtBeforeShown,
    rebuilt,
    rows_after_taking_out_step_3: rowsAfterTakingOut,
    wall_again_stands_on_your_line: standsOnYourLine,
    crest_3_again: !!crest3,
    to_list_groups: listed,
    wall_to_the_surface_said: after(panelText(), 'Toe B step 5', 110),
    crest_5_rls: crest5Rls,
    joined_into_one_face: !!joinedFace,
    join_said: after(panelText(), 'Toe B: ', 90),
  };
  log(`builder: ${JSON.stringify(result)}`);
  return result;
}

/**
 * `--cut`: the Cut solids panel (decision 0049) as Tim would use it, through its own buttons. Fifteen toes 50 m apart, numbered
 * by a line across and built up to RL 150 and down to RL -50 at 45°; the panel opened from the toolbar; the pit made between
 * a topography at RL 100 and a floor at RL 0 inside its limit; picked and sliced into strips by the faces' layer; the faces
 * joined a face a strip; and the pit cut at RL 50, keeping both sides. Every volume is known: strip 1 and what's past strip
 * 15 7,500,000 m³ each, every strip between 2,500,000 m³, the pit 50,000,000 m³.
 */
async function checkCut(viewport: Viewport, log: (line: string) => void, show: Show, stringLayer: () => StringLayer | null) {
  const layer = stringLayer();
  if (!layer) return { error: 'no string layer' };
  const wait = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));
  const buttons = () => [...document.querySelectorAll('button')];
  const button = (text: string) => buttons().find((b) => b.textContent?.trim() === text);
  const panelText = () => document.querySelector('[data-tool-panel="cut"]')?.textContent ?? '';
  const choose = (label: string, value: string) => {
    const list = document.querySelector<HTMLSelectElement>(`select[aria-label="${label}"]`);
    if (!list) return false;
    Object.getOwnPropertyDescriptor(HTMLSelectElement.prototype, 'value')?.set?.call(list, value);
    list.dispatchEvent(new Event('change', { bubbles: true }));
    return true;
  };
  const typeInto = (label: string, value: string) => {
    const input = document.querySelector<HTMLInputElement>(`input[aria-label="${label}"]`);
    if (!input) return false;
    Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value')?.set?.call(input, value);
    input.dispatchEvent(new Event('input', { bubbles: true }));
    return true;
  };
  /** Presses the panel's button and waits while it works: how long it took, or null if there was no such button. */
  const press = async (label: string) => {
    const go = button(label);
    if (!go || go.disabled) return null;
    const started = performance.now();
    go.click();
    await wait(100);
    for (let k = 0; k < 300 && buttons().some((b) => b.disabled && (b.textContent?.trim().endsWith('…') ?? false)); k++) await wait(100);
    await wait(400);
    return Math.round(performance.now() - started);
  };
  const volumesOn = async (name: string) => {
    const summary = await projectSummary();
    const id = summary.layers.find((l) => l.name === name)?.id;
    const volumes: Record<string, number> = {};
    for (const o of summary.objects.filter((o) => o.layer === id && o.kind === 'solid')) {
      const d = await objectDetails(o.id);
      if (d.measures.kind === 'solid') volumes[o.name] = Math.round(d.measures.volume * 1000) / 1000;
    }
    return volumes;
  };
  const resultLines = () => [...document.querySelectorAll('[data-tool-panel="cut"] [role="status"] div')].map((d) => d.textContent ?? '');
  const E = 650000;
  const N = 7450000;

  log('cut: a topography, a floor, a pit limit and fifteen numbered faces');
  const flat = (name: string, rl: number) =>
    runCommand('surface.from_mesh', { name, layer: 'surfaces', vertices: [[E - 100, N - 100, rl], [E + 1100, N - 100, rl], [E + 1100, N + 600, rl], [E - 100, N + 600, rl]], triangles: [[0, 1, 2], [0, 2, 3]] });
  const topography = (await flat('Topography', 100)).record.outputs[0].id;
  const floor = (await flat('Floor', 0)).record.outputs[0].id;
  await runCommand('string.create', { name: 'Pit limit', layer: 'limits', closed: true, points: [[E, N, 0], [E + 1000, N, 0], [E + 1000, N + 500, 0], [E, N + 500, 0]] });
  const toe = (await runCommand('string.create', { name: 'Toe', layer: 'toes', closed: false, points: [[E + 100, N - 50, 0], [E + 100, N + 550, 0]] })).record.outputs[0].id;
  const offsets = (await runCommand('string.offset', { string: toe, distance: 50, side: 'right', times: 14, layer: 'toes' })).record.outputs.map((o) => o.id);
  await runCommand('string.number_across', { strings: [toe, ...offsets], from: [E, N + 250, 0], to: [E + 1000, N + 250, 0], start: 1 });
  const content = {
    layer: 'faces',
    lines: [toe, ...offsets].flatMap((string) => ['Up', 'Down'].map((profile) => ({ string, profile, side: 'right', run_on: [60, 60] }))),
    profiles: [
      { name: 'Up', direction: 'up', register: { kind: 'rl', rl: 0 }, ramps: [], steps: [{ step: 'project', to: { kind: 'rl', rl: 150 }, batter: 45 }] },
      { name: 'Down', direction: 'down', register: { kind: 'rl', rl: 0 }, ramps: [], steps: [{ step: 'project', to: { kind: 'rl', rl: -50 }, batter: 45 }] },
    ],
    rules: [],
  };
  const design = (await runCommand('face.save_design', { name: 'Strips', content })).record.outputs[0].id;
  await show((await runCommand('face.project', { design })).summary);
  viewport.fitView();

  const open = button('Cut');
  open?.click();
  await wait(600);
  // The pit, between the two surfaces inside its limit: the only closed string, so the panel offers it first.
  button('Between surfaces')?.click();
  await wait(300);
  const chose = choose('The surface or grid on top', topography) && choose('The surface or grid underneath', floor);
  await wait(300);
  const insideOffered = panelText().includes('Pit limit');
  const madeMs = await press('Make the solid');
  const between = { chose, inside_offered: insideOffered, ms: madeMs, said: resultLines().slice(0, 2), volumes: await volumesOn('pit') };
  const pit = (await projectSummary()).objects.find((o) => o.name === 'PIT');
  if (!pit) return { error: 'no PIT made', between, panel: panelText().slice(0, 400) };

  // Into strips: the pit the panel just made is picked, the faces' layer offered first, sliced in one run.
  button('Into strips')?.click();
  await wait(500);
  const pickedAfterMaking = panelText();
  const facesSaid = panelText().match(/\d+ faces on \d+ strips: [0-9–]+/)?.[0] ?? '';
  const sliceLabel = buttons().find((b) => b.textContent?.trim().startsWith('Slice '))?.textContent?.trim() ?? '';
  const slicedMs = await press(sliceLabel);
  const strips = await volumesOn('strips');
  const stripVolumes = Object.values(strips);
  const sliced = {
    faces_said: facesSaid,
    button: sliceLabel,
    ms: slicedMs,
    said: resultLines().filter((l) => l.includes('sliced by') || l.includes('adding up') || l.includes('from the pit out')),
    strips: Object.keys(strips).length,
    strip_1: strips['PIT strip 1'],
    strip_8: strips['PIT strip 8'],
    past_strip_15: strips['PIT past strip 15'],
    total: Math.round(stripVolumes.reduce((a, b) => a + b, 0) * 1000) / 1000,
  };

  // Joined: one face a strip on a layer of its own.
  const joinedMs = await press('Join a face a strip');
  const joinedSummary = await projectSummary();
  const joinedLayer = joinedSummary.layers.find((l) => l.name === 'faces joined')?.id;
  const joined = { ms: joinedMs, faces: joinedSummary.objects.filter((o) => o.layer === joinedLayer).length, said: resultLines().slice(-1) };

  // By a surface: the pit, still picked, cut at RL 50, both sides kept.
  button('By a surface')?.click();
  await wait(300);
  choose('Surface, grid or RL to cut by', 'rl');
  await wait(200);
  const typedRl = typeInto('Surface, grid or RL to cut by: RL', '50');
  await wait(300);
  const cutMs = await press('Cut 1 solid');
  const parts = await volumesOn('cut');

  // Properties no longer cuts: it points to the panel.
  [...document.querySelectorAll('.dv-tab')].find((t) => t.textContent?.trim() === 'Properties')?.dispatchEvent(new MouseEvent('pointerdown', { bubbles: true }));
  [...document.querySelectorAll('.dv-tab')].find((t) => t.textContent?.trim() === 'Properties')?.dispatchEvent(new MouseEvent('click', { bubbles: true }));
  await wait(500);
  const properties = document.body.textContent?.includes('Cut it, or slice it into strips, in') ?? false;
  const result = { panel_opened_from_toolbar: !!open, pit_picked_after_making: pickedAfterMaking.includes('Picked (1)'), between, sliced, joined, cut: { typed_rl: typedRl, ms: cutMs, parts }, properties_points_to_the_panel: properties };
  log(`cut: ${JSON.stringify(result)}`);
  return result;
}

/**
 * `--design-lines`: Edit from here's copy lying on a step design's own line (Tim, 2026-11-06: his Deletes went to the
 * design's line under his copy, and the next run made it again). Through the real canvas and keys: a click on the two
 * picks the copy, Delete or a drag on the design's line is refused with what to do, and Delete on the copy takes the
 * vertex out.
 */
async function checkDesignLines(viewport: Viewport, log: (line: string) => void, show: Show, stringLayer: () => StringLayer | null) {
  const layer = stringLayer();
  if (!layer) return { error: 'no string layer' };
  const canvas = viewport.renderer.domElement;
  const settle = async () => {
    for (let i = 0; i < 8; i++) await viewport.nextFrame();
  };
  const send = (type: string, x: number, y: number, buttons: number) =>
    canvas.dispatchEvent(new PointerEvent(type, { bubbles: true, button: 0, buttons, clientX: x, clientY: y, pointerId: 1, pointerType: 'mouse' }));
  const click = async (x: number, y: number) => {
    send('pointermove', x, y, 0);
    await viewport.nextFrame();
    send('pointerdown', x, y, 1);
    send('pointerup', x, y, 0);
    await settle();
  };
  const press = async (key: string) => {
    window.dispatchEvent(new KeyboardEvent('keydown', { key, cancelable: true }));
    await settle();
    await settle();
  };
  const pointsOf = async (id: string) => {
    const d = await objectDetails(id);
    return d.measures.kind === 'string' ? d.measures.points : [];
  };
  // A bent toe on RL 100, a wall to RL 115 and a 10 m berm: the design's next toe, and a copy of it on top.
  log('design lines: a step design and a copy of its next toe');
  const toe = await runCommand('string.create', { name: 'Toe', layer: 'toes', closed: false, points: [[650000, 7450000, 100], [650300, 7450040, 100], [650600, 7450000, 100], [650900, 7450030, 100]] });
  const content = {
    layer: 'faces',
    lines: [{ string: toe.record.outputs[0].id, profile: 'Steps', side: 'left', run_on: [0, 0] }],
    profiles: [{
      name: 'Steps', direction: 'up', condition: 0.1, register: { kind: 'rl', rl: 100 }, ramps: [],
      steps: [
        { step: 'project', to: { kind: 'rl', rl: 115 }, batter: 70, condition: 0.1 },
        { step: 'berm', width: 10, condition: 0.1, min_bench: 1, on: null, register: true },
      ],
    }],
    rules: [],
  };
  const saved = await runCommand('face.save_design', { name: 'Steps from Toe', content });
  const ran = await runCommand('face.step', { design: saved.record.outputs[0].id, name: 'Steps from Toe', content });
  const made = ran.summary.objects.find((o) => o.kind === 'string' && o.name.endsWith(' next toe') && typeof o.attributes.face_design === 'string');
  if (!made) return { error: 'the design made no next toe' };
  const points = await pointsOf(made.id);
  const copy = await runCommand('string.create', { name: 'Toe Steps your line 2', layer: 'edited lines', closed: false, points });
  const yours = copy.record.outputs[0].id;
  // As the panel does: your line on row 2, and the step run again, which makes the design's lines afresh, after yours.
  const edited = { ...content, lines: [{ ...content.lines[0], edits: [{ row: 2, string: yours }] }] };
  const again = await runCommand('face.step', { design: saved.record.outputs[0].id, name: 'Steps from Toe', content: edited });
  const next = again.summary.objects.find((o) => o.kind === 'string' && o.name.endsWith(' next toe') && typeof o.attributes.face_design === 'string');
  if (!next) return { error: 'the design made no next toe with your line on' };
  const nextPoints = await pointsOf(next.id);
  const onTop = nextPoints.length === points.length && nextPoints.every((p, i) => p.every((c, k) => Math.abs(c - points[i][k]) < 1e-6));
  if (!onTop) return { error: "the design's next toe isn't where your line is" };
  await show(again.summary);
  layer.select(null);
  viewport.fitView();
  await settle();
  const origin = copy.summary.origin ?? [0, 0, 0];
  const onScreen = (p: number[]) => viewport.projectToClient(new THREE.Vector3(p[0] - origin[0], p[1] - origin[1], p[2] - origin[2]));
  const middle = (i: number) => onScreen(points[i].map((c, k) => (c + points[i + 1][k]) / 2));
  const status = () => document.body.innerText;
  const refusal = 'made again each time it runs';
  const corner = onScreen(points[1]);

  // Nothing selected: a click on the two picks your copy.
  await click(...middle(0));
  const pickedWithNothingSelected = layer.selected === yours ? 'your line' : layer.selected === next.id ? "the design's line" : String(layer.selected);
  // The design's line selected (from the tree, say), then clicked on: it stays selected, and says it's the design's.
  layer.select(next.id);
  await click(...middle(1));
  const designStaysSelected = layer.selected === next.id;
  const saysWhatItIs = status().includes(refusal);
  // Delete on one of its vertices: refused, nothing changes, and it says which row's ✎ to use.
  const designBefore = points.length;
  send('pointermove', corner[0], corner[1], 0);
  await settle();
  await press('Delete');
  const designAfter = (await pointsOf(next.id)).length;
  const deleteRefusedSaysRow = status().includes(refusal) && status().includes('✎ on row 2');
  // A drag on its handle doesn't start.
  send('pointerdown', corner[0], corner[1], 1);
  send('pointermove', corner[0] + 40, corner[1] + 30, 1);
  await settle();
  send('pointerup', corner[0] + 40, corner[1] + 30, 0);
  await settle();
  await settle();
  const afterDrag = await pointsOf(next.id);
  const notMoved = afterDrag.length === points.length && afterDrag[1].every((c, k) => c === points[1][k]);
  // Your copy selected by a click on it, then Delete on the same corner: the vertex goes.
  layer.select(null);
  await click(...middle(1));
  const yoursSelected = layer.selected === yours;
  send('pointermove', corner[0], corner[1], 0);
  await settle();
  await press('Delete');
  const yoursAfter = (await pointsOf(yours)).length;
  const result = {
    picked_with_nothing_selected: pickedWithNothingSelected,
    design_line_stays_selected_when_clicked: designStaysSelected,
    selecting_the_design_line_says_so: saysWhatItIs,
    design_line_vertices_before_after_delete: [designBefore, designAfter],
    delete_on_design_line_refused_naming_row: deleteRefusedSaysRow,
    design_line_vertex_not_moved_by_drag: notMoved,
    your_line_selected_by_click: yoursSelected,
    your_line_vertices_before_after_delete: [points.length, yoursAfter],
    panel: await checkEditFromHere(viewport, log, show, layer),
    join: await checkJoinSwitch(log, show),
  };
  log(`design lines: ${JSON.stringify(result)}`);
  return result;
}

/**
 * The Face design panel's ✎, as Tim uses it (2026-11-06: "when I click edit it should straight away make that line the
 * one that's selected so you go to that line"). A step design opened in the panel; ✎ on its berm row selects the line
 * it makes and frames it; the toe moved runs the step again; the row taken out takes its line with it.
 */
async function checkEditFromHere(viewport: Viewport, log: (line: string) => void, show: Show, layer: StringLayer) {
  // By the clock, not by frames: with the panel in front of the 3D view no frames are drawn.
  const wait = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));
  const buttons = () => [...document.querySelectorAll('button')];
  log('design lines: the Face design panel');
  const toe = await runCommand('string.create', { name: 'Toe 2', layer: 'toes', closed: false, points: [[650000, 7449000, 100], [650300, 7449040, 100], [650600, 7449000, 100]] });
  const toeId = toe.record.outputs[0].id;
  const content = {
    layer: 'faces',
    lines: [{ string: toeId, profile: 'Steps', side: 'left', run_on: [0, 0] }],
    profiles: [{
      name: 'Steps', direction: 'up', condition: 0.1, register: { kind: 'rl', rl: 100 }, ramps: [],
      steps: [
        { step: 'project', to: { kind: 'rl', rl: 115 }, batter: 70, condition: 0.1 },
        { step: 'berm', width: 10, condition: 0.1, min_bench: 1, on: null, register: true },
      ],
    }],
    rules: [],
  };
  const saved = await runCommand('face.save_design', { name: 'Steps from Toe 2', content });
  const design = saved.record.outputs[0].id;
  void show((await runCommand('face.step', { design, name: 'Steps from Toe 2', content })).summary);
  const face = buttons().find((b) => b.textContent?.trim() === 'Face');
  log(`design lines: Face button ${face ? 'found' : 'missing'}`);
  face?.click();
  await wait(500);
  const open = document.querySelector<HTMLSelectElement>('select[aria-label="Open a design"]');
  if (!open) return { error: 'no Open a design list in the Face design panel' };
  Object.getOwnPropertyDescriptor(HTMLSelectElement.prototype, 'value')?.set?.call(open, design);
  open.dispatchEvent(new Event('change', { bubbles: true }));
  await wait(2500);

  // ✎ on row 2: your line made, selected and framed.
  viewport.fitView();
  await wait(200);
  const pencil = buttons().find((b) => b.title.startsWith('Edit from here: copy') && b.title.includes('the next toe'));
  if (!pencil) {
    const titles = buttons().map((b) => b.title).filter((t) => /Edit|row|line/.test(t)).slice(0, 12);
    const options = [...open.options].map((o) => `${o.value}:${o.textContent}`);
    let host: HTMLElement | null = open;
    for (let i = 0; i < 6 && host?.parentElement; i++) host = host.parentElement;
    const panelText = host?.innerText.slice(0, 600);
    const selects = [...document.querySelectorAll('select')].map((s) => `${s.getAttribute('aria-label')}=${s.value}`);
    return { error: 'no ✎ on row 2', design, titles, options, panel_text: panelText ?? null, selects, still_there: document.contains(open) };
  }
  const tip = pencil.title;
  pencil.click();
  const selectedOverTime: string[] = [];
  let selectedAfterASecond: string | null = null;
  for (const ms of [100, 400, 1000]) {
    await wait(ms === 100 ? 100 : ms - (ms === 400 ? 100 : 400));
    const id = layer.selected;
    selectedOverTime.push(`${ms} ms: ${id ? ((await projectSummary()).objects.find((o) => o.id === id)?.name ?? id) : 'nothing'}`);
    selectedAfterASecond = id;
  }
  await wait(500);
  const after = await projectSummary();
  const mine = after.objects.find((o) => o.name === 'Toe 2 Steps your line 2');
  if (!mine?.vertices_local) return { error: '✎ made no line', tip };
  const xs = mine.vertices_local.map((p) => p[0]);
  const ys = mine.vertices_local.map((p) => p[1]);
  const zs = mine.vertices_local.map((p) => p[2]);
  const centre = [(Math.min(...xs) + Math.max(...xs)) / 2, (Math.min(...ys) + Math.max(...ys)) / 2, (Math.min(...zs) + Math.max(...zs)) / 2];
  const target = viewport.controls.target;
  const framedOff = Math.hypot(target.x - centre[0], target.y - centre[1], target.z - centre[2]);
  // How much of the view's width the line takes, at the distance the camera sits from it.
  const span = Math.hypot(Math.max(...xs) - Math.min(...xs), Math.max(...ys) - Math.min(...ys));
  const distance = viewport.camera.position.distanceTo(target);
  const camera = viewport.camera as THREE.PerspectiveCamera;
  const width = 2 * distance * Math.tan(THREE.MathUtils.degToRad(camera.fov / 2)) * camera.aspect;
  const details = await objectDetails(design);
  const onRow = details.measures.kind === 'face_design' && (details.measures.design.lines[0].edits ?? []).some((e) => e.row === 2 && e.string === mine.id);

  // The toe moved 2 m back from the pit at its bend: the step runs again, as a change to the design does.
  const stepsBefore = (await runLog()).filter((e) => 'command' in e && e.command === 'face.step').length;
  void show((await runCommand('string.move_vertex', { string: toeId, index: 1, to: [650300, 7449038, 100] })).summary);
  await wait(1500);
  const stepRuns = (await runLog()).filter((e) => 'command' in e && e.command === 'face.step');
  const lastStep = stepRuns[stepRuns.length - 1];

  // Row 1 taken out: row 2 goes with it, and your line on row 2 too.
  const removes = buttons().filter((b) => b.title.startsWith('Remove this step'));
  const removeTip = removes[0]?.title ?? null;
  removes[0]?.click();
  await wait(1500);
  const rowsLeft = buttons().filter((b) => b.title.startsWith('Remove this step')).length;
  const gone = !(await projectSummary()).objects.some((o) => o.id === mine.id);
  return {
    tip,
    selected_over_time: selectedOverTime,
    your_line_selected_a_second_after: selectedAfterASecond === mine.id,
    your_line_framed_off_m: Math.round(framedOff * 1000) / 1000,
    line_takes_of_view_width: Math.round((span / width) * 100) / 100,
    your_line_on_row_2: onRow,
    toe_moved_step_runs: stepRuns.length - stepsBefore,
    toe_moved_last_step: lastStep ? JSON.stringify(lastStep).slice(0, 300) : null,
    remove_tip: removeTip,
    rows_before_and_after_removing_row_1: [removes.length, rowsLeft],
    row_taken_out_took_your_line: gone,
  };
}

/**
 * Join on a berm, clicked in the Face design panel (decision 0047): Tim's step template with ramps A and B turning on
 * row 9, so their boxes are on row 8's berm. The Join switch shows on row 8 only, and a click runs the step with the two
 * switchbacks joined into one pad.
 */
async function checkJoinSwitch(log: (line: string) => void, show: Show) {
  const wait = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));
  const buttons = () => [...document.querySelectorAll('button')];
  log('design lines: Join in the Face design panel');
  const at = (p: number[]) => [650000 + p[0], 7448000 + p[1], p[2] ?? 0];
  const toe = await runCommand('string.create', { name: 'Toe 3', layer: 'toes', closed: false, points: [[0, 0], [600, 30], [1100, 10], [1500, 70]].map(at) });
  const ramp = async (name: string, points: number[][]) => (await runCommand('string.create', { name, layer: 'ramps', closed: false, points: points.map((p) => at([...p, 115])) })).record.outputs[0].id;
  const a = await ramp('Ramp A 3', [[150.249688, 17.985104], [148.002495, 62.928959]]);
  const b = await ramp('Ramp B 3', [[1099.896493, 20.561043], [1093.221172, 65.063179]]);
  const wall = (rl: number, batter: number, ramps: string[], turns: string[]) => ({ step: 'project', to: { kind: 'rl', rl }, batter, condition: 0.1, ramps, turns });
  const berm = { step: 'berm', on: null, width: 10, register: true, condition: 0.1, min_bench: 1 };
  const ab = ['A', 'B'];
  const road = (name: string, string: string, heading: string) => ({ name, string, fixed: 'start', heading, width: 30, grade: 10 });
  const content = {
    layer: 'faces 3',
    lines: [{ string: toe.record.outputs[0].id, profile: 'Steps', side: 'left', run_on: [0, 0] }],
    profiles: [{
      name: 'Steps', direction: 'up', condition: 0.1, register: { kind: 'rl', rl: 100 },
      ramps: [road('A', a, 'along'), road('B', b, 'back')],
      steps: [wall(115, 70, [], []), berm, wall(130, 70, ab, []), berm, wall(145, 65, ab, []), berm, wall(155, 65, ab, []), berm, wall(165, 65, ab, ab)],
    }],
    rules: [],
  };
  const saved = await runCommand('face.save_design', { name: 'Steps from Toe 3', content });
  const design = saved.record.outputs[0].id;
  void show((await runCommand('face.step', { design, name: 'Steps from Toe 3', content })).summary);
  await wait(500);
  // With a design open the panel has its Design list; with none, Open a design.
  const open = document.querySelector<HTMLSelectElement>('select[aria-label="Open a design"], select[aria-label="Design"]');
  if (!open) return { error: 'no list of designs in the Face design panel' };
  Object.getOwnPropertyDescriptor(HTMLSelectElement.prototype, 'value')?.set?.call(open, design);
  open.dispatchEvent(new Event('change', { bubbles: true }));
  await wait(2500);
  const joins = buttons().filter((x) => x.textContent?.trim() === 'Join');
  const tip = joins[0]?.title ?? null;
  const before = (await runLog()).filter((e) => 'command' in e && e.command === 'face.step').length;
  joins[0]?.click();
  await wait(2500);
  const runs = (await runLog()).filter((e) => 'command' in e && e.command === 'face.step');
  const last = runs[runs.length - 1];
  const messages = last && 'messages' in last ? (last.messages as string[]) : [];
  const pressed = buttons().find((x) => x.textContent?.trim() === 'Join')?.getAttribute('aria-pressed') ?? null;
  return {
    join_switches: joins.length,
    tip,
    step_runs_after_click: runs.length - before,
    joined_message: messages.find((m) => m.includes('joins the switchbacks')) ?? null,
    roads: messages.filter((m) => m.startsWith('Ramp ')).map((m) => m.slice(0, 90)),
    switch_pressed: pressed,
  };
}

/** A zip or folder of solids by the app's own path: import, draw, snap index, orbit, snap, undo. */
async function checkSolids(viewport: Viewport, config: BenchConfig, path: string, log: (line: string) => void, show: Show, section: () => SectionView | null, stringLayer: () => StringLayer | null) {
  const orbitSeconds = config.orbit_seconds;
  for (const surface of config.surfaces) await show((await runCommand('surface.import', { path: surface, layer: 'surfaces' })).summary);
  for (const strings of config.strings) await show((await runCommand('string.import', { path: strings })).summary);
  if (config.seam) await show((await runCommand('seam.build_model', { definition: config.seam, layer: 'seam models' })).summary);
  log(`solids: importing ${path}`);
  const start = performance.now();
  const reply = await runCommand('solid.import_many', { paths: [path], layer: 'solids', repair: true });
  const imported = performance.now();
  await show(reply.summary);
  const ready = performance.now();
  await viewport.nextFrame();
  const painted = performance.now();
  const tree = await checkTree(reply.summary);
  const hide = await checkHide(viewport, reply.summary, show);
  const click = await checkClickObjects(viewport, reply.summary, show);
  const layerCommands = await checkLayerCommands(viewport, reply.summary, show);
  const triangles = viewport.triangleCount;
  for (let i = 0; i < WARM_UP_FRAMES; i++) await viewport.nextFrame();
  log(`solids: orbiting ${triangles.toLocaleString()} triangles`);
  const orbited = await orbit(viewport, orbitSeconds);
  viewport.fitView();
  const snaps: number[] = [];
  for (let i = 0; i < 200; i++) {
    const ray = viewport.rayFromNdc(-0.6 + (i % 20) * 0.06, -0.3 + Math.floor(i / 20) * 0.06);
    const t = performance.now();
    await snapRay(ray.origin, ray.direction);
    snaps.push(performance.now() - t);
  }
  snaps.sort((a, b) => a - b);
  const pick = await checkPickTime(viewport);
  const camera = await checkCamera(viewport);
  const layer = stringLayer();
  const picks = layer ? await checkPicks(viewport, layer) : null;
  const snapSwitch = await checkSnapSwitch(viewport);
  const stringPick = await checkStringPick(viewport, show);
  const sectionCheck = await checkSection(viewport, section(), reply.summary, show, config.walls);
  const undoStart = performance.now();
  await show(await undo());
  return {
    solids: reply.summary.objects.length,
    triangles,
    engine_ms: reply.record.duration_ms,
    import_to_summary_ms: imported - start,
    draw_and_snap_index_ms: ready - imported,
    total_to_ready_ms: ready - start,
    total_to_painted_ms: painted - start,
    tree,
    hide,
    click,
    layer_commands: layerCommands,
    orbit: orbited,
    pick,
    camera,
    picks,
    snap_switch: snapSwitch,
    string_pick: stringPick,
    section: sectionCheck,
    snap_p50_ms: snaps[100],
    snap_p95_ms: snaps[190],
    undo_ms: performance.now() - undoStart,
    messages: reply.record.messages.slice(0, 2),
  };
}

/** The tree with every solid on one layer: opening it, and its frames from one end to the other. */
async function checkTree(summary: ProjectSummary) {
  const tree = document.querySelector('[data-slot="tree"]');
  const opener = document.querySelector<HTMLElement>('[aria-label="Open Solids on solids"]');
  if (!tree || !opener) return { error: 'no tree with a closed group of solids in the page' };
  const nextPaint = () => new Promise(requestAnimationFrame);
  const start = performance.now();
  opener.click();
  await nextPaint();
  const openMs = performance.now() - start;
  // Only the rows on screen are in the page, and the far end of the list is a scroll away.
  const rowsInPage = tree.querySelectorAll('[data-slot="tree-row"]').length;
  const frames: number[] = [];
  for (let k = 1; k <= TREE_SCROLL_STEPS; k++) {
    const t = performance.now();
    tree.scrollTo(0, (tree.scrollHeight * k) / TREE_SCROLL_STEPS);
    await nextPaint();
    frames.push(performance.now() - t);
  }
  await nextPaint();
  const rows = tree.querySelectorAll('[data-slot="tree-row"]');
  frames.sort((a, b) => a - b);
  return {
    open_ms: openMs,
    rows_in_page: rowsInPage,
    last_solid_in_last_row: rows[rows.length - 1]?.textContent?.includes(summary.objects[summary.objects.length - 1].name) ?? false,
    scroll_p50_ms: frames[Math.floor(frames.length / 2)],
    scroll_max_ms: frames[frames.length - 1],
  };
}

/** Hiding one solid of a layer and showing it again, then the whole layer, to the frame that shows it. */
async function checkHide(viewport: Viewport, summary: ProjectSummary, show: Show) {
  const solids = summary.objects.filter((o) => o.kind === 'solid');
  const one = solids[Math.floor(solids.length / 2)];
  const timed = async (change: () => Promise<ProjectSummary>) => {
    const start = performance.now();
    const next = await change();
    const replied = performance.now();
    await show(next);
    await viewport.nextFrame();
    return { reply_ms: replied - start, total_ms: performance.now() - start, triangles: viewport.triangleCount };
  };
  return {
    solids: solids.length,
    triangles_before: viewport.triangleCount,
    hide_one: await timed(() => setObjectsVisible([one.id], false)),
    show_one: await timed(() => setObjectsVisible([one.id], true)),
    hide_layer: await timed(() => setLayerVisible(one.layer, false)),
    show_layer: await timed(() => setLayerVisible(one.layer, true)),
    // The layer off, then one solid ticked on: it shows alone, and the rest come back.
    show_alone: await (async () => {
      await show(await setLayerVisible(one.layer, false));
      const alone = await timed(() => setObjectsVisible([one.id], true));
      const all = await timed(() => setObjectsVisible(solids.map((o) => o.id), true));
      return { alone, all };
    })(),
  };
}

/** Layer commands on the layer of solids, each timed to the frame that shows it, then undone. */
async function checkLayerCommands(viewport: Viewport, summary: ProjectSummary, show: Show) {
  const before = viewport.triangleCount;
  const timed = async (name: string, params: Record<string, unknown>) => {
    const start = performance.now();
    const reply = await runCommand(name, params);
    await show(reply.summary);
    await viewport.nextFrame();
    const ms = performance.now() - start;
    const triangles = viewport.triangleCount;
    const undoing = performance.now();
    await show(await undo());
    await viewport.nextFrame();
    return { ms, triangles, undo_ms: performance.now() - undoing, back: viewport.triangleCount === before };
  };
  const solid = summary.objects.find((o) => o.kind === 'solid');
  return {
    rename: await timed('layer.rename', { layer: 'solids', name: 'solids renamed' }),
    recolour: await timed('layer.recolour', { layer: 'solids', colour: [10, 20, 30] }),
    move_one: await timed('object.move', { objects: [solid?.id], layer: 'bench moved' }),
    delete_layer: await timed('layer.delete', { layer: 'solids' }),
  };
}

/**
 * Clicks in the viewport: on a solid with everything else off, then at the same spot on a seam model and
 * on a surface. Each should select what is on top there, light it up and find its row in the tree.
 */
async function checkClickObjects(viewport: Viewport, summary: ProjectSummary, show: Show) {
  const canvas = viewport.renderer.domElement;
  const box = canvas.getBoundingClientRect();
  const settle = async () => {
    for (let i = 0; i < 6; i++) await viewport.nextFrame();
  };
  const layerOf = (name: string) => summary.layers.find((l) => l.name === name);
  const setLayers = async (on: (name: string) => boolean) => {
    for (const layer of summary.layers) await show(await setLayerVisible(layer.id, on(layer.name)));
  };
  await setLayers((name) => name === 'solids');
  viewport.fitView();
  await settle();
  let spot: { x: number; y: number; id: string; local: Vec3 } | null = null;
  for (let down = 0.3; down < 0.8 && !spot; down += 0.05) {
    for (let across = 0.2; across < 0.85 && !spot; across += 0.05) {
      const [x, y] = [box.left + box.width * across, box.top + box.height * down];
      const ray = viewport.rayFromNdc(...viewport.clientToNdc(x, y));
      const hit = await snapRay(ray.origin, ray.direction);
      if (hit?.object) spot = { x, y, id: hit.object, local: hit.local };
    }
  }
  if (!spot || !layerOf('solids')) return { error: 'no solid under the view' };
  const { x, y, id, local } = spot;
  const row = () => document.querySelector('[data-slot="tree"] [data-state="selected"]');
  const clear = async () => {
    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }));
    for (let i = 0; i < 300 && row(); i++) await viewport.nextFrame();
    await settle();
  };
  const click = async () => {
    const start = performance.now();
    for (const [type, buttons] of [['pointerdown', 1], ['pointerup', 0]] as const) {
      canvas.dispatchEvent(new PointerEvent(type, { bubbles: true, button: 0, buttons, clientX: x, clientY: y, pointerId: 1, pointerType: 'mouse' }));
    }
    for (let i = 0; i < 300 && !row(); i++) await viewport.nextFrame();
    return { selected_ms: performance.now() - start, row: row()?.textContent ?? null };
  };

  // A selected solid lights up from its layer's table, drawn over the layer from its own geometry (decision 0060): no copy
  // is fetched, so what's drawn doesn't grow.
  const before = viewport.triangleCount;
  const highlight = () => viewport.content.children.some((g) => g.name.endsWith(' selected') && g.visible);
  const onSolid = await click();
  const selectedAt = performance.now();
  for (let i = 0; i < 300 && !highlight(); i++) await viewport.nextFrame();
  const litMs = performance.now() - selectedAt + onSolid.selected_ms;
  const lit = highlight();
  const copyTriangles = viewport.triangleCount - before;
  const inPlace = picture(viewport);
  viewport.focusOn(local, 200);
  await settle();
  const solidPicture = picture(viewport);
  await clear();
  const cleared = !highlight() && viewport.triangleCount === before;

  await setLayers((name) => name === 'seam models');
  await settle();
  const onSeam = await click();
  await settle();
  const seamPicture = picture(viewport);
  await clear();

  await setLayers((name) => name === 'surfaces');
  await settle();
  const onSurface = await click();
  await settle();
  const surfacePicture = picture(viewport);
  await clear();

  await setLayers(() => true);
  viewport.fitView();
  await settle();
  return {
    solid: { ...onSolid, lit_ms: litMs, lit, expected: summary.objects.find((o) => o.id === id)?.name, copy_triangles: copyTriangles, cleared, in_place: inPlace, picture: solidPicture },
    seam: { ...onSeam, expected: summary.objects.find((o) => o.kind === 'seam_model')?.name, picture: seamPicture },
    surface: { ...onSurface, expected: summary.objects.find((o) => o.kind === 'surface')?.name, picture: surfacePicture },
  };
}

/** What a press or a wheel tick costs: the time to pick, across the view, with everything loaded. */
async function checkPickTime(viewport: Viewport) {
  const box = viewport.renderer.domElement.getBoundingClientRect();
  const times: number[] = [];
  for (let k = 0; k < 200; k++) {
    const [x, y] = [box.left + box.width * (0.1 + (k % 20) * 0.04), box.top + box.height * (0.1 + Math.floor(k / 20) * 0.08)];
    const start = performance.now();
    await viewport.pickPoint?.(x, y);
    times.push(performance.now() - start);
  }
  times.sort((a, b) => a - b);
  return { p50_ms: times[100], p95_ms: times[190], max_ms: times[199] };
}

/** A line and a string picked by real clicks and Enter, and one abandoned, as Number and + Ramp take them. */
async function checkPicks(viewport: Viewport, strings: StringLayer) {
  const canvas = viewport.renderer.domElement;
  const box = canvas.getBoundingClientRect();
  const click = async (k: number) => {
    const [x, y] = [box.left + box.width * (0.35 + k * 0.1), box.top + box.height * 0.5];
    for (const [type, buttons] of [['pointerdown', 1], ['pointerup', 0]] as const) {
      canvas.dispatchEvent(new PointerEvent(type, { bubbles: true, button: 0, buttons, clientX: x, clientY: y, pointerId: 1, pointerType: 'mouse' }));
    }
    for (let i = 0; i < 8; i++) await viewport.nextFrame();
  };
  const key = (name: string) => window.dispatchEvent(new KeyboardEvent('keydown', { key: name, cancelable: true }));
  const within = <T,>(pick: Promise<T>) => Promise.race([pick, new Promise<'timed out'>((done) => setTimeout(() => done('timed out'), 5000))]);
  const count = (picked: Vec3[] | null | 'timed out') => (Array.isArray(picked) ? picked.length : picked);
  const line = strings.pickLine('Bench: first', 'Bench: second');
  await click(0);
  await click(1);
  const twoClicks = await within(line);
  const drawn = strings.pickString('Bench: a string');
  await click(0);
  await click(1);
  await click(2);
  key('Enter');
  const string = await within(drawn);
  const dropped = strings.pickString('Bench: abandoned');
  await click(0);
  key('Escape');
  const abandoned = await within(dropped);
  return { line_points: count(twoClicks), string_points: count(string), abandoned };
}

/**
 * Drags and wheels the real canvas with a point of the scene under the cursor: that point must stay
 * under the cursor (px), while the view turns and closes in on it (m).
 */
async function checkCamera(viewport: Viewport) {
  const canvas = viewport.renderer.domElement;
  const box = canvas.getBoundingClientRect();
  const settle = async () => {
    for (let i = 0; i < 4; i++) await viewport.nextFrame();
  };
  let found: { x: number; y: number; point: THREE.Vector3 } | null = null;
  for (let k = 0; k < 40 && !found; k++) {
    const [x, y] = [box.left + box.width * (0.25 + (k % 8) * 0.07), box.top + box.height * (0.3 + Math.floor(k / 8) * 0.1)];
    const hit = await viewport.pickPoint?.(x, y);
    if (hit) {
      // Start exactly on the point: a pick can land on a string a few pixels from the cursor.
      const point = new THREE.Vector3(...hit);
      const [px, py] = viewport.projectToClient(point);
      found = { x: px, y: py, point };
    }
  }
  if (!found) return { error: 'nothing under the cursor' };
  const { x, y, point } = found;
  const drift = () => {
    const [sx, sy] = viewport.projectToClient(point);
    return Math.hypot(sx - x, sy - y);
  };
  const send = (type: string, dx: number, dy: number) =>
    canvas.dispatchEvent(new PointerEvent(type, { bubbles: true, button: 0, buttons: 1, clientX: x + dx, clientY: y + dy, pointerId: 1, pointerType: 'mouse' }));

  const startDistance = viewport.camera.position.distanceTo(point);
  const startPosition = viewport.camera.position.clone();
  send('pointerdown', 0, 0);
  await settle();
  for (let k = 1; k <= 12; k++) {
    send('pointermove', 10 * k, 5 * k);
    await viewport.nextFrame();
  }
  const pivotMark = viewport.pivotMark.position?.distanceTo(point);
  send('pointerup', 120, 60);
  await settle();
  const turned = {
    drift_px: drift(),
    camera_moved_m: viewport.camera.position.distanceTo(startPosition),
    pivot_mark_off_point_m: pivotMark ?? null,
    pivot_mark_gone_after: viewport.pivotMark.position === null,
  };

  // The same drag sent at once, with no frame between the press and the first move: the turn waits
  // for its pick, so the point still holds (0044).
  const [qx, qy] = viewport.projectToClient(point);
  const quickSend = (type: string, dx: number, dy: number) =>
    canvas.dispatchEvent(new PointerEvent(type, { bubbles: true, button: 0, buttons: 1, clientX: qx + dx, clientY: qy + dy, pointerId: 1, pointerType: 'mouse' }));
  quickSend('pointerdown', 0, 0);
  for (let k = 1; k <= 6; k++) quickSend('pointermove', 15 * k, -4 * k);
  await settle();
  quickSend('pointerup', 90, -24);
  const [ax, ay] = viewport.projectToClient(point);
  const quickTurn = { drift_px: Math.hypot(ax - qx, ay - qy) };

  const turnedDistance = viewport.camera.position.distanceTo(point);
  const [zx, zy] = viewport.projectToClient(point);
  for (let k = 0; k < 12; k++) {
    canvas.dispatchEvent(new WheelEvent('wheel', { bubbles: true, cancelable: true, clientX: zx, clientY: zy, deltaY: -120 }));
    await viewport.nextFrame();
  }
  await settle();
  const [wx, wy] = viewport.projectToClient(point);
  return {
    start_distance_m: startDistance,
    turned,
    quick_turn: quickTurn,
    zoom: { drift_px: Math.hypot(wx - zx, wy - zy), from_m: turnedDistance, to_m: viewport.camera.position.distanceTo(point) },
    pan: await checkPan(viewport, point),
    field_of_view: await checkFieldOfView(viewport),
  };
}

/**
 * Changing the field of view keeps the view centre one size on screen (scale_off below 1e-9), and zoom to
 * extents frames the scene the same at the default and at 45 (box_across_screen equal; it was about 1.1 at
 * 45 before, so a corner of the box can sit just off screen). Puts the camera back after.
 */
async function checkFieldOfView(viewport: Viewport) {
  const [position, target] = [viewport.camera.position.clone(), viewport.controls.target.clone()];
  const before = viewport.pixelSize(target);
  viewport.setFieldOfView(45, false);
  const at45 = viewport.pixelSize(target);
  viewport.setFieldOfView(FIELD_OF_VIEW_DEFAULT, false);
  const back = viewport.pixelSize(target);
  const canvas = viewport.renderer.domElement.getBoundingClientRect();
  const fits: Record<string, { all_corners_on_screen: boolean; box_across_screen: number }> = {};
  for (const degrees of [FIELD_OF_VIEW_DEFAULT, 45]) {
    viewport.setFieldOfView(degrees, false);
    viewport.fitView();
    await viewport.nextFrame();
    const box = viewport.bounds;
    const xs: number[] = [];
    const ys: number[] = [];
    for (const cx of [box.min.x, box.max.x]) for (const cy of [box.min.y, box.max.y]) for (const cz of [box.min.z, box.max.z]) {
      const [sx, sy] = viewport.projectToClient(new THREE.Vector3(cx, cy, cz));
      xs.push(sx);
      ys.push(sy);
    }
    const [x0, x1, y0, y1] = [Math.min(...xs), Math.max(...xs), Math.min(...ys), Math.max(...ys)];
    fits[`fit_at_${degrees}`] = {
      all_corners_on_screen: x0 >= canvas.left && x1 <= canvas.right && y0 >= canvas.top && y1 <= canvas.bottom,
      box_across_screen: Math.max((x1 - x0) / canvas.width, (y1 - y0) / canvas.height),
    };
  }
  viewport.setFieldOfView(FIELD_OF_VIEW_DEFAULT, false);
  viewport.camera.position.copy(position);
  viewport.controls.target.copy(target);
  viewport.controls.update();
  await viewport.nextFrame();
  return {
    default_deg: FIELD_OF_VIEW_DEFAULT,
    scale_off_at_45: Math.abs(at45 / before - 1),
    scale_off_back: Math.abs(back / before - 1),
    ...fits,
  };
}

/**
 * A right-drag pan of 200 px from `point`: how far that point ends up from the cursor. Panning still
 * moves at the target's depth, so it is not on the cursor yet; 0044 makes it stick.
 */
async function checkPan(viewport: Viewport, point: THREE.Vector3) {
  const canvas = viewport.renderer.domElement;
  const [x, y] = viewport.projectToClient(point);
  const [position, target] = [viewport.camera.position.clone(), viewport.controls.target.clone()];
  const send = (type: string, dx: number, dy: number) =>
    canvas.dispatchEvent(new PointerEvent(type, { bubbles: true, button: 2, buttons: 2, clientX: x + dx, clientY: y + dy, pointerId: 1, pointerType: 'mouse' }));
  send('pointerdown', 0, 0);
  for (let k = 1; k <= 10; k++) {
    send('pointermove', 16 * k, 12 * k);
    await viewport.nextFrame();
  }
  send('pointerup', 160, 120);
  await viewport.nextFrame();
  const [px, py] = viewport.projectToClient(point);
  // Back to where it was, so the checks after this one see the same view as before.
  viewport.camera.position.copy(position);
  viewport.controls.target.copy(target);
  viewport.controls.update();
  await viewport.nextFrame();
  return { dragged_px: 200, point_off_cursor_px: Math.hypot(px - (x + 160), py - (y + 120)) };
}

/** Draws through the real toolbar, canvas and key: a click snaps, S, then a click lands on the plane. */
async function checkSnapSwitch(viewport: Viewport) {
  const canvas = viewport.renderer.domElement;
  const box = canvas.getBoundingClientRect();
  const settle = async () => {
    for (let i = 0; i < 6; i++) await viewport.nextFrame();
  };
  const button = (label: string) => [...document.querySelectorAll('button')].find((b) => b.textContent?.includes(label));
  const screen = () => document.body.innerText;
  const key = (name: string) => window.dispatchEvent(new KeyboardEvent('keydown', { key: name }));
  const click = async (x: number, y: number) => {
    for (const type of ['pointerdown', 'pointerup']) {
      canvas.dispatchEvent(new PointerEvent(type, { bubbles: true, button: 0, buttons: 1, clientX: x, clientY: y, pointerId: 1, pointerType: 'mouse' }));
    }
    await settle();
  };
  const [x, y] = [box.left + box.width * 0.5, box.top + box.height * 0.5];
  viewport.fitView();
  await settle();
  button('Draw string')?.click();
  await settle();
  const hover = async (hx: number, hy: number) => {
    canvas.dispatchEvent(new PointerEvent('pointermove', { bubbles: true, buttons: 0, clientX: hx, clientY: hy, pointerId: 1, pointerType: 'mouse' }));
    await settle();
    return document.querySelector<HTMLElement>('.snap-label')?.textContent ?? '';
  };
  const overSurface = await hover(x, y);
  await click(x, y);
  const first = screen().includes('1 vertices') && !screen().includes('1 vertices (on the working plane');
  key('s');
  await settle();
  const switchedOff = screen().includes('Snap off');
  const overPlane = await hover(x + 60, y + 30);
  await click(x + 60, y + 30);
  const second = screen().includes('2 vertices (on the working plane at RL');
  key('Escape');
  key('s');
  await settle();
  const switchedOn = screen().includes('Snap on');
  button('Select')?.click();
  await settle();
  return {
    first_click_snapped: first,
    s_turned_it_off: switchedOff,
    second_click_on_the_plane: second,
    s_turned_it_on: switchedOn,
    label_over_a_solid: overSurface.replace(/[-\d.]+$/, '<RL>'),
    label_snap_off: overPlane.replace(/[-\d.]+$/, '<RL>'),
  };
}

/**
 * A real string held 50 m above the surface, so a pick that only sees surfaces lands elsewhere:
 * turning from its vertex or its segment must pivot on the string, and hovering must name it.
 */
async function checkStringPick(viewport: Viewport, show: Show) {
  const canvas = viewport.renderer.domElement;
  const box = canvas.getBoundingClientRect();
  const settle = async () => {
    for (let i = 0; i < 6; i++) await viewport.nextFrame();
  };
  viewport.fitView();
  await settle();
  // The solids have gaps: the first row of the view with three surface points along it.
  let hits: SnapHit[] = [];
  for (let down = 0.25; down < 0.8 && hits.length < 3; down += 0.05) {
    hits = [];
    for (let across = 0.2; across < 0.85 && hits.length < 3; across += 0.1) {
      const ray = viewport.rayFromNdc(...viewport.clientToNdc(box.left + box.width * across, box.top + box.height * down));
      const hit = await snapRay(ray.origin, ray.direction);
      if (hit) hits.push(hit);
    }
  }
  if (hits.length < 3) return { error: 'the string needs three points on a surface' };
  const reply = await runCommand('string.create', { name: 'bench string', layer: 'design', points: hits.map((h) => [h.world[0], h.world[1], h.world[2] + 50]) });
  await show(reply.summary);
  await settle();

  const points = hits.map((h) => new THREE.Vector3(h.local[0], h.local[1], h.local[2] + 50));
  const middle = points[0].clone().lerp(points[1], 0.5);
  const send = (type: string, x: number, y: number, buttons: number) =>
    canvas.dispatchEvent(new PointerEvent(type, { bubbles: true, button: 0, buttons, clientX: x, clientY: y, pointerId: 1, pointerType: 'mouse' }));
  /** Close in on `p`, so the string stands clear of anything dense behind it. */
  const closeIn = async (p: THREE.Vector3) => {
    viewport.focusOn([p.x, p.y, p.z], 300);
    await settle();
  };
  /** How far the rotation point lands from `p` when a drag starts on it. */
  const turnFrom = async (p: THREE.Vector3) => {
    await closeIn(p);
    const [x, y] = viewport.projectToClient(p);
    send('pointerdown', x, y, 1);
    await settle();
    send('pointermove', x + 12, y + 6, 1);
    await settle();
    const off = viewport.pivotMark.position?.distanceTo(p) ?? null;
    send('pointerup', x + 12, y + 6, 0);
    await settle();
    return off;
  };
  const label = async (p: THREE.Vector3) => {
    await closeIn(p);
    const [x, y] = viewport.projectToClient(p);
    send('pointermove', x, y, 0);
    await settle();
    return document.querySelector<HTMLElement>('.snap-label')?.textContent?.replace(/[-\d.]+$/, '<RL>') ?? '';
  };
  const fromVertex = await turnFrom(points[1]);
  const fromSegment = await turnFrom(middle);
  const button = (name: string) => [...document.querySelectorAll('button')].find((b) => b.textContent?.includes(name));
  button('Draw string')?.click();
  await settle();
  const overVertex = await label(points[1]);
  const overSegment = await label(middle);
  button('Select')?.click();
  await settle();
  await show(await undo());
  return { turn_from_vertex_off_m: fromVertex, turn_from_segment_off_m: fromSegment, label_over_vertex: overVertex, label_over_segment: overSegment };
}

/** Picks a section through the real toolbar and canvas, then steps it: the gate's 50 ms a step. */
async function checkSection(viewport: Viewport, section: SectionView | null, summary: ProjectSummary, show: Show, walls: string[]) {
  if (!section) return { error: 'no section view' };
  const hiddenBefore = getComputedStyle(section.overviewCanvas).display === 'none';
  const gridHiddenBefore = getComputedStyle(section.gridCanvas).display === 'none';
  const canvas = viewport.renderer.domElement;
  const box = canvas.getBoundingClientRect();
  const settle = async () => {
    for (let i = 0; i < 6; i++) await viewport.nextFrame();
  };
  const button = (name: string) => [...document.querySelectorAll('button')].find((b) => b.textContent?.includes(name));
  const click = async (x: number, y: number) => {
    for (const type of ['pointerdown', 'pointerup']) {
      canvas.dispatchEvent(new PointerEvent(type, { bubbles: true, button: 0, buttons: 1, clientX: x, clientY: y, pointerId: 1, pointerType: 'mouse' }));
    }
    await settle();
  };
  viewport.fitView();
  await settle();
  button('Section')?.click();
  await settle();
  await click(box.left + box.width * 0.3, box.top + box.height * 0.5);
  const picked = performance.now();
  await click(box.left + box.width * 0.7, box.top + box.height * 0.5);
  // The first section makes what the steps after it reuse.
  while (!viewport.sectionView && performance.now() - picked < 60_000) await viewport.nextFrame();
  const openMs = performance.now() - picked;
  await settle();
  await settle();
  const opened = section.open && viewport.sectionView;
  const wide = picture(viewport);
  const [times, engine]: number[][] = [[], []];
  for (let k = 0; k < 40; k++) {
    await section.step(k < 20 ? 25 : -25);
    times.push(section.lastMs);
    engine.push(section.lastEngineMs);
  }
  times.sort((a, b) => a - b);
  engine.sort((a, b) => a - b);
  const frames = await orbitless(viewport);

  // The overview: a drag across it moves the section in whole steps.
  const map = section.overviewCanvas;
  // Where it lands on screen, which its own pixels can't show: a box in the corner, the section clear of it.
  const shown = getComputedStyle(map).display !== 'none';
  const mapBox = map.getBoundingClientRect();
  const place = [mapBox.left - box.left, box.bottom - mapBox.bottom, mapBox.width, mapBox.height];
  const inCorner = place.every((value, i) => Math.abs(value - [12, 12, 240, 240][i]) < 2);
  const sectionClear = document.elementFromPoint(box.left + box.width / 2, box.top + box.height / 2) === canvas;
  const across = section.firstSolidPoint();
  const atStart = section.offset;
  const drag = (type: string, x: number, y: number) =>
    map.dispatchEvent(new PointerEvent(type, { bubbles: true, button: 0, buttons: 1, clientX: x, clientY: y, pointerId: 1, pointerType: 'mouse' }));
  const [dragX, dragY] = [mapBox.left + mapBox.width * 0.5, mapBox.top + mapBox.height * 0.2];
  drag('pointerdown', dragX, dragY);
  await settle();
  const dragged = section.offset - atStart;
  const whole = dragged !== 0 && dragged % section.stepSize === 0;
  // Moving the wrong way, or the map sliding, would carry it on from here.
  drag('pointermove', dragX, dragY);
  await settle();
  const stayed = section.offset - atStart === dragged;
  // Held at the map's edge, a drag runs on past what is in view.
  drag('pointermove', dragX, mapBox.top + 1);
  for (let k = 0; k < 30; k++) await viewport.nextFrame();
  const carried = section.offset - atStart - dragged;
  drag('pointerup', dragX, mapBox.top + 1);
  await settle();
  await settle();
  const carriedOnMap = section.overviewOnMap;

  // A section as a planner would pick one: 1.5 km across the first solid cut, not the whole view.
  const o = summary.origin ?? [0, 0, 0];
  let close = '';
  let selected = {};
  let grid = {};
  let fill = {};
  let editing = {};
  let wall = {};
  if (across) {
    const [x, y] = [across.x + o[0], across.y + o[1]];
    await section.show([x - 750, y - 200], [x + 750, y + 200], summary);
    await settle();
    close = picture(viewport);
    const solid = summary.objects.find((o) => o.kind === 'surface');
    if (solid) {
      section.setSelected(solid.id);
      await settle();
      selected = { lines: section.lastCut?.groups.find((g) => g.kind === 'selected')?.segments ?? 0, picture: picture(viewport) };
      section.setSelected(null);
      await settle();
    }
    fill = await checkFill(viewport, section, button, settle);
    grid = await checkGrid(viewport, section, box, settle);
    editing = await checkEditing(viewport, section, box, button, settle, show);
    if (walls.length > 1) wall = await checkWall(viewport, section, walls, [x, y], settle, show);
    await section.step(125);
    await settle();
  }
  // Step lines stand apart at 200 m on this map, and a long move walks the section off it.
  section.stepSize = 200;
  const overview = map.toDataURL('image/png');
  const pictures: number[][] = [];
  let staysOnMap = section.overviewOnMap;
  for (let k = 0; k < 5; k++) {
    const started = performance.now();
    await section.step(2_000);
    const stepped = performance.now() - started;
    const picture = section.overviewPictureMs;
    await viewport.nextFrame();
    pictures.push([stepped, picture, performance.now() - started]);
    staysOnMap &&= section.overviewOnMap;
  }
  await settle();
  const walked = map.toDataURL('image/png');
  section.stepSize = 25;
  button('Section')?.click();
  await settle();
  const hiddenAfter = getComputedStyle(map).display === 'none';
  const gridHiddenAfter = getComputedStyle(section.gridCanvas).display === 'none';
  return { opened, open_ms: openMs, closed: !section.open && !viewport.sectionView, step_p50_ms: times[20], step_max_ms: times[39], engine_p50_ms: engine[20], fps: frames, overview_shown: shown, overview_place: place, overview_in_corner: inCorner, overview_section_clear: sectionClear, overview_hidden_before: hiddenBefore, overview_hidden_after: hiddenAfter, overview_pictures_ms: pictures, overview_drag_m: dragged, overview_whole_steps: whole, overview_stayed: stayed, overview_carried_m: carried, overview_carried_on_map: carriedOnMap, overview_stays_on_map: staysOnMap, grid_hidden_before: gridHiddenBefore, grid_hidden_after: gridHiddenAfter, grid, fill, editing, wall, wide, close, selected, overview, walked };
}

/**
 * A string drawn on the section and edited there through the real toolbar, canvas and keys. Its
 * vertices must be what the engine snapped to, to the bit, and on the plane; it must then be an
 * ordinary string, and undo.
 */
async function checkEditing(viewport: Viewport, section: SectionView, box: DOMRect, button: (name: string) => HTMLButtonElement | undefined, settle: () => Promise<void>, show: Show) {
  const canvas = viewport.renderer.domElement;
  const send = (type: string, x: number, y: number, buttons: number) =>
    canvas.dispatchEvent(new PointerEvent(type, { bubbles: true, button: 0, buttons, clientX: x, clientY: y, pointerId: 1, pointerType: 'mouse' }));
  const click = async (x: number, y: number) => {
    send('pointermove', x, y, 0);
    await viewport.nextFrame();
    send('pointerdown', x, y, 1);
    send('pointerup', x, y, 0);
    await settle();
  };
  // Three cut points well apart on screen, clear of the plan map.
  const positions = section.lastCut?.positions ?? new Float32Array();
  const targets: [number, number][] = [];
  for (let k = 0; k + 2 < positions.length && targets.length < 3; k += 3) {
    const [x, y] = viewport.projectToClient(new THREE.Vector3(positions[k], positions[k + 1], positions[k + 2]));
    const inside = x > box.left + 280 && x < box.right - 90 && y > box.top + 40 && y < box.bottom - 40;
    if (inside && targets.every((t) => Math.hypot(t[0] - x, t[1] - y) > 120)) targets.push([x, y]);
  }
  if (targets.length < 3) return { error: 'three cut points were not on screen' };
  // A snap's cost, as the cursor moves.
  const times: number[] = [];
  for (let i = 0; i < 200; i++) {
    const started = performance.now();
    await section.tools.snap(targets[0][0] + (i % 40), targets[0][1] + (i % 25), 10, 8);
    times.push(performance.now() - started);
  }
  times.sort((a, b) => a - b);

  const [planX, planY] = [box.right - 200, box.top + 60];
  const expected = await Promise.all(targets.map(([x, y]) => section.tools.snap(x, y, 10, 8)));
  const emptySpot = await section.tools.snap(planX, planY, 10, 8);
  const onPlane = section.tools.onPlane(planX, planY);
  const before = await projectSummary();
  const known = new Set(before.objects.map((o) => o.id));
  const origin = before.origin ?? [0, 0, 0];
  button('Draw string')?.click();
  await settle();
  for (const [x, y] of targets) await click(x, y);
  await click(planX, planY);
  window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter' }));
  await settle();
  await settle();
  const made = (await projectSummary()).objects.find((o) => !known.has(o.id));
  if (!made) return { error: 'no string was made' };
  const details = await objectDetails(made.id);
  if (details.measures.kind !== 'string') return { error: 'what was made is not a string' };
  const points = details.measures.points;
  const exact = expected.every((hit, i) => hit?.world.every((c, k) => c === points[i][k]));
  const off = (p: [number, number, number]) => Math.abs(section.tools.offset(new THREE.Vector3(p[0] - origin[0], p[1] - origin[1], p[2] - origin[2])));
  const worstOff = Math.max(...points.map(off));
  const free = points[3].every((c, k) => Math.abs(c - (onPlane?.world[k] ?? 0)) < 1e-6);
  const cutSegments = (section.lastCut?.groups ?? []).filter((g) => g.kind === 'string').reduce((sum, g) => sum + g.segments, 0);
  const drawn = screen(viewport, [section.gridCanvas, section.overviewCanvas]);

  // Selected by a click on it, then its free end dragged to an empty spot.
  button('Select')?.click();
  await settle();
  await click((targets[0][0] + targets[1][0]) / 2, (targets[0][1] + targets[1][1]) / 2);
  const [toX, toY] = [planX - 70, planY + 45];
  send('pointermove', planX, planY, 0);
  await viewport.nextFrame();
  send('pointerdown', planX, planY, 1);
  await settle();
  send('pointermove', toX, toY, 1);
  await settle();
  const target = section.tools.onPlane(toX, toY);
  send('pointerup', toX, toY, 0);
  await settle();
  await settle();
  const after = await objectDetails(made.id);
  const moved = after.measures.kind === 'string' ? after.measures.points[3] : null;
  const dragged = !!moved && !!target && moved.every((c, k) => Math.abs(c - target.world[k]) < 1e-6);

  await show(await undo());
  await show(await undo());
  await settle();
  const gone = !(await projectSummary()).objects.some((o) => o.id === made.id);
  return { targets: expected.map((hit) => hit?.kind), snap_p50_ms: times[100], snap_max_ms: times[199], empty_spot_snapped: !!emptySpot, vertices: points.length, exact, worst_off_plane_m: worstOff, free_point_on_plane: free, string_segments_in_cut: cutSegments, dragged, undone: gone, drawn };
}

/**
 * A wall stepped up real grids by the command the string tools run, then read on a section square to
 * it: a toe across `at` registered on the first grid, a step to each grid after it with a berm.
 */
async function checkWall(viewport: Viewport, section: SectionView, grids: string[], at: [number, number], settle: () => Promise<void>, show: Show) {
  let summary = await projectSummary();
  const known = new Set(summary.objects.map((o) => o.id));
  const made = () => {
    const fresh = summary.objects.filter((o) => !known.has(o.id));
    fresh.forEach((o) => known.add(o.id));
    return fresh;
  };
  const run = async (name: string, params: Record<string, unknown>) => {
    const reply = await runCommand(name, params);
    summary = reply.summary;
    await show(summary);
    return { fresh: made(), messages: reply.record.messages };
  };
  const targets: string[] = [];
  for (const path of grids) targets.push((await run('surface.import', { path, layer: 'walls' })).fresh[0].id);
  // Square to the section's own direction, 300 m each way.
  const [ux, uy] = [1500 / Math.hypot(1500, 400), 400 / Math.hypot(1500, 400)];
  const ends = [1, -1].map((k) => [at[0] - k * 300 * uy, at[1] + k * 300 * ux, 0]);
  const toe = (await run('string.create', { name: 'bench toe', layer: 'walls', points: ends })).fresh[0];
  await run('string.drape', { string: toe.id, surface: targets[0] });
  let from = toe.id;
  const steps: string[] = [];
  const started = performance.now();
  for (const target of targets.slice(1)) {
    const { fresh, messages } = await run('face.project_step', { string: from, target, batter: 70, side: 'left', berm: 10, tolerance: 0.1 });
    steps.push(...messages);
    const next = fresh.find((o) => o.name.endsWith('next toe'));
    if (!next) return { error: 'no next toe was made', steps };
    from = next.id;
  }
  const ms = performance.now() - started;
  // The same grids as a whole design, as the Face design panel runs it: the toe going up and going down.
  const wall = (to: unknown) => ({ step: 'project', to, batter: 'Batter', condition: 0.1 });
  const up = targets.slice(1).flatMap((id) => [wall({ kind: 'surface', id }), { step: 'berm', on: null, width: 10, register: true, condition: 0.1 }]);
  const register = { kind: 'surface', id: targets[0] };
  const content = {
    layer: 'bench faces',
    lines: ['Highwall', 'Highwall down'].map((profile) => ({ string: toe.id, profile, side: 'left', run_on: [100, 100] })),
    profiles: [
      { name: 'Highwall', direction: 'up', register, condition: null, steps: [...up, wall({ kind: 'rl', rl: 500 })] },
      { name: 'Highwall down', direction: 'down', register, condition: null, steps: [wall({ kind: 'rl', rl: -500 })] },
    ],
    rules: [{ name: 'Batter', value: { type: 'fixed', value: 70 } }],
  };
  const design = (await run('face.save_design', { name: 'Bench design', content })).fresh[0];
  const designed = performance.now();
  const whole = await run('face.project', { design: design.id });
  const designMs = performance.now() - designed;
  const again = await run('face.project', { design: design.id });
  // A line on a layer, in place of the string: the toe, going up.
  await run('object.move', { objects: [toe.id], layer: 'bench toes' });
  const toes = (await projectSummary()).layers.find((l) => l.name === 'bench toes')?.id;
  const onLayer = { ...content, layer: 'bench layer faces', lines: [{ layer: toes, profile: 'Highwall', side: 'left', run_on: [100, 100] }] };
  const layerDesign = (await run('face.save_design', { name: 'Bench layer design', content: onLayer })).fresh[0];
  const byLayer = await run('face.project', { design: layerDesign.id });
  // A batter and a berm that change part way along the toe: everything to one side of its middle is a domain.
  const across = [-uy * 2000, ux * 2000];
  const domain = [
    [at[0] - across[0], at[1] - across[1], 0],
    [at[0] + across[0], at[1] + across[1], 0],
    [at[0] + across[0] + ux * 2000, at[1] + across[1] + uy * 2000, 0],
    [at[0] - across[0] + ux * 2000, at[1] - across[1] + uy * 2000, 0],
  ];
  const polygon = (await run('string.create', { name: 'bench domain', layer: 'bench domains', points: domain, closed: true })).fresh[0];
  const changing = {
    ...content,
    layer: 'bench rule faces',
    lines: [{ string: toe.id, profile: 'Highwall', side: 'left', run_on: [100, 100] }],
    rules: [
      { name: 'Batter', value: { type: 'inside_polygon', polygon: polygon.id, inside: 60, outside: 70 } },
      { name: 'Berm', value: { type: 'inside_polygon', polygon: polygon.id, inside: 15, outside: 10 } },
    ],
    profiles: content.profiles.map((profile) => ({ ...profile, steps: profile.steps.map((step) => (step.step === 'berm' ? { ...step, width: 'Berm', min_bench: 5 } : step)) })),
  };
  const ruleRun = await (async () => {
    try {
      const saved = (await run('face.save_design', { name: 'Bench rule design', content: changing })).fresh[0];
      const started = performance.now();
      const made = await run('face.project', { design: saved.id });
      return { ms: performance.now() - started, made: made.fresh.length, messages: made.messages };
    } catch (e) {
      return { error: String(e) };
    }
  })();
  // A road from the middle of the toe up through the walls, each way along it.
  const roadFrom = async (heading: 'along' | 'back') => {
    try {
      const rampString = (await run('string.create', { name: `Bench ramp ${heading}`, layer: 'bench ramps', points: [[at[0] - 200 * ux, at[1] - 200 * uy, 0], [at[0] + 200 * ux, at[1] + 200 * uy, 0]] })).fresh[0];
      const ramps = [{ name: 'A', string: rampString.id, fixed: 'start', heading, width: 30, grade: 10 }];
      const profiles = content.profiles.map((profile) => (profile.name === 'Highwall' ? { ...profile, ramps, steps: profile.steps.map((step) => (step.step === 'project' ? { ...step, ramps: ['A'] } : step)) } : profile));
      const withRoad = { ...content, profiles, layer: `bench road ${heading}`, lines: [{ string: toe.id, profile: 'Highwall', side: 'left', run_on: [100, 100] }] };
      const saved = (await run('face.save_design', { name: `Bench road ${heading}`, content: withRoad })).fresh[0];
      const started = performance.now();
      const made = await run('face.project', { design: saved.id });
      return { ms: performance.now() - started, made: made.fresh.length, messages: made.messages, labels: document.querySelectorAll('.road-label').length };
    } catch (e) {
      return { error: String(e) };
    }
  };
  const rampRun = { along: await roadFrom('along'), back: await roadFrom('back') };
  // A toe offset into strips, numbered by a line across them, and one ramp string across the lot.
  const stripRun = await (async () => {
    try {
      const strips = (await run('string.offset', { string: toe.id, distance: 60, side: 'left', times: 4 })).fresh.map((o) => o.id);
      const point = (out: number): Vec3 => [at[0] + out * ux, at[1] + out * uy, 0];
      const numbered = await run('string.number_across', { strings: strips, from: point(0), to: point(300), start: 20, name: 'Bench strip #' });
      const rampString = (await run('string.create', { name: 'Bench ramp across', layer: 'bench ramps', points: [point(0), point(2000)] })).fresh[0];
      const ramps = [{ name: 'A', string: rampString.id, fixed: 'start', heading: 'along', width: 30, grade: 10 }];
      const profiles = content.profiles.map((profile) => (profile.name === 'Highwall' ? { ...profile, ramps, steps: profile.steps.map((step) => (step.step === 'project' ? { ...step, ramps: ['A'] } : step)) } : profile));
      const lines = strips.map((string) => ({ string, profile: 'Highwall', side: 'left', run_on: [100, 100] }));
      const saved = (await run('face.save_design', { name: 'Bench strips', content: { ...content, profiles, layer: 'bench strips', lines } })).fresh[0];
      const started = performance.now();
      const made = await run('face.project', { design: saved.id });
      return { ms: performance.now() - started, numbered: numbered.messages, made: made.fresh.length, roads: made.messages.filter((m) => m.startsWith('Ramp A')).length, messages: made.messages };
    } catch (e) {
      return { error: String(e) };
    }
  })();
  const close = async (half: number) => {
    await section.show([at[0] - half * ux, at[1] - half * uy], [at[0] + half * ux, at[1] + half * uy], summary);
    await settle();
    await settle();
    return screen(viewport, [section.gridCanvas, section.overviewCanvas]);
  };
  const wide = await close(400);
  const tight = await close(120);
  const surfaces = (section.lastCut?.groups ?? []).filter((g) => g.kind === 'surface').reduce((sum, g) => sum + g.segments, 0);
  return { steps, ms, surface_segments_in_cut: surfaces, design_ms: designMs, design_messages: whole.messages, design_made: whole.fresh.length, design_by_layer_made: byLayer.fresh.length, design_by_layer_messages: byLayer.messages, design_rule_run: ruleRun, design_ramp_run: rampRun, design_strip_run: stripRun, design_rerun_made: again.fresh.length, wide, tight };
}

/** The fills on, as they first show, then off and on again by the toolbar's Fill button. */
async function checkFill(viewport: Viewport, section: SectionView, button: (name: string) => HTMLButtonElement | undefined, settle: () => Promise<void>) {
  const triangles = (kind?: string) => (section.lastCut?.groups ?? []).filter((group) => !kind || group.kind === kind).reduce((sum, group) => sum + group.triangles, 0);
  const over = [section.gridCanvas, section.overviewCanvas];
  const toggle = async () => {
    button('Fill')?.click();
    await settle();
    await settle();
  };
  const [on, solids, seams, unfilled, filled] = [triangles(), triangles('solid'), triangles('seam'), section.unfilled, screen(viewport, over)];
  await toggle();
  const [off, lines] = [triangles(), screen(viewport, over)];
  await toggle();
  const materials = [...new Set((section.lastCut?.groups ?? []).map((group) => group.material).filter(Boolean))];
  return { triangles: on, solid_triangles: solids, seam_triangles: seams, materials, unfilled, off_triangles: off, back_triangles: triangles(), filled, lines };
}

/**
 * The RL grid against what is drawn: every line must sit where the camera puts that RL and distance,
 * through a zoom in and out and a pan by the real controls, with the labels staying apart.
 */
async function checkGrid(viewport: Viewport, section: SectionView, box: DOMRect, settle: () => Promise<void>) {
  const canvas = viewport.renderer.domElement;
  const gridBox = section.gridCanvas.getBoundingClientRect();
  const covers = [gridBox.left - box.left, gridBox.top - box.top, gridBox.width - box.width, gridBox.height - box.height].every((d) => Math.abs(d) < 1);
  const clear = document.elementFromPoint(box.left + box.width / 2, box.top + box.height / 2) === canvas;
  const [middleX, middleY] = [box.left + box.width / 2, box.top + box.height / 2];
  const readings = { worst_px: 0, least_gap_px: Infinity, most_gap_px: 0, spacings: new Set<number>(), most_labels: 0, worst_draw_ms: 0 };
  const read = () => {
    const { spacing, along, rl } = section.gridLines;
    const midAlong = along[Math.floor(along.length / 2)]?.value ?? 0;
    const midRl = rl[Math.floor(rl.length / 2)]?.value ?? 0;
    for (const { value, x } of along) readings.worst_px = Math.max(readings.worst_px, Math.abs(viewport.projectToClient(section.pointOn(value, midRl))[0] - box.left - x));
    for (const { value, y } of rl) readings.worst_px = Math.max(readings.worst_px, Math.abs(viewport.projectToClient(section.pointOn(midAlong, value))[1] - box.top - y));
    if (along.length > 1) {
      const gap = along[1].x - along[0].x;
      readings.least_gap_px = Math.min(readings.least_gap_px, gap);
      readings.most_gap_px = Math.max(readings.most_gap_px, gap);
    }
    readings.spacings.add(spacing);
    readings.most_labels = Math.max(readings.most_labels, along.length + rl.length);
    readings.worst_draw_ms = Math.max(readings.worst_draw_ms, section.gridDrawMs);
  };
  const frames = async () => {
    await viewport.nextFrame();
    await viewport.nextFrame();
  };
  const wheel = async (deltaY: number, times: number) => {
    for (let k = 0; k < times; k++) {
      canvas.dispatchEvent(new WheelEvent('wheel', { bubbles: true, cancelable: true, deltaY, clientX: middleX, clientY: middleY }));
      await frames();
      read();
    }
  };
  const pointer = (type: string, x: number, y: number) =>
    canvas.dispatchEvent(new PointerEvent(type, { bubbles: true, button: 0, buttons: 1, clientX: x, clientY: y, pointerId: 1, pointerType: 'mouse' }));
  read();
  const engine = readOffLabels(viewport, section, box);
  const framed = section.gridLines.spacing;
  await wheel(-120, 130);
  await settle();
  const closest = section.gridLines.spacing;
  const deep = screen(viewport, [section.gridCanvas, section.overviewCanvas]);
  await wheel(120, 175);
  const widest = section.gridLines.spacing;
  await wheel(-120, 45);
  // A pan by the real controls, left drag.
  pointer('pointerdown', middleX, middleY);
  await frames();
  pointer('pointermove', middleX + 137, middleY - 83);
  await frames();
  pointer('pointerup', middleX + 137, middleY - 83);
  await settle();
  read();
  const picture = screen(viewport, [section.gridCanvas, section.overviewCanvas]);
  // A step moves the plane, not the distances or RLs.
  const places = () => [...section.gridLines.along.map((l) => l.x), ...section.gridLines.rl.map((l) => l.y)];
  const before = places();
  await section.step(25);
  await settle();
  const after = places();
  const stepKeeps = before.length === after.length && before.every((at, k) => Math.abs(at - after[k]) < 1e-6);
  await section.step(-25);
  await settle();
  return { covers, clear, framed_spacing: framed, closest_spacing: closest, widest_spacing: widest, step_keeps: stepKeeps, engine, ...readings, spacings: [...readings.spacings].sort((a, b) => a - b), picture, deep };
}

/**
 * Reads the ends of what is cut off the grid's labels, as a planner would, and sets them against the
 * engine's own figures in mine grid: the only check here that does not lean on the local origin.
 */
function readOffLabels(viewport: Viewport, section: SectionView, box: DOMRect) {
  const cut = section.lastCut;
  const { along, rl } = section.gridLines;
  if (!cut?.bounds || along.length < 2 || rl.length < 2) return { error: 'nothing to read' };
  const [left, right] = [along[0], along[along.length - 1]];
  const [high, low] = [rl[rl.length - 1], rl[0]];
  const [alongRange, rlRange]: [number, number][] = [
    [Infinity, -Infinity],
    [Infinity, -Infinity],
  ];
  const widen = (range: [number, number], value: number) => {
    range[0] = Math.min(range[0], value);
    range[1] = Math.max(range[1], value);
  };
  const { positions } = cut;
  for (let k = 0; k + 2 < positions.length; k += 3) {
    const [x, y] = viewport.projectToClient(new THREE.Vector3(positions[k], positions[k + 1], positions[k + 2]));
    widen(alongRange, left.value + ((x - box.left - left.x) / (right.x - left.x)) * (right.value - left.value));
    widen(rlRange, low.value + ((y - box.top - low.y) / (high.y - low.y)) * (high.value - low.value));
  }
  const { along: engineAlong, rl: engineRl } = cut.bounds;
  const worst = Math.max(...[0, 1].flatMap((i) => [Math.abs(alongRange[i] - engineAlong[i]), Math.abs(rlRange[i] - engineRl[i])]));
  return { points: positions.length / 3, read_along: alongRange, engine_along: engineAlong, read_rl: rlRange, engine_rl: engineRl, worst_m: worst };
}

/** The viewport as it stands on screen: the 3D canvas, then what sits over it, each where it lands. */
function screen(viewport: Viewport, over: HTMLCanvasElement[]): string {
  const source = viewport.renderer.domElement;
  const box = source.getBoundingClientRect();
  const scale = source.width / box.width;
  const copy = Object.assign(document.createElement('canvas'), { width: source.width, height: source.height });
  const context = copy.getContext('2d');
  viewport.renderer.render(viewport.scene, viewport.camera);
  context?.drawImage(source, 0, 0);
  for (const canvas of over) {
    const at = canvas.getBoundingClientRect();
    if (at.width > 0) context?.drawImage(canvas, (at.left - box.left) * scale, (at.top - box.top) * scale, at.width * scale, at.height * scale);
  }
  return copy.toDataURL('image/png');
}

/** What the viewport shows now, as a PNG data URL: drawn again and copied before the frame is gone. */
function picture(viewport: Viewport): string {
  const source = viewport.renderer.domElement;
  const copy = Object.assign(document.createElement('canvas'), { width: source.width, height: source.height });
  viewport.renderer.render(viewport.scene, viewport.camera);
  copy.getContext('2d')?.drawImage(source, 0, 0);
  return copy.toDataURL('image/png');
}

/** Frames per second over two seconds, the view left alone. */
async function orbitless(viewport: Viewport): Promise<number> {
  const start = performance.now();
  let frames = 0;
  while (performance.now() - start < 2000) {
    await viewport.nextFrame();
    frames++;
  }
  return (frames * 1000) / (performance.now() - start);
}

/** Loads each surface coloured by RL over its own range, and with `opacity` under 1, seen through, all stacked. */
async function loadSurfaces(viewport: Viewport, paths: string[], log: (line: string) => void, opacity: number | null) {
  const loads = [];
  for (const path of paths) {
    log(`loading ${path}`);
    // The same path as the app: an engine import command, then the geometry fetched to draw.
    const start = performance.now();
    const reply = await runCommand('surface.import', { path, layer: 'bench' });
    const staged = await stageObject(reply.record.outputs[0].id);
    const opened = performance.now();
    const { group, timings } = await loadStaged(viewport, staged);
    if (opacity !== null && opacity < 1) viewport.setLook(group, { colour: 0xffffff, byRl: true, lines: true, opacity });
    loads.push({
      name: staged.name,
      kind: staged.kind,
      triangles: staged.triangles,
      bytes: staged.bytes,
      import_ms: reply.record.duration_ms,
      encode_ms: staged.timings.encode,
      import_and_encode_ms: opened - start,
      ...timings,
      total_to_frame_submitted_ms: opened - start + timings.to_frame_submitted_ms,
      import_log: reply.record.messages,
    });
  }
  return loads;
}

async function checkViewport(viewport: Viewport, config: BenchConfig, log: (line: string) => void) {
  const base = viewport.baseTriangleCount;
  const steps = [1, ...config.scale_targets.filter((t) => t > base).map((t) => Math.ceil(t / base))];
  const rows = [];
  for (const copies of steps) {
    const triangles = viewport.setCopies(copies);
    viewport.fitView();
    for (let i = 0; i < WARM_UP_FRAMES; i++) await viewport.nextFrame();
    log(`check 1: orbiting ${triangles.toLocaleString()} triangles (${copies} copies)`);
    rows.push({ copies, triangles, ...(await orbit(viewport, config.orbit_seconds)) });
  }
  viewport.setCopies(1);
  viewport.fitView();
  return rows;
}

/** One full turn around the scene while zooming in and out twice; returns frame-interval stats. */
async function orbit(viewport: Viewport, seconds: number): Promise<FrameSummary> {
  const box = viewport.bounds;
  const centre = box.getCenter(new THREE.Vector3());
  const radius = box.getSize(new THREE.Vector3()).length() / 2;
  const elevation = THREE.MathUtils.degToRad(35);
  const intervals: number[] = [];
  viewport.controls.enabled = false;
  await new Promise<void>((resolve) => {
    const start = performance.now();
    let last = -1;
    const hook = (now: number) => {
      const t = (now - start) / 1000;
      if (t > seconds) {
        viewport.beforeRender.delete(hook);
        resolve();
        return;
      }
      if (last >= 0) intervals.push(now - last);
      last = now;
      const angle = (t / seconds) * Math.PI * 2;
      const r = radius * 1.9 * (1 - 0.45 * Math.sin((t / seconds) * Math.PI * 4));
      viewport.camera.position.set(
        centre.x + r * Math.cos(angle) * Math.cos(elevation),
        centre.y + r * Math.sin(angle) * Math.cos(elevation),
        centre.z + r * Math.sin(elevation),
      );
      viewport.controls.target.copy(centre);
    };
    viewport.beforeRender.add(hook);
  });
  viewport.controls.enabled = true;
  return summarise(intervals);
}

/**
 * Each size in both layouts (general triangulation; grid elevations) over both transports
 * (WebView2 shared buffer; raw bytes over IPC). The mesh is generated and encoded in Rust
 * first, so only the hand-over, decode and first frame are timed.
 */
async function checkTransfer(viewport: Viewport, cellSizes: number[], log: (line: string) => void) {
  const transports: Transport[] = defaultTransport === 'shared' ? ['shared', 'ipc'] : ['ipc'];
  const rows = [];
  for (const cells of cellSizes) {
    for (const kind of ['triangulation', 'grid'] as const) {
      for (const transport of transports) {
        const staged = await prepareSynthetic(cells, kind);
        log(
          `check 3: ${staged.triangles.toLocaleString()} triangles as ${kind}, ` +
            `${(staged.bytes / 1e6).toFixed(0)} MB via ${transport}`,
        );
        const { group, timings } = await loadStaged(viewport, staged, { transport });
        rows.push({ cells, kind, triangles: staged.triangles, bytes: staged.bytes, encode_ms: staged.timings.encode, ...timings });
        viewport.removeMesh(group);
        await viewport.nextFrame();
      }
    }
  }
  return rows;
}

async function checkDrag(viewport: Viewport, tool: PolylineTool, updates: number, log: (line: string) => void) {
  log('check 4: building snap index');
  const index = await buildSnapIndex();
  viewport.fitView();
  await viewport.nextFrame();

  tool.clear();
  for (let i = 0; i < 6; i++) {
    const ray = viewport.rayFromNdc(-0.5 + i * 0.2, -0.1);
    const hit = await snapRay(ray.origin, ray.direction);
    if (hit) tool.add(hit.local);
  }
  if (tool.points.length < 3) throw new Error('check 4: could not place a polyline on the surface');

  log(`check 4: dragging a vertex through ${updates} snapped updates (full scene)`);
  const full = await drag(viewport, tool, updates);
  // Same drag with only the light design surface drawn, to separate render cost from snapping
  // (snapping still searches every loaded surface).
  viewport.showOnly((name) => name.startsWith('design'));
  log(`check 4: dragging a vertex through ${updates} snapped updates (design surface only drawn)`);
  const light = await drag(viewport, tool, updates);
  viewport.showOnly();
  return { snap_index: index, polyline_vertices: tool.points.length, full_scene: full, light_scene: light };
}

async function drag(viewport: Viewport, tool: PolylineTool, updates: number) {
  const vertex = 2;
  const [cx, cy] = [-0.5 + vertex * 0.2, -0.1];
  const snapMs: number[] = [];
  const submitMs: number[] = [];
  const gpuMs: number[] = [];
  let misses = 0;
  for (let i = 0; i < updates; i++) {
    const a = (i / updates) * Math.PI * 4;
    const ray = viewport.rayFromNdc(cx + 0.2 * Math.cos(a), cy + 0.2 * Math.sin(a));
    const start = performance.now();
    const hit = await snapRay(ray.origin, ray.direction);
    const snapped = performance.now();
    if (!hit) {
      misses++;
      continue;
    }
    tool.move(vertex, hit.local);
    const frame = await viewport.nextFrame();
    snapMs.push(snapped - start);
    submitMs.push(frame.submitted - start);
    gpuMs.push(frame.gpuDone - start);
  }
  return {
    drawn_triangles: viewport.triangleCount,
    updates: snapMs.length,
    misses,
    snap_round_trip: percentiles(snapMs),
    update_to_frame_submitted: percentiles(submitMs),
    update_to_gpu_done: percentiles(gpuMs),
  };
}

function summarise(intervals: number[]): FrameSummary {
  const p = percentiles(intervals);
  const mean = intervals.reduce((a, b) => a + b, 0) / Math.max(intervals.length, 1);
  return {
    frames: intervals.length,
    mean_fps: 1000 / mean,
    p50_ms: p.p50,
    p95_ms: p.p95,
    p99_ms: p.p99,
    max_ms: p.max,
    frames_over_33ms: intervals.filter((v) => v > 33.4).length,
  };
}

function percentiles(values: number[]) {
  const sorted = [...values].sort((a, b) => a - b);
  const at = (q: number) => (sorted.length ? sorted[Math.min(sorted.length - 1, Math.floor(q * sorted.length))] : NaN);
  return { p50: at(0.5), p95: at(0.95), p99: at(0.99), max: sorted.length ? sorted[sorted.length - 1] : NaN };
}
