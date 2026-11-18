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
// Properties into view (until workstream 7: now the Strings panel, set to the tool, run from its one button on the
// string picked, with Properties showing only). Find a command (workstream 6): Ctrl+K from anywhere, a box being typed in included, lists every
// tool; typing part of a name, what a tool does, its command or its stage finds it, and Enter carries it out. Then the
// panels' heads (workstream 3): a tool panel says what it
// does in one line that fits its dock, the rest in a tip, and its name only on its tab; Cut solids' line follows what
// it's set to do and its six choices show whole; and every panel's first control, or its first line where it has none,
// is within 40 px of its tab. With `--pictures <folder>` the window is then drawn in its states
// (a tool and a table open for the Solids, Walls and Reserves work, and a section open), over the project given by
// `--dataflow` if there is one. The layout saved on this machine is put back as it was at the end.
import * as THREE from 'three/webgpu';
import type { DockviewApi } from 'dockview-react';
import { objectDetails, openProject, projectSummary, type ProjectSummary, runCommand, type Vec3, writePicture } from '../bridge';
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
    await clickTool('strings:set_closed');
    const stringsPanel = () => api.getPanel('strings')?.group.element.querySelector<HTMLElement>('[data-tool-panel="strings"]') ?? null;
    const stringTool = () => stringsPanel()?.querySelector<HTMLSelectElement>('select[aria-label="String tool"]')?.value ?? '';
    expect('a tool folded into More does what it does on the row', foldedTools.some((m) => m.tool === 'strings:set_closed') && front('strings') === 'strings' && stringTool() === 'set_closed', {
      in_more: foldedTools.map((m) => m.words),
      strings_panel_set_to: stringTool(),
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

    // The project's name is no menu: a click opens the Project tab, whose row holds New, Open, Save as, Import and Export.
    await goStage('solids');
    document.querySelector<HTMLElement>('[data-slot="project-name"]')?.click();
    for (let k = 0; k < 20 && document.querySelector('[role="tab"][aria-selected="true"]')?.getAttribute('data-stage') !== 'project'; k++) await wait(50);
    expect(
      "the project's name opens the Project tab, and is no menu of its own",
      document.querySelector('[role="tab"][aria-selected="true"]')?.getAttribute('data-stage') === 'project' && !document.querySelector('[data-slot="project-menu"]'),
    );

    // The menus: Open, Import and Export on the Project row, Panels at the row's end.
    await goStage('project');
    const menuOf = async (button: HTMLElement | null | undefined) => {
      await openMenu(button);
      const found = menuItems();
      await closeMenu();
      return found;
    };
    const rowMenu = (name: string) => document.querySelector<HTMLElement>(`[data-row-menu="${name}"]`);
    const projectMenu = await menuOf(rowMenu('Open'));
    const importMenu = await menuOf(rowMenu('Import'));
    const exportMenu = await menuOf(rowMenu('Export'));
    const panelsMenu = await menuOf(document.querySelector<HTMLElement>('[data-slot="panels-menu"]'));
    const projectWas = ['A project (.zdp)…', 'An older project folder (.zdproj)…'];
    const importWas = ['Surfaces…', 'Vulcan grid…', 'Strings…', 'Solids', 'Deswik drawing (.dcf)…', 'Spry project (.spry)…', 'Suggest a stack definition (CSV)…', 'Build and check from a definition…', 'Relink geology…'];
    const exportWas = ['Selected to DXF…', 'Selected strings to CSV…', 'Selected solids to a zip…', 'Everything visible to DXF…', 'Visible strings to CSV…', 'Visible solids to a zip…'];
    const panelsWas = ['Layers', 'Properties', 'Strings', 'Builder', 'Cut solids', 'Reserves', 'Workflow', 'Run log', 'Block table', 'Reserves table', 'Dataflow', 'Parameters', 'Workflow canvas', 'Face design', 'Hide every panel, or bring them back`', 'Reset layout'];
    const reads = (menu: { words: string }[], was: string[]) => menu.map((m) => m.words).join('|') === was.join('|');
    expect('the Open, Import, Export and Panels menus read as they should, what needs a selection off without one', reads(projectMenu, projectWas) && reads(importMenu, importWas) && reads(exportMenu, exportWas) && reads(panelsMenu, panelsWas) && exportMenu.map((m) => m.off).join() === 'true,true,true,false,false,false', {
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
        forPolygons.includes('strings:polygons_from_lines') &&
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

    // The Strings tab (workstream 7): its tools open the Strings panel set to them, on the string picked, and run from
    // its one button. Properties shows only.
    resetLayout(api);
    await settle();
    await goStage('strings');
    await clickTool('strings:offset');
    const stringsNow = () => api.getPanel('strings')?.group.element.querySelector<HTMLElement>('[data-tool-panel="strings"]') ?? null;
    const setTo = () => stringsNow()?.querySelector<HTMLSelectElement>('select[aria-label="String tool"]')?.value ?? '';
    const stringsOn = () => stringsNow()?.querySelector('[data-slot="strings-on"]')?.textContent?.trim() ?? '';
    const goButton = () => [...(stringsNow()?.querySelectorAll<HTMLButtonElement>('button') ?? [])].at(-1) ?? null;
    const typeNumber = async (label: string, value: string) => {
      const input = stringsNow()?.querySelector<HTMLInputElement>(`input[aria-label="${label}"]`);
      if (!input) return false;
      Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value')?.set?.call(input, value);
      input.dispatchEvent(new Event('input', { bubbles: true }));
      await settle();
      return true;
    };
    const stringsNamed = async (start: string) => (await projectSummary()).objects.filter((o) => o.kind === 'string' && o.name.startsWith(start));
    const noneYet = { front: front('strings'), set_to: setTo(), on: stringsOn(), off: goButton()?.disabled ?? null, lit: lit() };
    expect('a Strings tool opens the Strings panel set to it, lit on the row; with no string picked the panel says so and its button is off', noneYet.front === 'strings' && noneYet.set_to === 'offset' && noneYet.on === 'No string picked' && noneYet.off === true && noneYet.lit.join() === 'strings:offset', noneYet);

    await runCommand('string.create', { name: 'Bench toe', layer: 'toes', closed: false, points: [[E + 100, N + 50, 100], [E + 900, N + 50, 100]] });
    await show(await projectSummary());
    await wait(1000);
    viewport.setPlanView(true);
    viewport.fitView();
    await settle();
    await click(screen([E + 500, N + 50, 100]));
    await wait(800);
    const onString = stringsOn();
    const offBeforeDistance = goButton()?.disabled ?? null;
    const typed = (await typeNumber('Offset distance', '40')) && (await typeNumber('How many offsets', '3'));
    const offsetSays = goButton()?.textContent?.trim() ?? '';
    const before = (await stringsNamed('Bench toe')).length;
    goButton()?.click();
    await wait(1500);
    const after = (await stringsNamed('Bench toe')).length;
    expect('with a string picked, Offset takes a distance and a number of times and makes that many strings from its one button', onString === 'Bench toe' && offBeforeDistance === true && typed && offsetSays === 'Offset Bench toe 3 times' && before === 1 && after === 4, {
      on: onString,
      off_before_a_distance: offBeforeDistance,
      button: offsetSays,
      strings_before: before,
      strings_after: after,
    });

    // Another tool from the row: the panel changes to it, the row's light follows, and it runs on the same string.
    await click(screen([E + 500, N + 50, 100]));
    await wait(600);
    await clickTool('strings:set_elevation');
    const litForRl = lit();
    await typeNumber('RL for every vertex', '120');
    const rlSays = goButton()?.textContent?.trim() ?? '';
    goButton()?.click();
    await wait(1200);
    const toe = (await stringsNamed('Bench toe')).find((o) => o.name === 'Bench toe');
    const measures = toe ? (await objectDetails(toe.id)).measures : null;
    const rls = measures?.kind === 'string' ? measures.points.map((p) => p[2]) : [];
    // The panel's own choice changes the tool too, and the row's light follows it.
    const chooser = stringsNow()?.querySelector<HTMLSelectElement>('select[aria-label="String tool"]');
    if (chooser) {
      Object.getOwnPropertyDescriptor(HTMLSelectElement.prototype, 'value')?.set?.call(chooser, 'reverse');
      chooser.dispatchEvent(new Event('change', { bubbles: true }));
    }
    await settle();
    const litForReverse = lit();
    expect("Set elevation sets the string's RLs, and the row's light follows the panel's choice", litForRl.join() === 'strings:set_elevation' && rlSays === 'Set Bench toe to RL 120' && rls.length === 2 && rls.every((z) => z === 120) && litForReverse.join() === 'strings:reverse', {
      lit: litForRl,
      button: rlSays,
      rls,
      lit_after_choosing_reverse: litForReverse,
    });

    // Properties shows only: no string command is run from it.
    showPanel(api, 'properties');
    await settle();
    const properties = api.getPanel('properties')?.group.element ?? null;
    const propertyButtons = [...(properties?.querySelectorAll<HTMLButtonElement>('.dv-content-container button') ?? [])].map((b) => b.textContent?.trim() ?? '');
    const commandWords = ['Apply', 'Reverse', 'Close', 'Open', 'Draw across', 'Offset'];
    expect('Properties shows the string and runs no string command', (properties?.textContent ?? '').includes('Length (plan)') && !(properties?.textContent ?? '').includes('String tools') && propertyButtons.every((b) => !commandWords.some((w) => b.startsWith(w))), {
      buttons: propertyButtons,
    });
    // Several strings as one run (decision 0078): every one of the twelve tools, on three made-up toes north of the
    // block. Picked once, by a click and two Ctrl-clicks, and each tool run from the panel's one button.
    const toeY = [320, 345, 370];
    for (const [k, y] of toeY.entries()) {
      await runCommand('string.create', { name: `Many ${k + 1}`, layer: 'many toes', closed: false, points: [[E + 100, N + y, 100], [E + 500, N + y + 12, 100], [E + 900, N + y, 100]] });
    }
    await show(await projectSummary());
    await wait(1000);
    viewport.fitView();
    await settle();
    const manyIds = (await stringsNamed('Many ')).map((o) => o.id);
    const ctrlClick = async (at: [number, number]) => {
      for (const type of ['pointerdown', 'pointerup']) {
        canvas.dispatchEvent(new PointerEvent(type, { clientX: at[0], clientY: at[1], button: 0, buttons: type === 'pointerup' ? 0 : 1, pointerId: 1, pointerType: 'mouse', isPrimary: true, bubbles: true, cancelable: true, ctrlKey: true }));
      }
      await settle();
    };
    // On each toe 40 m along its first leg, where smoothing moves it least.
    const toeAt = (k: number) => screen([E + 140, N + toeY[k] + 1.2, 100]);
    const pickToes = async () => {
      await click(toeAt(0));
      await ctrlClick(toeAt(1));
      await ctrlClick(toeAt(2));
      await wait(500);
    };
    const choose = async (name: string, value: string) => {
      const select = stringsNow()?.querySelector<HTMLSelectElement>(`select[aria-label="${name}"]`);
      if (!select) return false;
      Object.getOwnPropertyDescriptor(HTMLSelectElement.prototype, 'value')?.set?.call(select, value);
      select.dispatchEvent(new Event('change', { bubbles: true }));
      await settle();
      return true;
    };
    const useTool = (mode: string) => choose('String tool', mode);
    const go = async (ms = 1200) => {
      const says = goButton()?.textContent?.trim() ?? '';
      goButton()?.click();
      await wait(ms);
      return says;
    };
    const many = async () => {
      const lines = [];
      for (const id of manyIds) {
        const measures = (await objectDetails(id)).measures;
        if (measures?.kind === 'string') lines.push(measures);
      }
      return lines;
    };
    const counts = async () => (await many()).map((m) => m.points.length);

    await goStage('strings');
    await clickTool('strings:set_elevation');
    await pickToes();
    const onMany = stringsOn();
    const namesUnder = (stringsNow()?.querySelector('[data-slot="strings-on-names"]')?.textContent?.trim() ?? '').split(', ').sort().join(', ');
    await typeNumber('RL for every vertex', '110');
    const rlMany = await go();
    const atRl = (await many()).map((m) => m.points.every((q) => q[2] === 110));
    const saidOnce = (stringsNow()?.textContent ?? '').includes('3 of 3 strings');
    expect(
      'with three strings picked a tool works on all three as one run: the panel says how many and names them, and Set elevation sets every vertex of the three',
      manyIds.length === 3 && onMany === '3 strings' && namesUnder === 'Many 1, Many 2, Many 3' && rlMany === 'Set 3 strings to RL 110' && atRl.length === 3 && atRl.every(Boolean) && saidOnce,
      { strings: manyIds.length, on: onMany, names: namesUnder, button: rlMany, at_rl_110: atRl, says_3_of_3: saidOnce },
    );
    if (pictures) log(`shell: ${await writePicture('strings-many', await windowPicture(viewport))}`);

    await useTool('set_grade');
    const along = !!stringsNow()?.querySelector('[data-slot="strings-grade-along"]') && !stringsNow()?.querySelector('input[aria-label="Start vertex"]');
    await typeNumber('Grade', '2');
    const gradeSays = await go();
    const graded = (await many()).map((m) => m.points[0][2] === 110 && (m.points.at(-1)?.[2] ?? 0) > 125);
    await useTool('drape');
    const drapeSays = await go(1500);
    const draped = (await many()).map((m) => m.points.every((q) => Math.abs(q[2] - 100) < 1e-6));
    const asDraped = await counts();
    await useTool('smooth');
    const smoothSays = await go();
    const smoothed = await counts();
    await useTool('filter');
    await typeNumber('Distance from the line', '1');
    const filterSays = await go();
    const filtered = await counts();
    await useTool('reverse');
    const reverseSays = await go();
    const reversed = (await many()).map((m) => m.points[0][0] > (m.points.at(-1)?.[0] ?? Infinity));
    expect(
      'Grade, Drape, Smooth, Filter and Reverse each run on the three picked: graded first vertex to last with no vertex to choose, draped to the surface, vertices added then taken out, and turned round',
      along &&
        [gradeSays, drapeSays, smoothSays, filterSays, reverseSays].join() === 'Grade 3 strings,Drape 3 strings,Smooth 3 strings,Filter 3 strings,Reverse 3 strings' &&
        graded.length === 3 && graded.every(Boolean) && draped.every(Boolean) && reversed.every(Boolean) &&
        smoothed.every((n, k) => n > asDraped[k]) && filtered.every((n, k) => n < smoothed[k]),
      { no_vertex_to_choose: along, buttons: [gradeSays, drapeSays, smoothSays, filterSays, reverseSays], graded, draped, vertices: { draped: asDraped, smoothed, filtered }, reversed },
    );

    await useTool('set_closed');
    const closeSays = await go();
    const closed = (await many()).map((m) => m.closed);
    const openSays = await go();
    const opened = (await many()).map((m) => !m.closed);
    expect('Close closes the three picked, and then opens them', closeSays === 'Close 3 strings' && closed.length === 3 && closed.every(Boolean) && openSays === 'Open 3 strings' && opened.every(Boolean), {
      buttons: [closeSays, openSays],
      closed,
      opened,
    });

    // Split works at a vertex, on one string, the last picked. Join puts the pieces picked back into one (decision
    // 0078, workstream 3): no string to choose, and the one picked last keeps the result.
    await useTool('split');
    const last = stringsOn();
    const lastId = (await stringsNamed(last)).find((o) => o.name === last)?.id;
    const pointsOf = async (id: string | undefined) => {
      const measures = id ? (await objectDetails(id)).measures : null;
      return measures?.kind === 'string' ? measures.points : [];
    };
    /** The middle of a string's longest leg, on the screen: a place to click that's on it and nothing else. */
    const onIt = (points: number[][]) => {
      let longest = 0;
      for (let k = 1; k + 1 < points.length; k++) {
        const leg = (j: number) => Math.hypot(points[j + 1][0] - points[j][0], points[j + 1][1] - points[j][1]);
        if (leg(k) > leg(longest)) longest = k;
      }
      const [a, b] = [points[longest], points[longest + 1]];
      return screen([(a[0] + b[0]) / 2, (a[1] + b[1]) / 2, (a[2] + b[2]) / 2]);
    };
    const whole = (await pointsOf(lastId)).length;
    // Split splits at a click by default (String tools plan, workstream 1): a vertex by number is the other choice.
    [...(stringsNow()?.querySelectorAll<HTMLButtonElement>('[role="group"][aria-label="Where the string splits"] button') ?? [])].find((b) => b.textContent?.trim() === 'A vertex')?.click();
    await settle();
    await typeNumber('Vertex to split at', '2');
    const splitSays = await go();
    const part = (await stringsNamed(`${last} part 2`))[0];
    const [partPoints, toePoints] = [await pointsOf(part?.id), await pointsOf(lastId)];
    if (partPoints.length > 1 && toePoints.length > 1) {
      await click(onIt(partPoints));
      await ctrlClick(onIt(toePoints));
      await wait(500);
    }
    await useTool('join');
    const joinOn = stringsOn();
    const noChooser = !stringsNow()?.querySelector('select[aria-label="String to join on"]');
    const joinSays = await go();
    const partsLeft = (await stringsNamed(`${last} part 2`)).length;
    const joined = (await pointsOf(lastId)).length;
    const joinSaid = (stringsNow()?.textContent ?? '').includes(`joined 2 strings into '${last}'`);
    expect(
      'Split works on one string, the last of the three picked, at a vertex; Join then joins the two pieces picked back into one with no string to choose, the one picked last keeping the result',
      last.startsWith('Many ') && splitSays === `Split ${last}` && !!part && toePoints.length === 2 && joinOn === '2 strings' && noChooser && joinSays === 'Join 2 strings into one' && partsLeft === 0 && whole > 2 && joined === whole && joinSaid,
      { on: last, buttons: [splitSays, joinSays], second_part_made: !!part, join_on: joinOn, no_string_to_choose: noChooser, second_parts_left: partsLeft, vertices: { whole, split: toePoints.length, joined }, says_joined: joinSaid },
    );
    // The three toes picked again, each where it lies now.
    for (const [k, id] of manyIds.entries()) {
      const points = await pointsOf(id);
      if (points.length < 2) continue;
      if (k === 0) await click(onIt(points));
      else await ctrlClick(onIt(points));
    }
    await wait(500);

    // Add to workflow (decision 0078, workstream 4): while the Workflow panel adds steps, the Strings panel's button adds
    // its tool as a step and runs nothing, the three toes picked found by their layer, the only strings on it; Number
    // across asks for its line across first, which goes in as a setting. Stop adding, and the button runs again.
    const flowId = (await runCommand('workflow.save', { name: 'Toes', content: { steps: [], cases: [] } })).record.outputs[0]?.id ?? '';
    await show(await projectSummary());
    showPanel(api, 'workflow');
    await settle();
    document.querySelector<HTMLElement>('[aria-label="Add steps from the tool panels: their button reads Add to workflow"]')?.click();
    await settle();
    showPanel(api, 'strings');
    await settle();
    await useTool('smooth');
    const countsBefore = (await counts()).join();
    const addSmooth = await go();
    await useTool('number_across');
    await typeNumber('The first number', '1');
    const addAcross = goButton()?.textContent?.trim() ?? '';
    goButton()?.click();
    await wait(500);
    await click(screen([E + 300, N + 300, 100]));
    await wait(400);
    await click(screen([E + 300, N + 396, 100]));
    await wait(1500);
    const flow = flowId ? (await objectDetails(flowId)).measures : null;
    const flowSteps = flow?.kind === 'workflow' ? flow.workflow.steps : [];
    const stepRules = flowSteps.map((s) => (s.action.kind === 'command' ? { command: s.action.command, from: s.action.inputs.strings?.from, line: 'from' in s.action.settings } : null));
    const ranNothing = (await counts()).join() === countsBefore && (await projectSummary()).objects.filter((o) => manyIds.includes(o.id)).every((o) => o.name.startsWith('Many '));
    showPanel(api, 'workflow');
    await settle();
    document.querySelector<HTMLElement>('[aria-label="Stop adding from the tool panels"]')?.click();
    await settle();
    showPanel(api, 'strings');
    await settle();
    const ownAgain = goButton()?.textContent?.trim() ?? '';
    const byLayer = (k: number) => {
      const from = stepRules[k]?.from;
      return !!from && 'layer' in from && from.layer === 'many toes';
    };
    expect(
      "while adding, the Strings panel's button adds its tool to the workflow and runs nothing: Smooth, and Number across with the line drawn across, each finding the three toes by their layer",
      addSmooth === 'Add to Toes' && addAcross === 'Add to Toes' && flowSteps.length === 2 && stepRules[0]?.command === 'string.smooth' && stepRules[1]?.command === 'string.number_across' && byLayer(0) && byLayer(1) && stepRules[1]?.line === true && ranNothing && ownAgain === 'Draw across 3 strings',
      { buttons: [addSmooth, addAcross], steps: stepRules, ran_nothing: ranNothing, button_after: ownAgain },
    );
    // The workflow taken out again, so the checks after see the project as they always have.
    if (flowId) await show((await runCommand('object.delete', { object: flowId })).summary);

    // Number across: the three picked, by two clicks across them.
    await useTool('number_across');
    await typeNumber('The first number', '1');
    const acrossSays = goButton()?.textContent?.trim() ?? '';
    goButton()?.click();
    await wait(500);
    await click(screen([E + 300, N + 300, 100]));
    await wait(400);
    await click(screen([E + 300, N + 396, 100]));
    await wait(1500);
    const numbered = (await projectSummary()).objects.filter((o) => manyIds.includes(o.id)).map((o) => o.name).sort();
    expect('Number across numbers the three picked in the order a line drawn across them crosses them', acrossSays === 'Draw across 3 strings' && numbered.join() === 'Strip 1,Strip 2,Strip 3', { button: acrossSays, names: numbered });

    await useTool('offset');
    await pickToes();
    const onForOffset = stringsOn();
    await typeNumber('Offset distance', '8');
    await typeNumber('How many offsets', '1');
    const offsetMany = await go(1500);
    const offsets = (await projectSummary()).objects.filter((o) => o.kind === 'string' && o.name.includes(' offset 8 m ')).length;
    expect('Offset offsets each of the three picked, as one run', onForOffset === '3 strings' && offsetMany === 'Offset 3 strings' && offsets === 3, { on: onForOffset, button: offsetMany, strings_made: offsets });

    // Polygons from lines: four lines round a block east of the solid, picked the same way.
    const blockLines: [string, Vec3, Vec3][] = [
      ['south', [E + 680, N + 120, 100], [E + 920, N + 120, 100]],
      ['north', [E + 680, N + 260, 100], [E + 920, N + 260, 100]],
      ['west', [E + 700, N + 100, 100], [E + 700, N + 280, 100]],
      ['east', [E + 900, N + 100, 100], [E + 900, N + 280, 100]],
    ];
    for (const [name, a, b] of blockLines) await runCommand('string.create', { name: `Line ${name}`, layer: 'block lines', closed: false, points: [a, b] });
    await show(await projectSummary());
    await wait(1000);
    await useTool('polygons_from_lines');
    await click(screen([E + 800, N + 120, 100]));
    await ctrlClick(screen([E + 800, N + 260, 100]));
    await ctrlClick(screen([E + 700, N + 190, 100]));
    await ctrlClick(screen([E + 900, N + 190, 100]));
    await wait(500);
    const linesOn = stringsOn();
    const polygonSays = await go(1500);
    const polygons = (await projectSummary()).objects.filter((o) => o.kind === 'string' && o.closed && /^B\d+$/.test(o.name));
    expect('Polygons from lines makes a block polygon from the four lines picked', linesOn === '4 lines picked' && polygonSays === 'Make polygons from 4 lines' && polygons.length === 1, {
      on: linesOn,
      button: polygonSays,
      polygons: polygons.map((o) => o.name),
    });

    viewport.setPlanView(false);
    await goStage('solids');
    await clickTool('solids.repair');
    expect('Repair, with no closed mesh kept as a surface, says there is nothing to repair', footer().startsWith('Nothing to repair'), footer().slice(0, 70));

    // The panels' heads, every panel open in its dock, with nothing picked.
    await key('Escape');
    resetLayout(api);
    await settle();
    const everyPanel = ['strings', 'builder', 'cut', 'reserves', 'workflow', 'blocks', 'restable', 'params', 'canvas', 'face'] as const;
    for (const id of everyPanel) showPanel(api, id);
    await settle();
    const toFront = async (id: PanelId) => {
      api.getPanel(id)?.api.setActive();
      await settle();
    };
    const groupOf = (id: PanelId) => api.getPanel(id)?.group.element;
    const stripOf = (id: PanelId) => groupOf(id)?.querySelector<HTMLElement>('.dv-tabs-and-actions-container') ?? null;
    const shown = (e: Element) => e.getClientRects().length > 0;
    /**
     * What's in a panel under its tabs, in the order it's read: in its own content, which a panel kept in the page
     * (Parameters, 0081) holds outside its group.
     */
    const contentOf = (id: PanelId) => (api.getPanel(id) as { view?: { content?: { element?: HTMLElement } } } | undefined)?.view?.content?.element ?? groupOf(id);
    const inPanel = (id: PanelId, what: string) => [...(contentOf(id)?.querySelectorAll<HTMLElement>(what) ?? [])].filter((e) => !stripOf(id)?.contains(e) && shown(e));
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
    const toolPanels = ['strings', 'builder', 'cut', 'reserves', 'workflow'] as const;
    const titles: Record<(typeof toolPanels)[number], string> = { strings: 'Strings', builder: 'Builder', cut: 'Cut solids', reserves: 'Reserves', workflow: 'Workflow' };
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
    // A tooltip opens on its own delay: waited for, not read after one pause (this check failed now and then on that).
    for (let k = 0; k < 20 && !document.querySelector('[data-slot="tooltip-content"]'); k++) await wait(100);
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
      await picture('ribbon-strings', 'strings', [], ['strings:offset']);
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
