import { createContext, type ReactNode, useContext, useEffect, useRef, useState } from 'react';
import {
  type DockviewApi,
  DockviewDefaultTab,
  type DockviewGroupPanel,
  type DockviewReadyEvent,
  DockviewReact,
  type IDockviewHeaderActionsProps,
  type IDockviewPanelHeaderProps,
  themeDark,
  themeLight,
} from 'dockview-react';
import { Maximize2, Minimize2, Minus } from 'lucide-react';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { getAllWebviewWindows } from '@tauri-apps/api/webviewWindow';
import { cn } from 'cn';
import { useLook } from './useLook';
import { BuilderPanel } from '@/panels/BuilderPanel';
import { CutPanel } from '@/panels/CutPanel';
import { FaceDesignPanel } from '@/panels/FaceDesignPanel';
import { LayersPanel } from '@/panels/LayersPanel';
import { FromSurfacesPanel } from '@/panels/FromSurfacesPanel';
import { MovePanel } from '@/panels/MovePanel';
import { PropertiesPanel } from '@/panels/PropertiesPanel';
import { ReservesPanel } from '@/panels/ReservesPanel';
import { RunLogPanel } from '@/panels/RunLogPanel';
import { StringsPanel } from '@/panels/StringsPanel';
import { ViewportPanel } from '@/panels/ViewportPanel';
import { TablesPanel } from '@/panels/TablesPanel';
import { WorkflowDocument } from '@/panels/WorkflowDocument';
import { SurfacesPanel } from '@/panels/SurfacesPanel';
import { LicencePanel } from '@/panels/LicencePanel';
import { ViewControlsPanel } from '@/panels/ViewControlsPanel';
import { TablesSidePanel } from '@/panels/TablesSidePanel';
import { type Dock, isItem, type PanelId, PANELS, panelSide, TOOL_DOCK_WIDTH } from './panels';
import { openItem, poppedNow, type Section, setPopped, setPopper } from './tablesSide';
import type { Side } from '@/tools';

/** What each panel draws. A panel in the list with nothing to draw doesn't compile. */
const components: Record<PanelId | 'viewport' | (typeof RETIRED)[number], () => ReactNode> = {
  viewport: ViewportPanel,
  layers: LayersPanel,
  properties: PropertiesPanel,
  strings: StringsPanel,
  from_surfaces: FromSurfacesPanel,
  surfaces: SurfacesPanel,
  move: MovePanel,
  runlog: RunLogPanel,
  face: FaceDesignPanel,
  builder: BuilderPanel,
  cut: CutPanel,
  reserves: ReservesPanel,
  workflow: WorkflowDocument,
  restable: TablesPanel,
  // A tab no longer made, drawn as nothing until a saved layout's copy of it is closed.
  blocks: () => null,
  notebook: () => null,
  setup: () => null,
  canvas: () => null,
  // The Tables side, one panel: its list's items are drawn in its middle, never as tabs of their own (decision 0108).
  tables: TablesSidePanel,
  inputs: () => null,
  case: () => null,
  dataflows: () => null,
  params: () => null,
  results: () => null,
  schedule: () => null,
  licence: LicencePanel,
  viewcontrols: ViewControlsPanel,
};

/**
 * Tabs a layout saved before may hold that the app no longer has: the Objects table, now Objects in the Tables tab
 * (Tables and scripts plan, decision 1); the Dataflow and Setup tabs, now the Reserves case tab and its Setup (Reserves
 * case plan, decision 8). Opened from a saved layout, each is closed at once, the rest kept as saved.
 */
const RETIRED = ['blocks', 'notebook', 'setup', 'canvas'] as const;

/** The key that hides every panel, so the viewport fills the window, and brings them back. */
export const ALL_PANELS_KEY = '`';

// The view has no tab strip of its own (decision 0076). Its tab shows only while another panel shares its group, in a
// layout saved before the strip went, and has no close button: there is one view and the app always shows it.
const tabComponents = {
  fixed: (props: IDockviewPanelHeaderProps) => <DockviewDefaultTab {...props} hideClose />,
};

export { type Dock, type PanelId, PANELS, TOOL_DOCK_WIDTH };

const dockOf = (id: string): Dock | undefined => PANELS.find((p) => p.id === id)?.dock;

/**
 * The saved layout, and beside it which edge each minimised group shrank to, by group id (the layout itself keeps that
 * it's hidden). Layouts saved before the docks (v3, and its v1 edges) aren't read: the app opens once on the new default.
 */
