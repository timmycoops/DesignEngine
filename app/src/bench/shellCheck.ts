// `--shell`: the panels and the toolbar, as Tim would use them. Panels minimised by their header's button to strips
// at the window's edges and brought back by a name on a strip, kept that way through the layout being saved and read
// again; every panel hidden by the ` key and brought back; and the toolbar's groups, with what came off it in the
// Import menu. Then the docks (decision 0076): tools opened from the toolbar are tabs in one dock beside Properties and
// tables are tabs in one dock under the view, so the view keeps its width; a minimised dock comes back with the tool
// asked for in front; a table fills the window and goes back; and a dock closed and opened again gives its room to the
// view and takes it back, the other docks staying put. Then the view bar (workstream 2): the view has no tab strip and
// takes no panel dropped on its middle; its tools are on a bar over it and off the toolbar, and their buttons and keys
// (T, 2, [, ], E) do their jobs over a made-up topography and block; with a section open the bar never covers the
// section's overview, in a tall view or a short one. The ribbon (workstream 5): a tab a stage over a row of its tools,
// each row holding its tools in order at 1,600, 1,280 and 900 px with what doesn't fit folded into More and nothing
// clipped; the menus reading as they did; the tool in use lit; the stage remembered; a Strings tool bringing its row in
// Properties into view. Find a command (workstream 6): Ctrl+K from anywhere, a box being typed in included, lists every
// tool; typing part of a name, what a tool does, its command or its stage finds it, and Enter carries it out. Then the
// panels' heads (workstream 3): a tool panel says what it
// does in one line that fits its dock, the rest in a tip, and its name only on its tab; Cut solids' line follows what
// it's set to do and its six choices show whole; and every panel's first control, or its first line where it has none,
// is within 40 px of its tab. With `--pictures <folder>` the window is then drawn in its states
// (a tool and a table open for the Solids, Walls and Reserves work, and a section open), over the project given by
// `--dataflow` if there is one. The layout saved on this machine is put back as it was at the end.
import * as THREE from 'three/webgpu';
import type { DockviewApi } from 'dockview-react';
import { openProject, projectSummary, type ProjectSummary, runCommand, type Vec3, writePicture } from '../bridge';
import type { SectionView } from '../viewport/section';
import { BAR_CLEAR } from '../viewport/sectionGrid';
import type { Viewport } from '../viewport/Viewport';
import { windowPicture } from './windowPicture';
import { rowName, ROWS, STAGES, toolByKey, TOOLS } from '../tools';
import type { StageId } from '../workspace/Ribbon';
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
  const STAGE_KEY = 'zd.ribbon.stage.v1';
  const saved = [...LAYOUT_KEYS, STAGE_KEY].map((key) => [key, localStorage.getItem(key)] as const);
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
    const goStage = async (id: string) => {
      document.querySelector<HTMLButtonElement>(`[role="tab"][data-stage="${id}"]`)?.click();
      await settle();
    };
    const openMenu = async (button: HTMLElement | null | undefined) => {
      button?.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, button: 0, pointerType: 'mouse', ctrlKey: false }));
      await settle();
    };
    const closeMenu = async () => {
      document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
      await settle();
    };
    const menuItems = () => [...document.querySelectorAll<HTMLElement>('[role="menuitem"]')].map((m) => ({ words: m.textContent?.trim() ?? '', off: m.hasAttribute('data-disabled'), tool: m.dataset.tool ?? '' }));
    const moreButton = () => document.querySelector<HTMLButtonElement>('[data-slot="row-more"]');
    /** A click on a tool on the row, or in More where it didn't fit. */
    const clickTool = async (key: string) => {
      const onRow = document.querySelector<HTMLButtonElement>(`[data-slot="ribbon"] [data-tool="${key}"]`);
      if (onRow) onRow.click();
      else {
        await openMenu(moreButton());
        document.querySelector<HTMLElement>(`[role="menuitem"][data-tool="${key}"]`)?.click();
      }
      await settle();
    };
    /** The tools the old one row named, where they are now: a stage's tab, and the tool on its row. */
    const AT: Record<string, [string, string]> = {
      Cut: ['solids', 'cut:strips'],
      Builder: ['walls', 'builder'],
      Face: ['walls', 'face'],
      Reserves: ['reserves', 'reserves'],
      Blocks: ['solids', 'blocks'],
      'Reserves table': ['reserves', 'restable'],
      Parameters: ['reserves', 'params'],
      Workflow: ['', 'workflow'],
    };
    const tool = async (name: string) => {
      const [stage, key] = AT[name];
      if (stage) await goStage(stage);
      await clickTool(key);
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

    // The ribbon (workstream 5): a tab a stage, and under it the row of that stage's tools, named in words.
    resetLayout(api);
    await settle();
    const ribbon = document.querySelector<HTMLElement>('[data-slot="ribbon"]');
    const stageTabs = [...document.querySelectorAll<HTMLButtonElement>('[role="tab"][data-stage]')];
    const stageNow = () => document.querySelector<HTMLElement>('[role="tab"][data-stage][aria-selected="true"]')?.dataset.stage ?? '';
    const ribbonHeight = Math.round(ribbon?.getBoundingClientRect().height ?? 0);
    expect('the ribbon is a tab a stage over a row of tools, 68 px high', stageTabs.map((t) => t.textContent?.trim()).join(' · ') === STAGES.map((st) => st.title).join(' · ') && ribbonHeight === 68, {
      tabs: stageTabs.map((t) => t.textContent?.trim()),
      height: ribbonHeight,
    });

    const rowBox = () => document.querySelector<HTMLElement>('[data-slot="tool-row-tools"]');
    const rowParts = () => [...(rowBox()?.querySelectorAll<HTMLElement>(':scope > [data-row-part]') ?? [])];
    const wordsOf = (part: HTMLElement) => part.querySelector('button')?.textContent?.trim() ?? '';
    const rowWords = (id: StageId) => ROWS[id].filter((item) => item !== '|').map((item) => (typeof item === 'string' ? rowName(toolByKey(item)!) : item.label));
    const appRoot = document.getElementById('root');
    /** Every stage's row at a window width: what shows, what folds into More, and whether anything is clipped. */
    const rowsAt = async (width: number | null) => {
      if (appRoot) appRoot.style.width = width === null ? '' : `${width}px`;
      await settle();
      const rows: Record<string, { shown: number; more: number }> = {};
      const wrong: string[] = [];
      for (const { id } of STAGES) {
        await goStage(id);
        const shown = rowParts().map(wordsOf).filter((w) => w !== '');
        let folded: string[] = [];
        if (moreButton()) {
          await openMenu(moreButton());
          // The first menu is More's; a menu within it shows its name only.
          folded = [...(document.querySelector('[role="menu"]')?.querySelectorAll<HTMLElement>(':scope > [role="menuitem"]') ?? [])].map((m) => m.textContent?.trim() ?? '');
          await closeMenu();
        }
        rows[id] = { shown: shown.length, more: folded.length };
        const edge = (ribbon?.getBoundingClientRect().right ?? 0) + 0.5;
        const rowEdge = (rowBox()?.getBoundingClientRect().right ?? 0) + 0.5;
        const clipped = rowParts().some((part) => part.getBoundingClientRect().right > rowEdge) || (moreButton()?.getBoundingClientRect().right ?? 0) > rowEdge;
        const offScreen = (document.querySelector('[data-slot="panels-menu"]')?.getBoundingClientRect().right ?? Infinity) > edge || (ribbon ? ribbon.scrollWidth > ribbon.clientWidth : true);
        if ([...shown, ...folded].join('|') !== rowWords(id).join('|')) wrong.push(`${id}: the row and More don't hold its tools in order`);
        if (clipped) wrong.push(`${id}: a tool is clipped`);
        if (offScreen) wrong.push(`${id}: off the screen`);
        if (stageNow() !== id) wrong.push(`${id}: its tab is not the one picked`);
      }
      return { rows, wrong, width: Math.round(ribbon?.getBoundingClientRect().width ?? 0) };
    };
    const wide = await rowsAt(null);
    expect("at 1,600 px every stage's row holds its tools in order, nothing clipped or off the screen", wide.wrong.length === 0 && wide.width === 1600, wide);
    const laptop = await rowsAt(1280);
    expect('at 1,280 px the same, what does not fit folded into More', laptop.wrong.length === 0 && laptop.width === 1280, laptop);
    const narrow = await rowsAt(900);
    expect('at 900 px the same, with More on the rows that need it', narrow.wrong.length === 0 && narrow.width === 900 && Object.values(narrow.rows).some((r) => r.more > 0), narrow);
    // A tool in More does what it does on the row.
    await goStage('strings');
    const foldedTools = (await (async () => {
      await openMenu(moreButton());
      const found = menuItems();
      await closeMenu();
      return found;
    })()).filter((m) => m.tool);
    await clickTool('string.set_closed');
    const footerWords = () => document.querySelector('footer')?.textContent ?? '';
    expect('a tool folded into More does what it does on the row', foldedTools.some((m) => m.tool === 'string.set_closed') && footerWords().startsWith('Close: pick a string') && front('properties') === 'properties', {
      in_more: foldedTools.map((m) => m.words),
      said: footerWords().slice(0, 70),
    });
    await rowsAt(null);

    // Switching stage changes the row only: the panels stay where they are.
    const laidOut = JSON.stringify(api.toJSON());
    const viewWas = size();
    for (const { id } of STAGES) await goStage(id);
    await goStage('walls');
    const kept = JSON.parse(localStorage.getItem(STAGE_KEY) ?? '[]') as [string, string][];
    expect('switching stage changes the row only, and the stage is remembered for the project', JSON.stringify(api.toJSON()) === laidOut && size().width === viewWas.width && size().height === viewWas.height && kept.some(([path, stage]) => path === '' && stage === 'walls'), {
      view: [viewWas, size()],
      remembered: kept.filter(([path]) => path === ''),
    });

    // The menus: the project's on its name, Import and Export on the Project row, Panels at the row's end.
    await goStage('project');
    const menuOf = async (button: HTMLElement | null | undefined) => {
      await openMenu(button);
      const found = menuItems();
      await closeMenu();
      return found;
    };
    const rowMenu = (name: string) => document.querySelector<HTMLElement>(`[data-row-menu="${name}"]`);
    const projectMenu = await menuOf(document.querySelector<HTMLElement>('[data-slot="project-menu"]'));
    const importMenu = await menuOf(rowMenu('Import'));
    const exportMenu = await menuOf(rowMenu('Export'));
    const panelsMenu = await menuOf(document.querySelector<HTMLElement>('[data-slot="panels-menu"]'));
    const projectWas = ['New project', 'Open a project (.zdp)…', 'Open an older project folder (.zdproj)…', 'SaveCtrl+S', 'Save as…'];
    const importWas = ['Surfaces…', 'Vulcan grid…', 'Strings…', 'Solids', 'Deswik drawing (.dcf)…', 'Spry project (.spry)…', 'Suggest a stack definition (CSV)…', 'Build and check from a definition…', 'Relink geology…'];
    const exportWas = ['Selected to DXF…', 'Selected strings to CSV…', 'Selected solids to a zip…', 'Everything visible to DXF…', 'Visible strings to CSV…', 'Visible solids to a zip…'];
    const panelsWas = ['Layers', 'Properties', 'Builder', 'Cut solids', 'Reserves', 'Workflow', 'Run log', 'Block table', 'Reserves table', 'Parameters', 'Workflow canvas', 'Face design', 'Hide every panel, or bring them back`', 'Reset layout'];
    const reads = (menu: { words: string }[], was: string[]) => menu.map((m) => m.words).join('|') === was.join('|');
    expect('the project, Import, Export and Panels menus read as they did, what needs a selection off without one', reads(projectMenu, projectWas) && reads(importMenu, importWas) && reads(exportMenu, exportWas) && reads(panelsMenu, panelsWas) && exportMenu.map((m) => m.off).join() === 'true,true,true,false,false,false', {
      project: projectMenu.map((m) => m.words),
      import: importMenu.map((m) => m.words),
      export: exportMenu.map((m) => `${m.words}${m.off ? ' (off)' : ''}`),
      panels: panelsMenu.length,
    });
    await goStage('solids');
    const solidsIn = await menuOf(rowMenu('Import solids'));
    const solidsOut = await menuOf(rowMenu('Export solids'));
    expect("the Solids row's own menus hold the solids' imports and exports", reads(solidsIn, ['A zip or files…', 'A folder…', 'A zip or files…', 'A folder…']) && reads(solidsOut, ['Selected solids to a zip…', 'Visible solids to a zip…']) && solidsOut[0]?.off === true, {
      import: solidsIn.map((m) => m.words),
      export: solidsOut.map((m) => m.words),
    });

    // The tool in use is lit: its panel in front in its dock, set to it.
    const lit = () => [...document.querySelectorAll<HTMLElement>('[data-slot="ribbon"] [data-tool][aria-pressed="true"]')].map((b) => b.dataset.tool ?? '');
    const litAtStart = lit();
    await clickTool('cut:seams');
    const litSeams = lit();
    [...(api.getPanel('cut')?.group.element.querySelectorAll<HTMLButtonElement>('[role="group"][aria-label="What to do"] button') ?? [])].find((b) => b.textContent?.trim() === 'By benches')?.click();
    await settle();
    const litBenches = lit();
    await clickTool('blocks');
    const litBlocks = lit();
    document.querySelector<HTMLButtonElement>('button[aria-label^="Minimise Run log"]')?.click();
    await settle();
    const litMinimised = lit();
    expect("the tool in use is lit: Cut solids' as its choice changes, the Block table's until its dock is minimised", litAtStart.length === 0 && litSeams.join() === 'cut:seams' && litBenches.join() === 'cut:benches' && litBlocks.join() === 'cut:benches,blocks' && litMinimised.join() === 'cut:benches', {
      at_start: litAtStart,
      by_seams: litSeams,
      by_benches_picked_in_the_panel: litBenches,
      block_table_open: litBlocks,
      its_dock_minimised: litMinimised,
    });

    // Find a command (workstream 6): Ctrl+K from anywhere, type, Enter.
    resetLayout(api);
    await settle();
    await goStage('project');
    const palette = () => document.querySelector<HTMLElement>('[cmdk-root]');
    const findInput = () => document.querySelector<HTMLInputElement>('[cmdk-input]');
    const found = () => [...document.querySelectorAll<HTMLElement>('[cmdk-item]')].map((i) => ({ tool: i.dataset.tool ?? '', off: i.getAttribute('aria-disabled') === 'true', picked: i.getAttribute('aria-selected') === 'true' }));
    const ctrlK = async (from: EventTarget = window) => {
      from.dispatchEvent(new KeyboardEvent('keydown', { key: 'k', ctrlKey: true, bubbles: true }));
      await settle();
    };
    const typeFind = async (text: string) => {
      const input = findInput();
      if (!input) return;
      Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value')?.set?.call(input, text);
      input.dispatchEvent(new Event('input', { bubbles: true }));
      await settle();
    };
    const enter = async () => {
      findInput()?.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }));
      await settle();
    };
    const litNow = () => [...document.querySelectorAll<HTMLElement>('[data-slot="ribbon"] [data-tool][aria-pressed="true"]')].map((b) => b.dataset.tool ?? '');

    await ctrlK();
    const listed = found();
    const typingThere = document.activeElement === findInput();
    await closeMenu();
    const closedByEsc = palette() === null;
    document.querySelector<HTMLButtonElement>('[data-slot="find-button"]')?.click();
    await settle();
    const openedByButton = palette() !== null;
    await closeMenu();
    expect('Ctrl+K, or its box on the ribbon, opens Find a command on every tool in the list; Esc closes it', listed.length === TOOLS.length && new Set(listed.map((f) => f.tool)).size === TOOLS.length && typingThere && closedByEsc && openedByButton && palette() === null, {
      listed: listed.length,
      in_the_list: TOOLS.length,
      typingThere,
      closedByEsc,
      openedByButton,
    });

    await ctrlK();
    await typeFind('seams');
    const forSeams = found();
    await enter();
    const afterSeams = { open: palette() !== null, lit: litNow(), front: front('cut'), stage: stageNow() };
    expect('typing part of a name and Enter carries the tool out, from another stage, which stays', forSeams.find((f) => f.picked)?.tool === 'cut:seams' && !afterSeams.open && afterSeams.front === 'cut' && afterSeams.stage === 'project', {
      found: forSeams.map((f) => f.tool),
      after: afterSeams,
    });
    await goStage('solids');
    const litOnItsRow = litNow();
    await goStage('project');

    await ctrlK();
    await typeFind('polygons');
    const forPolygons = found().map((f) => f.tool);
    await typeFind('solid.cut_by_benches');
    const forCommand = found();
    await typeFind('geology');
    const forStage = found().map((f) => f.tool);
    await typeFind('selected solids');
    const forSelected = found();
    await typeFind('no such tool anywhere');
    const forNothing = { found: found().length, says: document.querySelector('[cmdk-empty]')?.textContent ?? '' };
    await closeMenu();
    expect(
      'a tool is found by what it does, its engine command or its stage; one that needs a selection is listed but off',
      forPolygons.includes('cut:polygons') &&
        forPolygons.includes('string.polygons_from_lines') &&
        forCommand.find((f) => f.picked)?.tool === 'cut:benches' &&
        ['import.vulcan_grid', 'seam.suggest', 'seam.build', 'seam.relink'].every((key) => forStage.includes(key)) &&
        forSelected.find((f) => f.tool === 'export.selected_solids')?.off === true &&
        forNothing.found === 0 &&
        forNothing.says !== '' &&
        litOnItsRow.join() === 'cut:seams',
      { polygons: forPolygons, by_command: forCommand.find((f) => f.picked)?.tool, geology: forStage.length, selected_solids_off: forSelected.find((f) => f.tool === 'export.selected_solids')?.off, nothing: forNothing, lit_on_its_row: litOnItsRow },
    );

    // From a box being typed in, and a view tool.
    const box = document.querySelector<HTMLInputElement>('.dv-content-container input');
    box?.focus();
    await ctrlK(box ?? window);
    const fromABox = palette() !== null && !!box;
    await typeFind('plan or 3d');
    await enter();
    const inPlan = viewport.planView;
    await ctrlK();
    await typeFind('plan or 3d');
    await enter();
    expect('Ctrl+K opens it from a box being typed in, and it reaches the tools on the view: Plan, and back to 3D', fromABox && inPlan && !viewport.planView && palette() === null, { fromABox, inPlan, back: !viewport.planView });

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

    // The Strings tab: its tools open Properties at their rows, on the string picked. And Repair, with nothing to repair.
    resetLayout(api);
    await settle();
    await goStage('strings');
    await clickTool('string.offset');
    const noString = footer();
    await runCommand('string.create', { name: 'Bench toe', layer: 'toes', closed: false, points: [[E + 100, N + 50, 100], [E + 900, N + 50, 100]] });
    await show(await projectSummary());
    await wait(1000);
    viewport.setPlanView(true);
    viewport.fitView();
    await settle();
    await click(screen([E + 500, N + 50, 100]));
    await wait(800);
    const stringPicked = !!document.querySelector('[data-string-tool="string.offset"]');
    await clickTool('string.smooth');
    await wait(300);
    const rowLit = document.querySelector('[data-string-tool="string.smooth"]')?.classList.contains('tool-asked') ?? false;
    expect('a Strings tool with no string picked says so; with one picked it brings its row in Properties into view, lit', noString.startsWith('Offset: pick a string') && stringPicked && rowLit && !footer().startsWith('Smooth: pick a string'), {
      with_none: noString.slice(0, 60),
      string_picked: stringPicked,
      row_lit: rowLit,
    });
    viewport.setPlanView(false);
    await goStage('solids');
    await clickTool('solids.repair');
    expect('Repair, with no closed mesh kept as a surface, says there is nothing to repair', footer().startsWith('Nothing to repair'), footer().slice(0, 70));

    // The panels' heads, every panel open in its dock, with nothing picked.
    await key('Escape');
    resetLayout(api);
    await settle();
    const everyPanel = ['builder', 'cut', 'reserves', 'workflow', 'blocks', 'restable', 'params', 'canvas', 'face'] as const;
    for (const id of everyPanel) showPanel(api, id);
    await settle();
    const toFront = async (id: PanelId) => {
      api.getPanel(id)?.api.setActive();
      await settle();
    };
    const groupOf = (id: PanelId) => api.getPanel(id)?.group.element;
    const stripOf = (id: PanelId) => groupOf(id)?.querySelector<HTMLElement>('.dv-tabs-and-actions-container') ?? null;
    const shown = (e: Element) => e.getClientRects().length > 0;
    /** What's in a panel under its tabs, in the order it's read. */
    const inPanel = (id: PanelId, what: string) => [...(groupOf(id)?.querySelectorAll<HTMLElement>(what) ?? [])].filter((e) => !stripOf(id)?.contains(e) && shown(e));
    const headOf = (id: PanelId) => inPanel(id, '[data-slot="panel-head"]')[0] ?? null;
    const lineOf = (id: PanelId) => headOf(id)?.querySelector<HTMLElement>('span') ?? null;
    /** How far under its tabs a panel's first control is, or its first line of words where it has no control (px). */
    const firstAt = (id: PanelId) => {
      const top = stripOf(id)?.getBoundingClientRect().bottom ?? 0;
      const control = inPanel(id, 'button, input, select').find((e) => !e.closest('[data-slot="panel-head"]'));
      const words = inPanel(id, 'p, div, span, th, td, h3').find((e) => !e.closest('[data-slot="panel-head"]') && e.children.length === 0 && (e.textContent?.trim() ?? '') !== '');
      const first = control ?? words;
      return first ? Math.round(first.getBoundingClientRect().top - top) : null;
    };
    const toolPanels = ['builder', 'cut', 'reserves', 'workflow'] as const;
    const titles: Record<(typeof toolPanels)[number], string> = { builder: 'Builder', cut: 'Cut solids', reserves: 'Reserves', workflow: 'Workflow' };
    const heads: Record<string, unknown> = {};
    let headsOk = true;
    for (const id of toolPanels) {
      await toFront(id);
      const head = headOf(id);
      const line = lineOf(id);
      const words = line?.textContent?.trim() ?? '';
      const height = head ? Math.round(head.getBoundingClientRect().height) : 0;
      const whole = !!line && line.scrollWidth <= line.clientWidth;
      const named = inPanel(id, '*').some((e) => e.children.length === 0 && e.textContent?.trim() === titles[id] && !e.closest('label, select, option, button'));
      heads[id] = { line: words, height, whole, name_again: named };
      headsOk &&= !!head && height === 28 && whole && words !== '' && !named;
    }
    expect('a tool panel says what it does in one line that fits, and its name only on its tab', headsOk, heads);

    await toFront('builder');
    const mark = headOf('builder')?.querySelector<HTMLButtonElement>('[data-slot="panel-head-more"]');
    mark?.focus();
    await settle();
    const tip = document.querySelector('[data-slot="tooltip-content"]')?.textContent ?? '';
    mark?.blur();
    await settle();
    expect('the rest of what a tool does is in a tip at the end of the line', !!mark && tip.includes('Rebuild from here') && (lineOf('builder')?.title ?? '').includes('Rebuild from here'), { tip: tip.slice(0, 140) });

    await toFront('cut');
    const choice = groupOf('cut')?.querySelector<HTMLElement>('[role="group"][aria-label="What to do"]');
    const choices = [...(choice?.querySelectorAll<HTMLButtonElement>('button') ?? [])];
    const lines: string[] = [];
    let linesFit = true;
    for (const b of choices) {
      b.click();
      await settle();
      const line = lineOf('cut');
      lines.push(line?.textContent?.trim() ?? '');
      linesFit &&= !!line && line.scrollWidth <= line.clientWidth;
    }
    choices[0]?.click();
    await settle();
    expect("Cut solids' line says what it's set to do, a line a choice, each fitting the dock", lines.length === 6 && new Set(lines).size === 6 && linesFit, { lines });
    const rows = new Set(choices.map((b) => Math.round(b.getBoundingClientRect().top))).size;
    const cutShort = choices.filter((b) => b.scrollWidth > b.clientWidth).map((b) => b.textContent?.trim());
    const choiceBox = boxOf(choice);
    const cutBox = boxOf(groupOf('cut'));
    expect("Cut solids' six choices show whole in the dock, three to a row", choices.length === 6 && rows === 2 && cutShort.length === 0 && !!choiceBox && !!cutBox && choiceBox.right <= cutBox.right && choiceBox.left >= cutBox.left, {
      rows,
      cut_short: cutShort,
      choice: choiceBox,
      dock: cutBox,
    });

    const firsts: Record<string, number | null> = {};
    for (const id of [...everyPanel, 'runlog', 'properties', 'layers'] as const) {
      await toFront(id);
      firsts[id] = firstAt(id);
    }
    const far = Object.entries(firsts).filter(([, px]) => px === null || px > 40);
    expect("every panel's first control, or its first line, is within 40 px of its tab", far.length === 0, firsts);

    if (pictures) {
      if (pictures.project) {
        log(`shell: opening ${pictures.project}`);
        await show(await openProject(pictures.project));
        await wait(1500);
        viewport.fitView();
      }
      const saved: string[] = [];
      const picture = async (name: string, stage: StageId, panels: PanelId[], tools: string[] = []) => {
        resetLayout(api);
        await settle();
        await goStage(stage);
        for (const id of panels) showPanel(api, id);
        for (const key of tools) await clickTool(key);
        await wait(1200);
        const path = await writePicture(name, await windowPicture(viewport));
        log(`shell: ${path}`);
        saved.push(path);
      };
      await picture('docks-default', 'project', []);
      await picture('docks-solids', 'solids', ['builder', 'reserves'], ['cut:seams', 'blocks']);
      await picture('docks-walls', 'walls', ['cut'], ['builder', 'face']);
      await picture('docks-reserves', 'reserves', ['cut', 'params'], ['reserves', 'restable']);
      await picture('ribbon-strings', 'strings', []);
      await picture('ribbon-geology', 'geology', []);
      // Find a command, open on what "seam" finds.
      resetLayout(api);
      await settle();
      await goStage('solids');
      await ctrlK();
      await typeFind('seam');
      await wait(600);
      const findPicture = await writePicture('find-command', await windowPicture(viewport));
      log(`shell: ${findPicture}`);
      saved.push(findPicture);
      await closeMenu();
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
