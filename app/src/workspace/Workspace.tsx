import { createContext, useContext, useEffect, useState } from 'react';
import {
  type AddPanelOptions,
  type DockviewApi,
  DockviewDefaultTab,
  type DockviewGroupPanel,
  type DockviewReadyEvent,
  DockviewReact,
  type IDockviewHeaderActionsProps,
  type IDockviewPanelHeaderProps,
  themeDark,
} from 'dockview-react';
import { Minus } from 'lucide-react';
import { cn } from 'cn';
import { BuilderPanel } from '@/panels/BuilderPanel';
import { CutPanel } from '@/panels/CutPanel';
import { FaceDesignPanel } from '@/panels/FaceDesignPanel';
import { LayersPanel } from '@/panels/LayersPanel';
import { PropertiesPanel } from '@/panels/PropertiesPanel';
import { ReservesPanel } from '@/panels/ReservesPanel';
import { RunLogPanel } from '@/panels/RunLogPanel';
import { ViewportPanel } from '@/panels/ViewportPanel';
import { WorkflowCanvas } from '@/panels/WorkflowCanvas';
import { WorkflowPanel } from '@/panels/WorkflowPanel';

const components = {
  viewport: ViewportPanel,
  layers: LayersPanel,
  properties: PropertiesPanel,
  runlog: RunLogPanel,
  face: FaceDesignPanel,
  builder: BuilderPanel,
  cut: CutPanel,
  reserves: ReservesPanel,
  workflow: WorkflowPanel,
  canvas: WorkflowCanvas,
};

/** The key that hides every panel, so the viewport fills the window, and brings them back. */
export const ALL_PANELS_KEY = '`';

// The viewport tab has no close button: there is one viewport and the app always shows it. A double-click on it
// hides every other panel, or brings them back.
const tabComponents = {
  fixed: (props: IDockviewPanelHeaderProps) => (
    <div className="h-full" onDoubleClick={() => toggleAllPanels(props.containerApi)} title={`Double-click (or ${ALL_PANELS_KEY}) to hide every panel, or bring them back`}>
      <DockviewDefaultTab {...props} hideClose />
    </div>
  ),
};

export type PanelId = Exclude<keyof typeof components, 'viewport'>;

/** The panels that can be closed and reopened, with where each goes and its size there. */
export const PANELS: { id: PanelId; title: string; place: AddPanelOptions['position']; size: { width?: number; height?: number } }[] = [
  { id: 'layers', title: 'Layers', place: { referencePanel: 'viewport', direction: 'left' }, size: { width: 280 } },
  { id: 'properties', title: 'Properties', place: { referencePanel: 'viewport', direction: 'right' }, size: { width: 320 } },
  { id: 'runlog', title: 'Run log', place: { referencePanel: 'viewport', direction: 'below' }, size: { height: 200 } },
  { id: 'face', title: 'Face design', place: { referencePanel: 'viewport', direction: 'right' }, size: { width: 680 } },
  { id: 'builder', title: 'Builder', place: { referencePanel: 'viewport', direction: 'right' }, size: { width: 440 } },
  { id: 'cut', title: 'Cut solids', place: { referencePanel: 'viewport', direction: 'right' }, size: { width: 400 } },
  { id: 'reserves', title: 'Reserves', place: { referencePanel: 'viewport', direction: 'right' }, size: { width: 400 } },
  { id: 'workflow', title: 'Workflow', place: { referencePanel: 'viewport', direction: 'right' }, size: { width: 420 } },
  { id: 'canvas', title: 'Workflow canvas', place: { referencePanel: 'viewport', direction: 'below' }, size: { height: 420 } },
];

const LAYOUT_KEY = 'zd.workspace.layout.v3';
/** Which edge each minimised group shrank to, by group id. The layout itself keeps that it's hidden. */
const MINIMISED_KEY = 'zd.workspace.minimised.v1';

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

/** Shrinks a group to a strip with its panels' names at the nearest edge of the window. The viewport takes the room. */
function minimise(api: DockviewApi, group: DockviewGroupPanel) {
  writeEdges({ ...readEdges(), [group.id]: edgeOf(api, group) });
  group.api.setVisible(false);
  saveLayout(api);
  onMinimisedChange?.();
}

/** Brings a minimised group back where it was, at its size, with `panelId` in front. */
function restore(api: DockviewApi, groupId: string, panelId: string) {
  const group = api.getGroup(groupId);
  if (!group) return;
  group.api.setVisible(true);
  api.getPanel(panelId)?.api.setActive();
  const { [groupId]: _gone, ...rest } = readEdges();
  writeEdges(rest);
  saveLayout(api);
  onMinimisedChange?.();
}