export const LAYOUT_KEYS = ['zd.workspace.layout.v4', 'zd.workspace.minimised.v2'] as const;
const [LAYOUT_KEY, MINIMISED_KEY] = LAYOUT_KEYS;
const OLD_LAYOUT_KEYS = ['zd.workspace.layout.v3', 'zd.workspace.minimised.v1'];

type Edge = 'left' | 'right' | 'top' | 'bottom';

/** A group shrunk to a strip at the edge of the window: its panels' names, which bring it back. */
interface Minimised {
  groupId: string;
  edge: Edge;
  panels: { id: string; title: string }[];
}

function readEdges(): Record<string, Edge> {
  try {
    return JSON.parse(localStorage.getItem(MINIMISED_KEY) ?? '{}') as Record<string, Edge>;
  } catch {
    return {};
  }
}

function writeEdges(edges: Record<string, Edge>) {
  try {
    localStorage.setItem(MINIMISED_KEY, JSON.stringify(edges));
  } catch {
    // Storage unavailable: a minimised group comes back on the right next time.
  }
}

/** The side the workspace is on (decision 0108): Design's docks round the view, or the Tables side's group filling the window. */
let currentSide: Side = 'design';
/** Design's docks' sizes as they stood the last time Design was in front, put back as the Tables side comes and goes. */
let designDocks: Held[] = [];
/** Told when a panel of the other side is asked for, so App switches its side and ribbon with it. */
let onSideAsked: ((side: Side) => void) | null = null;


/** A panel the Tables side shows: its own, and one that goes with either side. */
const onTables = (id: string) => panelSide(id) !== 'design';

/**
 * The Tables side's group: the one holding a Tables panel and none of Design's, never the view's. It fills the window on
 * the Tables side and is hidden on Design, its panels kept as they were left.
 */
function tablesGroup(api: DockviewApi): DockviewGroupPanel | undefined {
  return api.groups.find(
    (g) =>
      g.api.location.type === 'grid' &&
      g.panels.length > 0 &&
      !g.panels.some((p) => p.id === 'viewport') &&
      g.panels.every((p) => onTables(p.id)) &&
      g.panels.some((p) => panelSide(p.id) === 'tables'),
  );
}

/** A group of its own under the view, empty, for a side's panels to be moved into. */
const groupUnderView = (api: DockviewApi) => api.addGroup({ referencePanel: 'viewport', direction: 'below' });

/**
 * The Tables panels open, gathered into the Tables side's group (made under the view if there's none): those of a
 * layout saved before the two sides sat in the tables dock beside Design's.
 */
function gatherTables(api: DockviewApi): DockviewGroupPanel | undefined {
  let group = tablesGroup(api);
  for (const p of PANELS) {
    if (p.side !== 'tables') continue;
    const panel = api.getPanel(p.id);
    // One in a window of its own stays there (decision 0109).
    if (!panel || panel.group === group || panel.group.api.location.type !== 'grid') continue;
    group ??= groupUnderView(api);
    panel.api.moveTo({ group, position: 'center' });
  }
  return group;
}

/**
 * Puts the workspace on a side (decision 0108). Tables: its panels gathered into their group, which fills the window
 * (the Reserves case opened if none is). Design: that group hidden, Design's docks round the view as they were left.
 */
export function showSide(api: DockviewApi, side: Side) {
  // Design's docks keep their sizes as the Tables side's group comes and goes beside them: as they stood on Design.
  if (currentSide === 'design' && !api.hasMaximizedGroup()) designDocks = dockSizes(api);
  currentSide = side;
  // Tables asked for while the Tables side is in its own window (decision 0109): it comes back first, and the side is
  // put on the layout once it's back in the grid (`backFromWindow`).
  if (side === 'tables' && poppedGroup(api)) {
    poppedWindow?.close();
    return;
  }
  if (side === 'design') {
    if (api.hasMaximizedGroup()) api.exitMaximizedGroup();
    gatherTables(api)?.api.setVisible(false);
    const tables = tablesGroup(api);
    holdDocks(api, designDocks.filter((h) => h.group !== tables));
    keepMounted(api);
    onMinimisedChange?.();
    return;
  }
  const group = tablesSideGroup(api);
  holdDocks(api, designDocks);
  if (!group.api.isMaximized()) {
    if (api.hasMaximizedGroup()) api.exitMaximizedGroup();
    group.api.maximize();
  }
  keepMounted(api);
  onMinimisedChange?.();
}

