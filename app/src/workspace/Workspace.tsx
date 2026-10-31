import {
  type AddPanelOptions,
  type DockviewApi,
  DockviewDefaultTab,
  type DockviewReadyEvent,
  DockviewReact,
  type IDockviewPanelHeaderProps,
  themeDark,
} from 'dockview-react';
import { FaceDesignPanel } from '@/panels/FaceDesignPanel';
import { LayersPanel } from '@/panels/LayersPanel';
import { ObjectsPanel } from '@/panels/ObjectsPanel';
import { PropertiesPanel } from '@/panels/PropertiesPanel';
import { RunLogPanel } from '@/panels/RunLogPanel';
import { ViewportPanel } from '@/panels/ViewportPanel';

const components = {
  viewport: ViewportPanel,
  layers: LayersPanel,
  objects: ObjectsPanel,
  properties: PropertiesPanel,
  runlog: RunLogPanel,
  face: FaceDesignPanel,
};

// The viewport tab has no close button: there is one viewport and the app always shows it.
const tabComponents = {
  fixed: (props: IDockviewPanelHeaderProps) => <DockviewDefaultTab {...props} hideClose />,
};

export type PanelId = Exclude<keyof typeof components, 'viewport'>;

/** The panels that can be closed and reopened, with where each goes and its size there. */
export const PANELS: { id: PanelId; title: string; place: AddPanelOptions['position']; size: { width?: number; height?: number } }[] = [
  { id: 'layers', title: 'Layers', place: { referencePanel: 'viewport', direction: 'left' }, size: { width: 280 } },
  { id: 'objects', title: 'Objects', place: { referencePanel: 'viewport', direction: 'left' }, size: { width: 280 } },
  { id: 'properties', title: 'Properties', place: { referencePanel: 'viewport', direction: 'right' }, size: { width: 320 } },
  { id: 'runlog', title: 'Run log', place: { referencePanel: 'viewport', direction: 'below' }, size: { height: 200 } },
  { id: 'face', title: 'Face design', place: { referencePanel: 'viewport', direction: 'right' }, size: { width: 680 } },
];

const LAYOUT_KEY = 'zd.workspace.layout.v2';

function addViewport(api: DockviewApi) {
  // 'always' keeps the canvas in the page while other tabs share its group.
  api.addPanel({ id: 'viewport', component: 'viewport', tabComponent: 'fixed', title: 'Viewport', renderer: 'always' });
}

/** Layers over objects on the left, properties on the right, the run log under the viewport. */
function defaultLayout(api: DockviewApi) {
  addViewport(api);
  api.addPanel({ id: 'layers', component: 'layers', title: 'Layers', position: { referencePanel: 'viewport', direction: 'left' } });
  api.addPanel({ id: 'objects', component: 'objects', title: 'Objects', position: { referencePanel: 'layers', direction: 'below' } });
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

/** Brings a panel to the front, reopening it where it belongs if it was closed. */
export function showPanel(api: DockviewApi, id: PanelId) {
  const open = api.getPanel(id);
  if (open) return open.api.setActive();
  const panel = PANELS.find((p) => p.id === id)!;
  api.addPanel({ id, component: id, title: panel.title, position: panel.place }).group.api.setSize(panel.size);
}

export function resetLayout(api: DockviewApi) {
  api.clear();
  defaultLayout(api);
}

/** The dockable workspace: drag tabs to rearrange; the layout is remembered between sessions. */
export function Workspace({ onReady }: { onReady: (api: DockviewApi) => void }) {
  const ready = ({ api }: DockviewReadyEvent) => {
    restoreLayout(api);
    api.onDidLayoutChange(() => {
      // A maximised viewport (the bench, or a quick look) is not the layout to come back to.
      if (api.hasMaximizedGroup()) return;
      try {
        localStorage.setItem(LAYOUT_KEY, JSON.stringify(api.toJSON()));
      } catch {
        // Storage unavailable: the layout just isn't remembered.
      }
    });
    onReady(api);
  };
  return <DockviewReact components={components} tabComponents={tabComponents} onReady={ready} theme={themeDark} className="h-full" />;
}