/** Hides every panel but the viewport, which fills the window, or brings them back (`, or a double-click on its tab). */
export function toggleAllPanels(api: DockviewApi) {
  if (api.hasMaximizedGroup()) api.exitMaximizedGroup();
  else api.getPanel('viewport')?.api.maximize();
  onMinimisedChange?.();
}

function addViewport(api: DockviewApi) {
  // 'always' keeps the canvas in the page while other tabs share its group.
  api.addPanel({ id: 'viewport', component: 'viewport', tabComponent: 'fixed', title: 'Viewport', renderer: 'always' });
}

/** The layer tree on the left, properties on the right, the run log under the viewport. */
function defaultLayout(api: DockviewApi) {
  addViewport(api);
  api.addPanel({ id: 'layers', component: 'layers', title: 'Layers', position: { referencePanel: 'viewport', direction: 'left' } });
  api.addPanel({ id: 'properties', component: 'properties', title: 'Properties', position: { referencePanel: 'viewport', direction: 'right' } });
  api.addPanel({ id: 'runlog', component: 'runlog', title: 'Run log', position: { referencePanel: 'viewport', direction: 'below' } });
  // Splits start even; give the viewport the room.
  for (const id of ['layers', 'properties', 'runlog'] as const) {
    api.getPanel(id)?.group.api.setSize(PANELS.find((p) => p.id === id)!.size);
  }
}

/** The saved layout from the last session, or the default one if there is none or it no longer fits. */
function restoreLayout(api: DockviewApi) {
  try {
    const saved = localStorage.getItem(LAYOUT_KEY);
    if (saved) {
      api.fromJSON(JSON.parse(saved));
      if (!api.getPanel('viewport')) addViewport(api);
      return;
    }
  } catch {
    api.clear();
  }
  defaultLayout(api);
}

/** Brings a panel to the front, reopening it where it belongs if it was closed, or its group if it was minimised. */
export function showPanel(api: DockviewApi, id: PanelId) {
  const open = api.getPanel(id);
  if (open) {
    if (!open.group.api.isVisible) return restore(api, open.group.id, id);
    return open.api.setActive();
  }
  const panel = PANELS.find((p) => p.id === id)!;
  api.addPanel({ id, component: id, title: panel.title, position: panel.place }).group.api.setSize(panel.size);
}

export function resetLayout(api: DockviewApi) {
  if (api.hasMaximizedGroup()) api.exitMaximizedGroup();
  api.clear();
  writeEdges({});
  defaultLayout(api);
  onMinimisedChange?.();
}

const MinimiseContext = createContext<DockviewApi | null>(null);

/** A group's header: a button that minimises it, on every group but the viewport's and one floating free. */
function GroupActions({ group, panels, location }: IDockviewHeaderActionsProps) {
  const api = useContext(MinimiseContext);
  if (!api || panels.some((p) => p.id === 'viewport') || (location && location.type !== 'grid')) return null;
  const names = panels.map((p) => p.title ?? p.id).join(', ');
  return (
    <button
      type="button"
      className="flex h-full items-center px-2 text-muted-foreground hover:text-foreground"
      title={`Minimise ${names}: it shrinks to its name at the edge of the window; click the name to bring it back`}
      aria-label={`Minimise ${names}`}
      onClick={() => minimise(api, group)}
    >
      <Minus className="size-3.5" />
    </button>
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
export function Workspace({ onReady }: { onReady: (api: DockviewApi) => void }) {
  const [api, setApi] = useState<DockviewApi | null>(null);
  const [minimised, setMinimised] = useState<Minimised[]>([]);
  const [allHidden, setAllHidden] = useState(false);

  useEffect(() => {
    if (!api) return;
    const refresh = () => {
      const edges = readEdges();
      setMinimised(
        api.groups
          .filter((g) => !g.api.isVisible && g.api.location.type === 'grid')
          .map((g) => ({ groupId: g.id, edge: edges[g.id] ?? 'right', panels: g.panels.map((p) => ({ id: p.id, title: p.title ?? p.id })) })),
      );
      setAllHidden(api.hasMaximizedGroup());
    };
    onMinimisedChange = refresh;
    refresh();
    const layout = api.onDidLayoutChange(() => {
      saveLayout(api);
      refresh();
    });
    return () => {
      layout.dispose();
      if (onMinimisedChange === refresh) onMinimisedChange = null;
    };
  }, [api]);

  const ready = ({ api: dock }: DockviewReadyEvent) => {
    restoreLayout(dock);
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
          <div className="min-h-0 flex-1">
            <DockviewReact
              components={components}
              tabComponents={tabComponents}
              rightHeaderActionsComponent={GroupActions}
              onReady={ready}
              theme={themeDark}
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