/** The Tables side's group, its panels gathered in, made under the view if there's none, and shown. */
function tablesSideGroup(api: DockviewApi): DockviewGroupPanel {
  let group = gatherTables(api);
  if (!group) {
    const made = PANELS.find((p) => p.id === 'tables')!;
    api.addPanel({ id: made.id, component: made.id, title: made.title, position: { referencePanel: 'viewport', direction: 'below' } });
    group = api.getPanel('tables')!.group;
  }
  if (!group.api.isVisible) group.api.setVisible(true);
  // The Tables panel alone fills its group with no tab over it, as the view has none (decision 0108, workstream 4).
  const alone = group.panels.length === 1 && group.panels[0].id === 'tables';
  if (group.header.hidden !== alone) group.header.hidden = alone;
  return group;
}

/** The Tables panel's group while it's in a window of its own, and that window (decision 0109). */
const poppedGroup = (api: DockviewApi) => {
  const panel = api.getPanel('tables');
  return panel && panel.group.api.location.type === 'popout' ? panel.group : undefined;
};
let poppedWindow: Window | null = null;

/**
 * The Tables side into a window of its own, for a second monitor (decision 0109): dockview's pop-out, into an app window
 * the shell makes for it (`on_new_window` in main.rs), the main window left on Design. One app, so picks, edits and Run
 * stay shared. It pops out from either side: from Design, the ribbon's button beside the switch.
 */
async function popOutTables(api: DockviewApi) {
  if (poppedGroup(api)) return poppedWindow?.focus();
  if (currentSide === 'design' && !api.hasMaximizedGroup()) designDocks = dockSizes(api);
  const group = tablesSideGroup(api);
  if (api.hasMaximizedGroup()) api.exitMaximizedGroup();
  const position = {
    left: window.screenX + 60,
    top: window.screenY + 60,
    width: Math.max(1000, Math.round(window.outerWidth * 0.7)),
    height: Math.max(700, Math.round(window.outerHeight * 0.85)),
  };
  const opened = await api.addPopoutGroup(group, {
    popoutUrl: '/popout.html',
    position,
    onDidOpen: ({ window: w }) => {
      poppedWindow = w;
      passKeys(w);
    },
  });
  if (!opened || !poppedGroup(api)) return;
  // The empty group dockview keeps in the grid to bring it back to: away, so the view has its room.
  for (const g of api.groups) if (g.api.location.type === 'grid' && g.panels.length === 0 && g.api.isVisible) g.api.setVisible(false);
  setPopped(true);
  onSideAsked?.('design');
  showSide(api, 'design');
}

/** Brought back by its button, the switch's Tables or the ribbon's: the main window goes to Tables to show it there. */
let backToTables = false;

/** The Tables side back into the main window, shown there on Tables: its window closed, dockview puts the group back. */
const popBackTables = () => {
  backToTables = true;
  poppedWindow?.close();
};

/**
 * Once the Tables side's window has closed, the side put on the layout: brought back, on Tables, filling the window;
 * closed by its X, the main window stays on Design with the Tables side away, a click on Tables from it.
 */
function backFromWindow(api: DockviewApi) {
  if (!poppedNow() || poppedGroup(api)) return;
  setPopped(false);
  poppedWindow = null;
  closePoppedWindows();
  if (backToTables && currentSide !== 'tables') {
    currentSide = 'tables';
    onSideAsked?.('tables');
  }
  backToTables = false;
  setTimeout(() => showSide(api, currentSide), 0);
  // The main window takes the keys again.
  void getCurrentWindow()
    .setFocus()
    .catch(() => undefined);
}

/**
 * The Tables side's window itself closed once its panel is back. Closing its page (dockview's way back, from the switch
 * or its button) leaves the app window round it standing empty, so the shell is asked to take it away; by its X it's
 * gone already. Any left from an earlier pop-out go too.
 */
function closePoppedWindows() {
  if (!('__TAURI_INTERNALS__' in window)) return;
  void getAllWebviewWindows()
    .then((all) => Promise.all(all.filter((w) => w.label.startsWith('popout-')).map((w) => w.destroy().catch(() => undefined))))
    .catch(() => undefined);
}

/**
 * Keys pressed in the Tables side's window reach the app's shortcuts, as they would in the main window: Find a command,
 * Save, Undo and Redo, Esc, Delete. Typing in a box keeps its keys, but for Save and Find a command.
 */
