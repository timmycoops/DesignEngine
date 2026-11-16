// `--shell`: the panels and the toolbar, as Tim would use them. Panels minimised by their header's button to strips
// at the window's edges and brought back by a name on a strip, kept that way through the layout being saved and read
// again; every panel hidden by the ` key and brought back; and the toolbar's groups, with what came off it in the
// Import menu. Then the docks (decision 0076): tools opened from the toolbar are tabs in one dock beside Properties and
// tables are tabs in one dock under the view, so the view keeps its width; a minimised dock comes back with the tool
// asked for in front; a table fills the window and goes back; and a dock closed and opened again gives its room to the
// view and takes it back, the other docks staying put. Then the view bar (workstream 2): the view has no tab strip and
// takes no panel dropped on its middle; its tools are on a bar over it and off the toolbar, and their buttons and keys
// (T, 2, [, ], E) do their jobs over a made-up topography and block; with a section open the bar never covers the
// section's overview, in a tall view or a short one. With `--pictures <folder>` the window is then drawn in its states
// (a tool and a table open for the Solids, Walls and Reserves work, and a section open), over the project given by
// `--dataflow` if there is one. The layout saved on this machine is put back as it was at the end.
import * as THREE from 'three/webgpu';
import type { DockviewApi } from 'dockview-react';
import { openProject, projectSummary, type ProjectSummary, runCommand, type Vec3, writePicture } from '../bridge';
import type { SectionView } from '../viewport/section';
import { BAR_CLEAR } from '../viewport/sectionGrid';
import type { Viewport } from '../viewport/Viewport';
import { windowPicture } from './windowPicture';
import { LAYOUT_KEYS, type PanelId, resetLayout, showPanel, TOOL_DOCK_WIDTH } from '../workspace/Workspace';

const wait = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));

/** Shows a summary as the app does. */
type Show = (summary: ProjectSummary) => Promise<void> | undefined;

/** A made-up topography at RL 100 and a block standing on it, for the view's tools to work on (grid coordinates). */
const E = 650_000;
const N = 7_450_000;
const BLOCK: Vec3[] = Array.from({ length: 8 }, (_, i) => [E + 400 + (i & 1) * 200, N + 100 + ((i >> 1) & 1) * 200, 100 + ((i >> 2) & 1) * 40] as Vec3);
const BLOCK_FACES = [[0, 2, 3], [0, 3, 1], [4, 5, 7], [4, 7, 6], [0, 1, 5], [0, 5, 4], [2, 6, 7], [2, 7, 3], [0, 4, 6], [0, 6, 2], [1, 3, 7], [1, 7, 5]];

type Box = { left: number; top: number; right: number; bottom: number };
const boxOf = (element: Element | null | undefined): Box | null => {
  if (!element) return null;
  const { left, top, right, bottom } = element.getBoundingClientRect();
  return { left: Math.round(left), top: Math.round(top), right: Math.round(right), bottom: Math.round(bottom) };
};
const apart = (a: Box | null, b: Box | null) => !!a && !!b && (a.right <= b.left || b.right <= a.left || a.bottom <= b.top || b.bottom <= a.top);

