// `--shell`: the panels and the toolbar, as Tim would use them. Panels minimised by their header's button to strips
// at the window's edges and brought back by a name on a strip, kept that way through the layout being saved and read
// again; every panel hidden by the ` key and brought back; and the toolbar's groups, with what came off it in the
// Import menu. Then the docks (decision 0076): tools opened from the toolbar are tabs in one dock beside Properties and
// tables are tabs in one dock under the view, so the view keeps its width; a minimised dock comes back with the tool
// asked for in front; a table fills the window and goes back; and a dock closed and opened again gives its room to the
// view and takes it back, the other docks staying put. With `--pictures <folder>` the window is then drawn in three
// states (a tool and a table open for the Solids, Walls and Reserves work), over the project given by `--dataflow` if
// there is one. The layout saved on this machine is put back as it was at the end.
import type { DockviewApi } from 'dockview-react';
import { openProject, type ProjectSummary, writePicture } from '../bridge';
import type { Viewport } from '../viewport/Viewport';
import { windowPicture } from './windowPicture';
import { LAYOUT_KEYS, type PanelId, resetLayout, showPanel, TOOL_DOCK_WIDTH } from '../workspace/Workspace';

const wait = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));

/** A project to draw the window over, and how the app shows it. */
interface Pictures {
  project: string | null;
  show: (summary: ProjectSummary) => Promise<void> | undefined;
}

export async function checkShell(viewport: Viewport, log: (line: string) => void, dock: () => DockviewApi | null, pictures: Pictures | null = null) {
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

    if (pictures) {
      if (pictures.project) {
        log(`shell: opening ${pictures.project}`);
        await pictures.show(await openProject(pictures.project));
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