function passKeys(w: Window) {
  w.addEventListener('keydown', (e) => {
    const t = e.target as HTMLElement | null;
    const typing = !!t && (t.isContentEditable || ['INPUT', 'TEXTAREA', 'SELECT'].includes(t.tagName));
    if (typing && !((e.ctrlKey || e.metaKey) && ['s', 'k'].includes(e.key.toLowerCase()))) return;
    const copy = new KeyboardEvent('keydown', { key: e.key, code: e.code, ctrlKey: e.ctrlKey, shiftKey: e.shiftKey, altKey: e.altKey, metaKey: e.metaKey, repeat: e.repeat, bubbles: true, cancelable: true });
    document.body.dispatchEvent(copy);
    if (copy.defaultPrevented) e.preventDefault();
  });
}

function saveLayout(api: DockviewApi) {
  // A maximised viewport (the bench, or every panel hidden) is not the layout to come back to.
  if (api.hasMaximizedGroup()) return;
  try {
    localStorage.setItem(LAYOUT_KEY, JSON.stringify(api.toJSON()));
  } catch {
    // Storage unavailable: the layout just isn't remembered.
  }
}

/** Told when a group is minimised or brought back from outside the workspace, so the strips follow. */
let onMinimisedChange: (() => void) | null = null;

/** Which side of the viewport a group sits on: the edge of the window it shrinks to. */
function edgeOf(api: DockviewApi, group: DockviewGroupPanel): Edge {
  const view = api.getPanel('viewport')?.group.element.getBoundingClientRect();
  const box = group.element.getBoundingClientRect();
  if (!view) return 'right';
  if (box.right <= view.left + 1) return 'left';
  if (box.left >= view.right - 1) return 'right';
  if (box.top >= view.bottom - 1) return 'bottom';
  return 'top';
}

/** A dock's size as it stood: its width beside the view, or its height above or below it. */
type Held = { group: DockviewGroupPanel; upright: boolean; size: number };

/** The docks' sizes as they stand, to put back after a dock comes or goes. None while a group fills the window. */
function dockSizes(api: DockviewApi): Held[] {
  if (api.hasMaximizedGroup()) return [];
  return api.groups
    .filter((g) => g.api.location.type === 'grid' && g.api.isVisible && !g.panels.some((p) => p.id === 'viewport'))
    .map((group) => {
      const edge = edgeOf(api, group);
      const upright = edge === 'left' || edge === 'right';
      return { group, upright, size: upright ? group.api.width : group.api.height };
    });
}

/**
 * Puts the docks still there back at the sizes they had. dockview shares a closed dock's room out among every panel
 * left, and takes a new dock's from them all; this way the view takes the room and gives it, and the docks stay put.
 */
function holdDocks(api: DockviewApi, held: Held[]) {
  if (api.hasMaximizedGroup()) return;
  for (const { group, upright, size } of held) {
    if (api.getGroup(group.id) !== group || !group.api.isVisible || size <= 0) continue;
    group.api.setSize(upright ? { width: size } : { height: size });
  }
}

/** Shrinks a group to a strip with its panels' names at the nearest edge of the window. The viewport takes the room. */
function minimise(api: DockviewApi, group: DockviewGroupPanel) {
  writeEdges({ ...readEdges(), [group.id]: edgeOf(api, group) });
  const held = dockSizes(api);
  group.api.setVisible(false);
  holdDocks(api, held);
  saveLayout(api);
  onMinimisedChange?.();
}

/** Brings a minimised group back where it was, at its size, with `panelId` in front. */
function restore(api: DockviewApi, groupId: string, panelId: string) {
  const group = api.getGroup(groupId);
  if (!group) return;
  const held = dockSizes(api);
  group.api.setVisible(true);
  holdDocks(api, held);
  api.getPanel(panelId)?.api.setActive();
  const { [groupId]: _gone, ...rest } = readEdges();
  writeEdges(rest);
  saveLayout(api);
  onMinimisedChange?.();
}

/**
 * Hides every panel but the viewport, which fills the window, or brings them back (`, or the Panels menu). With a table
 * filling the window, the view takes its place.
 */
export function toggleAllPanels(api: DockviewApi) {
  const view = api.getPanel('viewport');
  const viewFills = view?.api.isMaximized() ?? false;
  if (api.hasMaximizedGroup()) api.exitMaximizedGroup();
  if (!viewFills) view?.api.maximize();
  // On the Tables side the same key shows the view in the window, and puts the tables back over it (decision 0108).
  else if (currentSide !== 'design') return showSide(api, currentSide);
  onMinimisedChange?.();
}

/** Lets a group fill the window, over the view and every other panel, or puts it back in its dock. */
function toggleFill(group: DockviewGroupPanel) {
  if (group.api.isMaximized()) group.api.exitMaximized();
  else group.api.maximize();
  onMinimisedChange?.();
}