export async function checkShell(
  viewport: Viewport,
  log: (line: string) => void,
  dock: () => DockviewApi | null,
  show: Show,
  section: () => SectionView | null,
  pictures: { project: string | null } | null = null,
) {
  const api = dock();
  if (!api) return { error: 'no dock' };
  const results: Record<string, unknown> = {};
  const failures: string[] = [];
  const expect = (what: string, ok: boolean, detail?: unknown) => {
    results[what] = detail === undefined ? ok : { ok, detail };
    if (!ok) failures.push(what);
    log(`shell: ${what}: ${ok ? 'yes' : 'NO'}${detail === undefined ? '' : ` (${JSON.stringify(detail)})`}`);
  };
  const saved = LAYOUT_KEYS.map((key) => [key, localStorage.getItem(key)] as const);
  const canvas = viewport.renderer.domElement;
  const settle = async () => {
    await wait(300);
    await viewport.nextFrame();
  };
  const size = () => ({ width: canvas.clientWidth, height: canvas.clientHeight });
  const buttonLabelled = (label: string) => document.querySelector<HTMLButtonElement>(`button[aria-label="${label}"]`);
  const strip = (edge: string) => [...document.querySelectorAll(`[data-slot="minimised-strip"][data-edge="${edge}"] button`)].map((b) => b.textContent?.trim() ?? '');
  const stripButton = (name: string) => [...document.querySelectorAll<HTMLButtonElement>('[data-slot="minimised-strip"] button')].find((b) => b.textContent?.trim() === name);
  const key = async (name: string) => {
    window.dispatchEvent(new KeyboardEvent('keydown', { key: name, bubbles: true }));
    await settle();
  };

  try {
    if (api.hasMaximizedGroup()) api.exitMaximizedGroup();
    resetLayout(api);
    await settle();
    const start = size();
    log(`shell: the default layout, viewport ${start.width} x ${start.height}`);

    buttonLabelled('Minimise Properties')?.click();
    await settle();
    const noProperties = size();
    expect('minimising Properties gives the viewport its width', noProperties.width > start.width + 200, { before: start.width, after: noProperties.width });
    expect('Properties waits on a strip on the right', strip('right').includes('Properties'), strip('right'));

    buttonLabelled('Minimise Layers')?.click();
    buttonLabelled('Minimise Run log')?.click();
    await settle();
    const bare = size();
    expect('Layers on the left and the run log at the bottom', strip('left').includes('Layers') && strip('bottom').includes('Run log'), { left: strip('left'), bottom: strip('bottom') });
    expect('the viewport takes their room too', bare.width > noProperties.width + 150 && bare.height > noProperties.height + 120, { before: noProperties, after: bare });

    // Read back as the app does when it starts: the groups stay minimised, on the same strips.
    const layout = localStorage.getItem(LAYOUT_KEYS[0]);
    if (layout) {
      api.fromJSON(JSON.parse(layout));
      await settle();
    }
    expect('minimised groups stay so when the layout is read again', strip('right').includes('Properties') && strip('left').includes('Layers') && strip('bottom').includes('Run log'), {
      left: strip('left'),
      right: strip('right'),
      bottom: strip('bottom'),
    });

    stripButton('Properties')?.click();
    await settle();
    const back = size();
    expect('a click on its name brings Properties back at its size', !strip('right').includes('Properties') && Math.abs(bare.width - back.width - (noProperties.width - start.width)) < 4, {
      width_given_back: bare.width - back.width,
      width_it_took: noProperties.width - start.width,
    });

    await key('`');
    const full = size();
    expect('` hides every panel and the strips: the viewport fills the window', full.width >= window.innerWidth - 8 && strip('left').length === 0, { viewport: full, window: [window.innerWidth, window.innerHeight] });
    await key('`');
    // The strips go with the panels, so dockview shares their few pixels out again on the way back: within 10 px.
    expect('` again brings them back as they were', Math.abs(size().width - back.width) <= 10 && strip('left').includes('Layers'), { before: back.width, after: size().width });

    // The docks, from the default layout again.
    resetLayout(api);
    await settle();
    const tool = async (name: string) => {
      [...document.querySelectorAll<HTMLButtonElement>('header button')].find((b) => b.textContent?.trim() === name)?.click();
      await settle();
    };
    const tabs = (id: PanelId) => api.getPanel(id)?.group.panels.map((p) => p.id) ?? [];
    const front = (id: PanelId) => api.getPanel(id)?.group.activePanel?.id;
    const width = (id: PanelId) => api.getPanel(id)?.group.api.width ?? 0;
    const closeTab = async (id: PanelId) => {
      api.getPanel(id)?.api.close();
      await settle();
    };
    const open = size();
    expect('no layout from before the docks is kept', localStorage.getItem('zd.workspace.layout.v3') === null && localStorage.getItem('zd.workspace.minimised.v1') === null);
    expect('the tool dock starts at its width with Properties in it', width('properties') === TOOL_DOCK_WIDTH && tabs('properties').length === 1, { width: width('properties'), tabs: tabs('properties') });

    await tool('Cut');
    const oneTool = size();
    expect('one tool opens as a tab beside Properties: the view keeps its width', oneTool.width === open.width && tabs('cut').includes('properties') && front('cut') === 'cut', { before: open.width, after: oneTool.width, tabs: tabs('cut') });
    await tool('Builder');
    const twoTools = size();
    await tool('Reserves');
    await tool('Workflow');
    const allTools = size();
    expect('two tools, then all four: the view is as wide as with one', twoTools.width === open.width && allTools.width === open.width && tabs('properties').length === 5, {
      one: oneTool.width,
      two: twoTools.width,
      four: allTools.width,
      tabs: tabs('properties'),
    });

    await tool('Blocks');
    const blocksHeight = api.getPanel('blocks')?.group.api.height ?? 0;
    await tool('Reserves table');
    await tool('Parameters');
    await tool('Face');
    showPanel(api, 'canvas');
    await settle();
    const tables = size();
    expect('the tables and Face design are tabs in one row under the view', tabs('runlog').length === 6 && ['blocks', 'restable', 'params', 'face', 'canvas'].every((id) => tabs('runlog').includes(id as PanelId)) && tables.width === open.width, {
      tabs: tabs('runlog'),
      view: tables,
    });
    expect('the results dock grows to the height a table asks for, and no further', blocksHeight === 320 && (api.getPanel('runlog')?.group.api.height ?? 0) === 400, { with_block_table: blocksHeight, with_them_all: api.getPanel('runlog')?.group.api.height });

    // A tool closed, its dock minimised, and the tool asked for from the toolbar.
    await closeTab('builder');
    document.querySelector<HTMLButtonElement>('button[aria-label^="Minimise Properties"]')?.click();
    await settle();
    const toolsAway = strip('right');
    await tool('Builder');
    expect('a minimised tool dock comes back with the tool asked for in front', toolsAway.includes('Cut solids') && strip('right').length === 0 && front('properties') === 'builder' && size().width === open.width, {
      strip_while_minimised: toolsAway,
      front: front('properties'),
      view: size().width,
    });

    // A table fills the window, and goes back.
    buttonLabelled('Fill the window')?.click();
    await settle();
    const dock = api.getPanel('runlog')?.group;
    const filled = { width: dock?.api.width ?? 0, height: dock?.api.height ?? 0 };
    buttonLabelled('Put the tables back under the view')?.click();
    await settle();
    expect('a table fills the window, and goes back under the view', filled.width >= window.innerWidth - 8 && filled.height > tables.height + 300 && size().width === tables.width && size().height === tables.height, { filled, window: [window.innerWidth, window.innerHeight], back: size() });
    buttonLabelled('Fill the window')?.click();
    await settle();
    await key('`');
    const viewOnly = size();
    await key('`');
    expect('` with a table filling the window gives the window to the view, and ` again brings everything back', viewOnly.width >= window.innerWidth - 8 && Math.abs(size().width - tables.width) <= 10 && tabs('runlog').length === 6, { view_only: viewOnly, back: size() });

    // A dock with nothing left in it goes, and the view takes its room; opened again, the view gives it back.
    const layers = width('layers');
    for (const id of ['properties', 'cut', 'builder', 'reserves', 'workflow'] as const) api.getPanel(id)?.api.close();
    await settle();
    const noTools = { view: size().width, layers: width('layers') };
    await tool('Cut');
    expect('the tool dock closed gives its room to the view, and opened again takes it back; Layers stays as it was', noTools.view >= tables.width + TOOL_DOCK_WIDTH - 2 && noTools.layers === layers && width('cut') === TOOL_DOCK_WIDTH && width('layers') === layers && Math.abs(size().width - tables.width) <= 2, {
      view_without_tools: noTools.view,
      view_with_them: size().width,
      layers: [layers, noTools.layers, width('layers')],
      tool_dock: width('cut'),
    });

    // The toolbar: groups, with the imports in one menu.
    const header = [...document.querySelectorAll('header button')].map((b) => b.textContent?.trim() ?? '');
    const gone = ['Surface', 'Solids', 'Strings', 'Deswik', 'Spry', 'Vulcan grid', 'Seam model'].filter((word) => header.includes(word));
    expect('the toolbar has Import, Export and Panels menus, and no import buttons of its own', ['Import', 'Export', 'Panels'].every((word) => header.includes(word)) && gone.length === 0, { still_there: gone });
    const trigger = [...document.querySelectorAll<HTMLButtonElement>('header button')].find((b) => b.textContent?.trim() === 'Import');
    trigger?.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, button: 0, pointerType: 'mouse', ctrlKey: false }));
    await settle();
    const items = [...document.querySelectorAll('[role="menuitem"]')].map((m) => m.textContent?.trim() ?? '');
    const wanted = ['Surfaces…', 'Vulcan grid…', 'Strings…', 'Solids', 'Deswik drawing (.dcf)…', 'Spry project (.spry)…', 'Suggest a stack definition (CSV)…', 'Build and check from a definition…'];
    expect('the Import menu holds every import and the seam model', wanted.every((w) => items.includes(w)), { missing: wanted.filter((w) => !items.includes(w)) });
    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
    await settle();

    // The view bar, from the default layout, over a made-up topography and a block.
    resetLayout(api);
    await settle();
    await runCommand('surface.from_mesh', {
      name: 'Topography',
      layer: 'surfaces',
      vertices: [[E, N, 100], [E + 1000, N, 100], [E + 1000, N + 400, 100], [E, N + 400, 100]],
      triangles: [[0, 1, 2], [0, 2, 3]],
    });
    await runCommand('solid.from_mesh', { name: 'Block', layer: 'blocks', vertices: BLOCK, triangles: BLOCK_FACES });
    const made = await projectSummary();
    await show(made);
    await wait(1500);
    viewport.fitView();
    await settle();
    const origin = made.origin ?? [0, 0, 0];
    const screen = (p: Vec3) => viewport.projectToClient(new THREE.Vector3(p[0] - origin[0], p[1] - origin[1], p[2] - origin[2]));
    const pointer = (type: string, x: number, y: number) =>
      canvas.dispatchEvent(new PointerEvent(type, { clientX: x, clientY: y, button: 0, buttons: type === 'pointerup' ? 0 : 1, pointerId: 1, pointerType: 'mouse', isPrimary: true, bubbles: true, cancelable: true }));
    const click = async (at: [number, number]) => {
      pointer('pointerdown', at[0], at[1]);
      pointer('pointerup', at[0], at[1]);
      await settle();
    };
    const bar = () => document.querySelector<HTMLElement>('[data-slot="view-bar"]');
    const nameOf = (b: Element) => b.getAttribute('aria-label') ?? b.textContent?.trim() ?? '';
    const onBar = () => [...(bar()?.querySelectorAll('button, input') ?? [])].map(nameOf);
    const barButton = (name: string) => [...(bar()?.querySelectorAll<HTMLButtonElement>('button') ?? [])].find((b) => nameOf(b) === name);
    const pressed = (name: string) => barButton(name)?.getAttribute('aria-pressed') === 'true';
    const press = async (name: string) => {
      barButton(name)?.click();
      await settle();
    };
    const footer = () => document.querySelector('footer')?.textContent ?? '';
    const onToolbar = () => [...document.querySelectorAll('header button')].map((b) => b.textContent?.trim() ?? '');

    const viewGroup = api.getPanel('viewport')?.group;
    const tabStrip = viewGroup?.element.querySelector<HTMLElement>('.dv-tabs-and-actions-container');
    expect('the view has no tab strip: it is as tall as its group', !!viewGroup && viewGroup.header.hidden && (tabStrip?.offsetHeight ?? 0) === 0 && size().height === viewGroup.api.height, {
      view: size().height,
      group: viewGroup?.api.height,
      strip: tabStrip?.offsetHeight,
    });
    expect('no panel can be dropped on the middle of the view', viewGroup?.locked === true && viewGroup.panels.length === 1, { locked: viewGroup?.locked });
    const viewTools = ['Select', 'Draw string', 'Snap', '3D', "Zoom to what's selected", 'Zoom to everything', 'Field of view', 'Section'];
    const stillUp = ['Select', 'Draw string', 'Snap', '3D', 'Plan', 'Section'].filter((word) => onToolbar().includes(word));
    expect('the view tools are on the view, and off the toolbar', viewTools.every((name) => onBar().includes(name)) && stillUp.length === 0, { on_the_bar: onBar(), still_on_the_toolbar: stillUp });
    // The row the view tools came off: what it needs, against the window.
    const row = document.querySelector<HTMLElement>('header');
    const parts = [...(row?.children ?? [])].map((c) => c.getBoundingClientRect());
    const tools = parts.slice(0, -1);
    const needs = Math.ceil((tools.at(-1)?.right ?? 0) - (parts[0]?.left ?? 0) + (parts.at(-1)?.width ?? 0) + 16);
    expect('with the view tools off it, the toolbar fits the window: nothing clipped', !!row && row.scrollWidth <= row.clientWidth && needs < window.innerWidth, { needs_px: needs, window_px: window.innerWidth });
    const barBox = boxOf(bar());
    const viewBox = boxOf(canvas);
    expect('the bar is in the top left corner of the view', !!barBox && !!viewBox && barBox.left - viewBox.left === 8 && barBox.top - viewBox.top === 8 && barBox.right < viewBox.right, { bar: barBox, view: viewBox });

    await press('3D');
    const planByButton = viewport.planView && onBar().includes('Plan');
    await press('Plan');
    const backByButton = !viewport.planView && onBar().includes('3D');
    await key('t');
    const planByKey = viewport.planView && pressed('Plan');
    await key('t');
    expect('3D or Plan: the button and T both go into plan and back', planByButton && backByButton && planByKey && !viewport.planView, { planByButton, backByButton, planByKey });

    await press('Draw string');
    const drawing = pressed('Draw string') && !pressed('Select');
    await press('Select');
    const snapWas = pressed('Snap');
    await press('Snap');
    const snapNow = pressed('Snap');
    await press('Snap');
    expect('Select, Draw and Snap say which is on', drawing && pressed('Select') && !pressed('Draw string') && snapWas !== snapNow && pressed('Snap') === snapWas, { drawing, snap: [snapWas, snapNow] });

    const all = viewport.bounds.getCenter(new THREE.Vector3());
    viewport.focusOn([all.x + 300, all.y - 200, all.z], 100);
    await settle();
    const away = viewport.controls.target.distanceTo(all);
    await key('e');
    const byKey = viewport.controls.target.distanceTo(all);
    viewport.focusOn([all.x + 300, all.y - 200, all.z], 100);
    await settle();
    await press('Zoom to everything');
    expect('E and its button zoom to everything', away > 100 && byKey < 1e-6 && viewport.controls.target.distanceTo(all) < 1e-6, { away_m: Number(away.toFixed(1)), by_key_m: byKey, by_button_m: viewport.controls.target.distanceTo(all) });

    // A section: its button starts the picking, two points open it, and its step and fill come onto the bar.
    viewport.setPlanView(true);
    viewport.fitView();
    await settle();
    await press('Section');
    const picking = footer().includes('Section: pick the first point') && pressed('Section');
    await key('2');
    const stopped = footer().startsWith('Select:') && !pressed('Section');
    await key('2');
    await click(screen([E + 100, N + 200, 100]));
    await click(screen([E + 900, N + 200, 100]));
    for (let k = 0; k < 50 && !section()?.open; k++) await wait(100);
    await settle();
    const sectionTools = ['Step back', 'Section step in metres', 'Step on', 'Fill'];
    expect('Section and 2 start picking and stop it; two points open a section, and its step and fill come onto the bar', picking && stopped && !!section()?.open && sectionTools.every((name) => onBar().includes(name)), {
      picking,
      stopped,
      open: !!section()?.open,
      on_the_bar: onBar(),
    });
    const at = () => section()?.offset ?? NaN;
    const from = at();
    await key(']');
    const on = at();
    await key('[');
    const steppedBack = at();
    await press('Step on');
    const onByButton = at();
    await press('Step back');
    expect('] and [ step the section on and back, as their buttons do', on - from === 25 && steppedBack === from && onByButton - from === 25 && at() === from, { from, on, back: steppedBack, by_button: onByButton, back_again: at() });

    const underBar = (boxOf(bar())?.bottom ?? Infinity) - (boxOf(canvas)?.top ?? 0);
    expect("the section's distances are under the bar, not behind it", underBar <= BAR_CLEAR, { bar_ends_px: underBar, distances_from_px: BAR_CLEAR });

    const overview = () => boxOf(section()?.overviewCanvas);
    const tall = { bar: boxOf(bar()), overview: overview(), view: size().height };
    // A view too short for the bar above the overview: the tables given all but 300 px of it.
    const resultsDock = api.getPanel('runlog')?.group;
    const tablesHeight = resultsDock?.api.height ?? 0;
    resultsDock?.api.setSize({ height: tablesHeight + size().height - 300 });
    await settle();
    const short = { bar: boxOf(bar()), overview: overview(), view: size().height };
    resultsDock?.api.setSize({ height: tablesHeight });
    await settle();
    expect(
      "the bar never covers the section's overview: over it in a tall view, beside it in a short one",
      apart(tall.bar, tall.overview) && apart(short.bar, short.overview) && short.view <= 300 && (short.bar?.left ?? 0) >= (short.overview?.right ?? Infinity) && size().height === tall.view,
      { tall, short },
    );
    await key('2');
    expect('2 closes the section, and its step and fill leave the bar', !section()?.open && !onBar().includes('Fill') && !pressed('Section'), { open: !!section()?.open, on_the_bar: onBar() });
    viewport.setPlanView(false);
    await settle();

    if (pictures) {
      if (pictures.project) {
        log(`shell: opening ${pictures.project}`);
        await show(await openProject(pictures.project));
        await wait(1500);
        viewport.fitView();
      }
      const saved: string[] = [];
      const picture = async (name: string, panels: PanelId[]) => {
        resetLayout(api);
        await settle();
        for (const id of panels) showPanel(api, id);
        await wait(1200);
        const path = await writePicture(name, await windowPicture(viewport));
        log(`shell: ${path}`);
        saved.push(path);
      };
      await picture('docks-default', []);
      await picture('docks-solids', ['builder', 'reserves', 'cut', 'blocks']);
      await picture('docks-walls', ['cut', 'builder', 'face']);
      await picture('docks-reserves', ['cut', 'reserves', 'params', 'restable']);
      // A section across the middle of what's on show, from plan, with its step and fill on the bar.
      resetLayout(api);
      viewport.setPlanView(true);
      viewport.fitView();
      await settle();
      await key('2');
      const across = boxOf(canvas)!;
      const mid = (across.top + across.bottom) / 2;
      await click([across.left + 0.3 * (across.right - across.left), mid]);
      await click([across.left + 0.7 * (across.right - across.left), mid]);
      for (let k = 0; k < 80 && !section()?.open; k++) await wait(100);
      if (section()?.open) {
        await wait(1500);
        const path = await writePicture('view-bar-section', await windowPicture(viewport));
        log(`shell: ${path}`);
        saved.push(path);
        await key('2');
      } else {
        log('shell: no section opened for its picture');
      }
      viewport.setPlanView(false);
      results.pictures = saved;
    }
  } finally {
    // This machine's layout, as it was, in the app and in its storage.
    for (const [key, value] of saved) {
      if (value === null) localStorage.removeItem(key);
      else localStorage.setItem(key, value);
    }
    const layout = saved[0][1];
    if (layout) {
      try {
        api.fromJSON(JSON.parse(layout));
      } catch {
        // Stored, so it's what the app opens with next time.
      }
    }
  }
  results.failures = failures;
  return results;
}