/**
 * A dock as it stands: the group in the layout holding the most of its panels, wherever it has been dragged to. The
 * viewport's own group is never one, so a table dragged over the view doesn't take the next table with it.
 */
function dockGroup(api: DockviewApi, dock: Dock): DockviewGroupPanel | undefined {
  const count = (g: DockviewGroupPanel) => g.panels.filter((p) => dockOf(p.id) === dock).length;
  const tables = tablesGroup(api);
  return api.groups
    .filter((g) => g.api.location.type === 'grid' && count(g) > 0 && g !== tables && !g.panels.some((p) => p.id === 'viewport'))
    .sort((a, b) => count(b) - count(a))[0];
}

/** The room a dock is made with: a width beside the view, a height under it. */
const dockSize = (panel: (typeof PANELS)[number]) => (panel.dock === 'results' ? { height: panel.size } : { width: panel.size });

/** The tools that share the one slot: a tool in front closes the one before it (decision 0108). */
const inSlot = (id: string) => dockOf(id) === 'tools';

function addViewport(api: DockviewApi) {
  // 'always' keeps the canvas in the page while other tabs share its group.
  api.addPanel({ id: 'viewport', component: 'viewport', tabComponent: 'fixed', title: 'Viewport', renderer: 'always' });
  bareView(api);
}

/**
 * The view's group has no tab strip, and takes no panel dropped on its middle: a panel dropped there would cover the
 * view with no tab to come back by. Panels still dock against its edges. If a layout from before has a panel sharing
 * the group, the strip shows until that panel is closed or dragged out.
 */
function bareView(api: DockviewApi) {
  const group = api.getPanel('viewport')?.group;
  if (!group) return;
  const alone = group.panels.length === 1;
  if (group.header.hidden !== alone) group.header.hidden = alone;
  if (group.locked !== alone) group.locked = alone;
}

/** The layer tree on the left, the tool dock with Properties on the right, the results dock with the run log under the view. */
function defaultLayout(api: DockviewApi) {
  addViewport(api);
  const first = ['layers', 'properties', 'runlog'] as const;
  for (const id of first) showPanel(api, id);
  // Splits start even; give the viewport the room.
  const size = () => first.forEach((id) => api.getPanel(id)?.group.api.setSize(dockSize(PANELS.find((p) => p.id === id)!)));
  size();
  // As the app starts the workspace has no size yet, and shares the window out evenly when it gets one: size it then.
  if (api.width === 0 || api.height === 0) requestAnimationFrame(size);
}

/**
 * A layout saved before the tool slot (decision 0108) put right: the tool in front kept and the others closed,
 * Properties in a group of its own under it, and Face design over the view.
 */
function tidyDocks(api: DockviewApi) {
  const tools = api.panels.filter((p) => inSlot(p.id));
  const kept = tools.find((p) => p.group.activePanel?.id === p.id) ?? tools[0];
  for (const p of tools) if (p !== kept) p.api.close();
  const props = api.getPanel('properties');
  if (props && props.group.panels.length > 1) props.api.moveTo({ group: props.group, position: 'bottom' });
  for (const p of PANELS.filter((q) => q.dock === 'document')) {
    const open = api.getPanel(p.id);
    const view = api.getPanel('viewport');
    if (open && view && open.group !== view.group) open.api.moveTo({ group: view.group, position: 'center' });
  }
}

/** The saved layout from the last session, or the default one if there is none or it no longer fits. */
function restoreLayout(api: DockviewApi) {
  try {
    for (const key of OLD_LAYOUT_KEYS) localStorage.removeItem(key);
    const saved = localStorage.getItem(LAYOUT_KEY);
    if (saved) {
      api.fromJSON(JSON.parse(saved));
      for (const id of RETIRED) api.getPanel(id)?.api.close();
      // The Reserves case and Parameters tabs of a layout saved before the Tables side had its list (decision 0108,
      // workstream 4): each is an item of the list now, shown in the Tables panel.
      for (const p of [...api.panels]) if (isItem(p.id)) p.api.close();
      // A layout saved before a tab was renamed (the Notebook, now Dataflow) keeps its old title: today's names.
      for (const p of PANELS) {
        const shown = api.getPanel(p.id);
        if (shown && shown.title !== p.title) shown.api.setTitle(p.title);
      }
      if (!api.getPanel('viewport')) addViewport(api);
      tidyDocks(api);
      bareView(api);
      keepMounted(api);
      return;
    }
  } catch {
    api.clear();
  }
  defaultLayout(api);
}

/**
 * Brings a panel to the front. One that was closed opens as a tab in its dock: a tool beside Properties, a table under
 * the view, so the view keeps its room. A minimised dock comes back with the panel in front. A dock with nothing left
 * in it is made again: the tools down the right of the window, the tables under the view.
 */
export function showPanel(api: DockviewApi, id: PanelId) {
  // An item of the Tables side's list opens in its panel (decision 0108, workstream 4).
  if (isItem(id)) {
    openItem(id as Section);
    id = 'tables';
  }
  const side = panelSide(id);
  if (side !== currentSide) {
    onSideAsked?.(side);
    showSide(api, side);
  }
  if (currentSide !== 'design' && onTables(id)) return showOnTables(api, id);
  bringToFront(api, id);
  keepMounted(api);
}

/** A panel shown on the Tables side: a tab in its group, which fills the window. */
function showOnTables(api: DockviewApi, id: PanelId) {
  const group = tablesGroup(api) ?? groupUnderView(api);
  const open = api.getPanel(id);
  if (open && open.group !== group) open.api.moveTo({ group, position: 'center' });
  if (!open) {
    const made = PANELS.find((p) => p.id === id)!;
    api.addPanel({ id, component: id, title: made.title, position: { referenceGroup: group, direction: 'within' } });
  }
  api.getPanel(id)?.api.setActive();
  showSide(api, 'tables');
}

/**
 * Tabs kept in the page while another tab of their dock is in front, as the view is: the Parameters tab holds edits
 * not yet saved, which a tab put away would lose, as Run in notebook brings the Notebook to the front beside it
 * (decision 0081, point 7).
 */
const KEEP_MOUNTED: readonly PanelId[] = ['tables'];

function keepMounted(api: DockviewApi) {
  for (const id of KEEP_MOUNTED) {
    const panel = api.getPanel(id);
    if (panel && panel.api.renderer !== 'always') panel.api.setRenderer('always');
  }
}

function bringToFront(api: DockviewApi, id: PanelId) {
  const open = api.getPanel(id);
  if (open) {
    if (!open.group.api.isVisible) return restore(api, open.group.id, id);
    return open.api.setActive();
  }
  const panel = PANELS.find((p) => p.id === id)!;
  // A document opens over the view, a tab beside it: the view's tab comes back as the document's is closed.
  if (panel.dock === 'document') {
    api.addPanel({ id, component: id, title: panel.title, position: { referencePanel: 'viewport', direction: 'within' } });
    bareView(api);
    return;
  }
  // One tool at a time: the one in the slot makes way for the one asked for.
  if (inSlot(id)) for (const other of api.panels.filter((p) => p.id !== id && inSlot(p.id))) other.api.close();
  const dock = dockGroup(api, panel.dock);
  if (!dock) {
    // The tool slot sits over Properties, the inspector, where it's open; else down the right of the window.
    const props = panel.dock === 'tools' ? api.getPanel('properties') : undefined;
    const position =
      panel.dock === 'results'
        ? ({ referencePanel: 'viewport', direction: 'below' } as const)
        : props && props.group.api.isVisible
          ? ({ referencePanel: 'properties', direction: 'above' } as const)
          : ({ direction: panel.dock === 'left' ? 'left' : 'right' } as const);
    if (props && props.group.api.isVisible) {
      api.addPanel({ id, component: id, title: panel.title, position });
      return;
    }
    const held = dockSizes(api);
    const made = api.addPanel({ id, component: id, title: panel.title, position }).group;
    made.api.setSize(dockSize(panel));
    holdDocks(api, held);
    made.api.setSize(dockSize(panel));
    return;
  }
  api.addPanel({ id, component: id, title: panel.title, position: { referenceGroup: dock, direction: 'within' } });
  if (!dock.api.isVisible) return restore(api, dock.id, id);
  // A table gets the height it asks for; the dock never shrinks under one, and a tool never widens its dock.
  if (panel.dock === 'results' && !dock.api.isMaximized() && dock.api.height < panel.size) dock.api.setSize({ height: panel.size });
}

/** Whether `id` is open in a group of its own beside `beside`'s, rather than a tab in the same group. */
export function isBeside(api: DockviewApi, id: PanelId, beside: PanelId) {
  const panel = api.getPanel(id);
  const other = api.getPanel(beside);
  return !!panel && !!other && panel.group.id !== other.group.id && panel.group.api.location.type === 'grid' && panel.group.api.isVisible;
}

/**
 * Puts `id` beside `beside`, each with half of `beside`'s room (the Parameters beside the Notebook, say), so both are
 * read at once without dragging a tab. Asked again with them side by side, `id` goes back in as a tab behind it.
 */
export function placeBeside(api: DockviewApi, id: PanelId, beside: PanelId) {
  const other = api.getPanel(beside);
  if (!other) return showPanel(api, id);
  if (other.group.api.isMaximized()) other.group.api.exitMaximized();
  const panel = api.getPanel(id);
  if (panel && isBeside(api, id, beside)) {
    panel.api.moveTo({ group: other.group, position: 'center' });
    other.api.setActive();
  } else if (panel) {
    panel.api.moveTo({ group: other.group, position: 'right' });
  } else {
    const made = PANELS.find((p) => p.id === id)!;
    api.addPanel({ id, component: id, title: made.title, position: { referencePanel: beside, direction: 'right' } });
  }
  keepMounted(api);
  onMinimisedChange?.();
}

export function resetLayout(api: DockviewApi) {
  if (api.hasMaximizedGroup()) api.exitMaximizedGroup();
  // The layout from the start is Design's: the side goes back with it.
  if (currentSide !== 'design') onSideAsked?.('design');
  currentSide = 'design';
  api.clear();
  writeEdges({});
  defaultLayout(api);
  onMinimisedChange?.();
}

const MinimiseContext = createContext<DockviewApi | null>(null);

/**
 * A group's header: a button that minimises it, on every group but the viewport's and one floating free, and on the
 * results dock one before it that lets a table fill the window and puts it back.
 */
function GroupActions({ group, panels, location }: IDockviewHeaderActionsProps) {
  const api = useContext(MinimiseContext);
  const [fills, setFills] = useState(() => group.api.isMaximized());
  useEffect(() => {
    if (!api) return;
    setFills(group.api.isMaximized());
    const changed = api.onDidMaximizedGroupChange(() => setFills(group.api.isMaximized()));
    return () => changed.dispose();
  }, [api, group]);
  if (!api || panels.some((p) => p.id === 'viewport') || (location && location.type !== 'grid')) return null;
  // The Tables side's group is the side itself, filling the window or hidden: nothing to minimise, or put back under the view.
  if (group === tablesGroup(api)) return null;
  const names = panels.map((p) => p.title ?? p.id).join(', ');
  const tables = panels.some((p) => dockOf(p.id) === 'results');
  const button = 'flex h-full items-center px-2 text-muted-foreground hover:text-foreground';
  return (
    <div className="flex h-full">
      {tables && (
        <button
          type="button"
          className={button}
          title={fills ? 'Put the tables back under the view' : 'Fill the window with this table; the same button puts it back under the view'}
          aria-label={fills ? 'Put the tables back under the view' : 'Fill the window'}
          onClick={() => toggleFill(group)}
        >
          {fills ? <Minimize2 className="size-3.5" /> : <Maximize2 className="size-3.5" />}
        </button>
      )}
      {!fills && (
        <button
          type="button"
          className={button}
          title={`Minimise ${names}: it shrinks to its name at the edge of the window; click the name to bring it back`}
          aria-label={`Minimise ${names}`}
          onClick={() => minimise(api, group)}
        >
          <Minus className="size-3.5" />
        </button>
      )}
    </div>
  );
}

/** The groups minimised to one edge, each panel's name a button that brings its group back. */
function Strip({ edge, groups, api }: { edge: Edge; groups: Minimised[]; api: DockviewApi }) {
  const here = groups.filter((g) => g.edge === edge);
  if (here.length === 0) return null;
  const upright = edge === 'left' || edge === 'right';
  return (
    <div
      data-slot="minimised-strip"
      data-edge={edge}
      className={cn('flex shrink-0 gap-1 bg-card p-0.5', upright ? 'w-7 flex-col' : 'h-7 flex-row', { left: 'border-r', right: 'border-l', top: 'border-b', bottom: 'border-t' }[edge])}
    >
      {here.flatMap((g) =>
        g.panels.map((p) => (
          <button
            key={`${g.groupId}:${p.id}`}
            type="button"
            className={cn('rounded px-1 py-2 text-xs text-muted-foreground hover:bg-accent hover:text-foreground', upright ? '[writing-mode:vertical-rl]' : 'px-2 py-0.5', edge === 'left' && 'rotate-180')}
            title={`Bring back ${p.title}`}
            onClick={() => restore(api, g.groupId, p.id)}
          >
            {p.title}
          </button>
        )),
      )}
    </div>
  );
}

/**
 * The dockable workspace: drag tabs to rearrange; the layout is remembered between sessions. A group can be minimised
 * to a strip at the edge of the window, and every panel hidden at once so the viewport fills it.
 */
export function Workspace({
  side,
  onSide,
  onReady,
  onShown,
}: {
  side: Side;
  onSide: (side: Side) => void;
  onReady: (api: DockviewApi) => void;
  onShown?: (panels: PanelId[]) => void;
}) {
  const look = useLook();
  const told = useRef('');
  const [api, setApi] = useState<DockviewApi | null>(null);
  const [minimised, setMinimised] = useState<Minimised[]>([]);
  const [allHidden, setAllHidden] = useState(false);
  const room = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!api) return;
    const refresh = () => {
      const edges = readEdges();
      const tables = tablesGroup(api);
      setMinimised(
        api.groups
          // An empty group is no dock to bring back: the one dockview keeps while the Tables side is in its window.
          .filter((g) => !g.api.isVisible && g.api.location.type === 'grid' && g !== tables && g.panels.length > 0)
          .map((g) => ({ groupId: g.id, edge: edges[g.id] ?? 'right', panels: g.panels.map((p) => ({ id: p.id, title: p.title ?? p.id })) })),
      );
      setAllHidden(api.hasMaximizedGroup());
      // The panels in front in their docks, for the ribbon to light their tools: not one minimised, nor one behind a
      // group that fills the window.
      const filling = api.hasMaximizedGroup();
      const shown = api.groups
        .filter((g) => g.api.isVisible && (!filling || g.api.isMaximized()))
        .map((g) => g.activePanel?.id)
        .filter((id): id is PanelId => !!id && id !== 'viewport')
        .sort();
      if (shown.join() !== told.current) {
        told.current = shown.join();
        onShown?.(shown);
      }
    };
    onMinimisedChange = refresh;
    refresh();
    bareView(api);
    // The docks' sizes after each change to the layout, kept while a group fills the window: what they go back to when a
    // dock's last tab is closed, and when the workspace changes size.
    let held = dockSizes(api);
    // The Tables side's window: its button pops it out and brings it back; closed, the side goes back on the layout.
    setPopper({ out: () => void popOutTables(api), back: popBackTables });
    setPopped(!!poppedGroup(api));
    const layout = api.onDidLayoutChange(() => {
      backFromWindow(api);
      bareView(api);
      if (!api.hasMaximizedGroup()) held = dockSizes(api);
      saveLayout(api);
      refresh();
    });
    const closed = api.onDidRemoveGroup(() => {
      holdDocks(api, held);
      // The Tables side's last tab closed: the side keeps a group, with the Reserves case in it.
      if (currentSide !== 'design' && !tablesGroup(api)) showSide(api, currentSide);
    });
    const fronted = api.onDidActivePanelChange(() => refresh());
    // The window resized, or the strips came or went: dockview scales every panel, so once it has, the docks are put
    // back at their sizes and the view takes the difference.
    let frame = 0;
    const resized = new ResizeObserver(() => {
      cancelAnimationFrame(frame);
      frame = requestAnimationFrame(() => (frame = requestAnimationFrame(() => holdDocks(api, held))));
    });
    if (room.current) resized.observe(room.current);
    return () => {
      layout.dispose();
      closed.dispose();
      fronted.dispose();
      resized.disconnect();
      cancelAnimationFrame(frame);
      if (onMinimisedChange === refresh) onMinimisedChange = null;
    };
  }, [api, onShown]);

  onSideAsked = onSide;
  // The side App holds, put on the layout as it changes.
  useEffect(() => {
    if (api && side !== currentSide) showSide(api, side);
  }, [api, side]);

  const ready = ({ api: dock }: DockviewReadyEvent) => {
    restoreLayout(dock);
    showSide(dock, side);
    setApi(dock);
    onReady(dock);
  };
  const strip = (edge: Edge) => (api && !allHidden ? <Strip edge={edge} groups={minimised} api={api} /> : null);
  return (
    <MinimiseContext.Provider value={api}>
      <div className="flex h-full">
        {strip('left')}
        <div className="flex min-w-0 flex-1 flex-col">
          {strip('top')}
          <div ref={room} className="min-h-0 flex-1">
            <DockviewReact
              components={components}
              tabComponents={tabComponents}
              rightHeaderActionsComponent={GroupActions}
              onReady={ready}
              theme={look === 'dark' ? themeDark : themeLight}
              className="h-full"
            />
          </div>
          {strip('bottom')}
        </div>
        {strip('right')}
      </div>
    </MinimiseContext.Provider>
  );
}
