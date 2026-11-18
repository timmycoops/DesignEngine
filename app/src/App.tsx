import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { confirm, message, open, save } from '@tauri-apps/plugin-dialog';
import type { DockviewApi } from 'dockview-react';
import { cn } from 'cn';
import {
  benchConfig,
  blockValues,
  buildSnapIndex,
  type Colouring,
  type Contours,
  type DeswikLayer,
  type Display,
  deswikLayers,
  spryContents,
  type LogEntry,
  type Note,
  newProject,
  objectsBounds,
  objectsInBox,
  type ObjectSummary,
  openProject,
  type ProjectSummary,
  projectSummary,
  redo,
  reservesFieldValues,
  type RunRecord,
  type WorkflowRecord,
  type ModelLinks,
  runCommand,
  runWorkflow as runWorkflowCall,
  runAllCases as runAllCasesCall,
  captureWorkflow as captureWorkflowCall,
  addWorkflowStep,
  checkLinks,
  runLog,
  saveProject,
  setContours,
  setLayerOpacity,
  setLayerVisible,
  setLooks,
  setObjectsVisible,
  snapRay,
  undo,
  type Vec3,
  withLayerOpacity,
  withLooks,
} from './bridge';
import { runChecks } from './bench/runChecks';
import { type Bands, valueBands } from './viewport/bands';
import { lookAt, RunGoing, ShowNote } from './design-system/kit';
import { TooltipProvider } from './design-system/ui/tooltip';
import { type DeswikChoice, DeswikImportDialog, type ImportSource, type PickableLayer } from './panels/DeswikImportDialog';
import { ExportSolidsDialog, type SolidExportOptions } from './panels/ExportSolidsDialog';
import { type NotASolid, RepairDialog } from './panels/RepairDialog';
import { type ViewTools, ViewToolsContext } from './panels/ViewBar';
import { IssueMarkers } from './viewport/markers';
import { type RlShown, type SeamShown, SceneSync } from './viewport/sceneSync';
import { planPoint, SectionView } from './viewport/section';
import { RoadLayer } from './viewport/roads';
import { type Mode, StringLayer } from './viewport/strings';
import { typing } from './viewport/camera';
import { FIELD_OF_VIEW_DEFAULT, type FrameStats, Viewport } from './viewport/Viewport';
import { type AddingTo, type Draft, type FieldColour, type NotebookEdit, type ViewMenuAt, type Workspace as WorkspaceState, WorkspaceContext } from './workspace/context';
import { ALL_PANELS_KEY, isBeside, type PanelId, placeBeside, resetLayout, showPanel, toggleAllPanels, Workspace } from './workspace/Workspace';
import { inUse, type Tool, type ToolAction, type ToolNeeds } from './tools';
import { listenForChords } from './workspace/chords';
import { FindCommand } from './workspace/FindCommand';
import { Ribbon, type StageId } from './workspace/Ribbon';
import { rememberStage, stageOf } from './workspace/stage';
import type { ToolUse } from './workspace/ToolMenu';

interface Status {
  text: string;
  error?: boolean;
}

/** One line per run: what it made or changed, how long it took, how many of its notes to look at, and its first few. */
/** A workflow run in a line for the status line: its case, how many steps ran and how it ended. */
function workflowWords(r: WorkflowRecord): string {
  const ran = r.steps.filter((s) => s.outcome === 'ran').length;
  const ended = r.status === 'finished' ? 'finished' : r.status === 'paused' ? `paused at step ${r.at}` : `refused at step ${r.at}`;
  const which = r.case && r.case !== 'Base' ? ` (${r.case})` : '';
  return `${r.name}${which}: ${ran} step${ran === 1 ? '' : 's'} run from step ${r.from} in ${(r.duration_ms / 1000).toFixed(1)} s, ${ended}`;
}

function describe(record: RunRecord): string {
  const outputs = record.outputs.map((o) => `${o.name} (${o.change})`);
  const head = `${record.command}: ${outputs.slice(0, 4).join(', ') || 'done'}${outputs.length > 4 ? ` and ${outputs.length - 4} more` : ''} in ${record.duration_ms.toFixed(0)} ms`;
  const look = lookAt(record.notes);
  const messages = record.messages.slice(0, 2);
  const more = record.messages.length > 2 ? [`(${record.messages.length - 2} more messages in the run log)`] : [];
  return [head, ...(look ? [`${look} to look at`] : []), ...messages, ...more].join(' · ');
}

/** A file name from an object or project name: characters Windows refuses become underscores. */
function fileName(name: string, extension: string): string {
  const stem = name.replace(/[<>:"/\\|?*\u0000-\u001f]/g, '_').trim() || 'export';
  return `${stem}.${extension}`;
}

/** What goes out to DXF: strings, surfaces and solids. */
const DXF_KINDS = new Set(['string', 'surface', 'solid']);

/** Splits a file path into its folder and name, for either separator. */
function splitPath(path: string): [string, string] {
  const cut = Math.max(path.lastIndexOf('/'), path.lastIndexOf('\\'));
  return [path.slice(0, cut), path.slice(cut + 1)];
}

/** How near a click on a section must be to a line to pick it (px). */
const PICK_PX = 8;

/** What a click in the viewport picks: the nearest surface, solid or seam horizon on show. */
async function objectAt(viewport: Viewport, sync: SceneSync, section: SectionView, x: number, y: number): Promise<string | null> {
  if (section.open) return section.tools.pick(x, y, PICK_PX);
  const [nx, ny] = viewport.clientToNdc(x, y);
  const ray = viewport.rayFromNdc(nx, ny);
  const hit = await snapRay(ray.origin, ray.direction);
  const distance = hit ? Math.hypot(hit.local[0] - ray.origin[0], hit.local[1] - ray.origin[1], hit.local[2] - ray.origin[2]) : Infinity;
  const seam = sync.seamAt(nx, ny);
  return seam && seam.distance < distance ? seam.id : (hit?.object ?? null);
}


/** How the import dialog speaks of a Spry project (decision 0039). */
const SPRY: ImportSource = {
  from: 'Spry',
  description:
    "Pick the Solid fields and design layers to read. A block solid is named by its node path and keeps its position on each level, the values beside its Solid field and Spry's volume; strings and layer solids keep their attributes.",
  placeholder: 'Filter: words in any order, e.g. coal solid or design toe',
};
export default function App() {
  const viewportRef = useRef<Viewport | null>(null);
  const syncRef = useRef<SceneSync | null>(null);
  const stringsRef = useRef<StringLayer | null>(null);
  const roadsRef = useRef<RoadLayer | null>(null);
  const layerRef = useRef('design');
  const dockRef = useRef<DockviewApi | null>(null);
  const [viewportHost] = useState(() => Object.assign(document.createElement('div'), { className: 'viewport-host' }));
  const [hostMounted, setHostMounted] = useState(false);
  const [summary, setSummary] = useState<ProjectSummary | null>(null);
  const [log, setLog] = useState<LogEntry[]>([]);
  const [stats, setStats] = useState<FrameStats | null>(null);
  const [backend, setBackend] = useState('starting…');
  const [status, setStatus] = useState<Status>({ text: 'Open a project or import a surface to start.' });
  const [mode, setModeState] = useState<Mode>('select');
  const [plan, setPlan] = useState(false);
  const [fieldOfView, setFieldOfView] = useState(FIELD_OF_VIEW_DEFAULT);
  const [snapping, setSnapping] = useState(true);
  const [sectionOpen, setSectionOpen] = useState(false);
  const [sectionStep, setSectionStep] = useState('25');
  const [sectionFill, setSectionFill] = useState(true);
  const sectionRef = useRef<SectionView | null>(null);
  const [activeLayer, setActiveLayerState] = useState('design');
  /** Everything selected, the last picked last. */
  const [selection, setSelectionState] = useState<string[]>([]);
  const selected = selection.at(-1) ?? null;
  const [seamViews, setSeamViews] = useState<SeamShown[]>([]);
  const [rlShown, setRlShown] = useState<RlShown>({ shared: false, objects: [] });
  const summaryRef = useRef(summary);
  summaryRef.current = summary;
  const seamsRef = useRef<SeamShown[]>([]);
  const selectionRef = useRef<string[]>([]);
  const [viewMenu, setViewMenu] = useState<ViewMenuAt | null>(null);
  const [deswikFile, setDeswikFile] = useState<{ path: string; layers: DeswikLayer[] } | null>(null);
  const [spryFile, setSpryFile] = useState<{ path: string; layers: PickableLayer[]; solidFields: Set<string> } | null>(null);
  const [notSolids, setNotSolids] = useState<NotASolid[] | null>(null);
  /** The solids the Export solids dialog is open on, with a name to offer for the zip. */
  const [solidsOut, setSolidsOut] = useState<{ solids: ObjectSummary[]; suggested: string } | null>(null);
  const markersRef = useRef<IssueMarkers | null>(null);
  const crossingMarkersRef = useRef<IssueMarkers | null>(null);
  /** An issue to fly to once its horizon has loaded. */
  const pendingFocus = useRef<{ id: string; horizon: string; issue: number } | null>(null);

  /** The seam models on show changed. */
  const onSeams = useCallback((showing: SeamShown[]) => {
    seamsRef.current = showing;
    setSeamViews(showing);
    markersRef.current?.set(showing.flatMap((s) => s.result.markers.map((m) => m.position)));
    const pending = pendingFocus.current;
    const view = showing.find((s) => s.id === pending?.id && s.result.horizon === pending.horizon);
    if (pending && view) {
      pendingFocus.current = null;
      const marker = view.result.markers.find((m) => m.issue === pending.issue);
      if (marker) viewportRef.current?.focusOn(marker.position);
    }
  }, []);

  /** The blocks coloured by a seam grid's value, as `model#horizon#variable`. */
  const [blocksColouredBy, setBlocksColouredBy] = useState<string | null>(null);

  /** The reserves field the blocks are coloured by (decision 0075, point 7). */
  const [fieldColoured, setFieldColoured] = useState<(FieldColour & { bands: Bands; count: number }) | null>(null);
  const colourBlocksByField = useCallback((field: FieldColour | null) => {
    if (!field) {
      setFieldColoured(null);
      void syncRef.current?.colourBlocks(null);
      return;
    }
    void reservesFieldValues(field.table, field.column, field.byBlock)
      .then(async (pairs) => {
        if (pairs.length === 0) {
          setStatus({ text: `No block has a ${field.column}: run the reserves (or Run again) so the table holds it`, error: true });
          return;
        }
        const values = pairs.map(([, v]) => v);
        const bands = valueBands(Math.min(...values), Math.max(...values));
        await syncRef.current?.colourBlocks({ values: new Map(pairs), bands });
        // One colouring of the blocks at a time: a seam grid's goes.
        setBlocksColouredBy(null);
        setFieldColoured({ ...field, bands, count: pairs.length });
        setStatus({ text: `${pairs.length} blocks coloured by ${field.column}, ${field.byBlock ? "each its block's" : 'each its own leaf'}` });
      })
      .catch((e) => setStatus({ text: String(e), error: true }));
  }, []);

  // The table run again (a revision): the blocks take its new values.
  const colouredTable = fieldColoured ? summary?.objects.find((o) => o.id === fieldColoured.table) : undefined;
  const colouredRevision = colouredTable?.revision ?? null;
  const colouredAt = useRef<number | null>(null);
  useEffect(() => {
    if (!fieldColoured) {
      colouredAt.current = null;
      return;
    }
    if (colouredRevision === null) return colourBlocksByField(null);
    if (colouredAt.current !== null && colouredAt.current !== colouredRevision) colourBlocksByField({ table: fieldColoured.table, column: fieldColoured.column, byBlock: fieldColoured.byBlock });
    colouredAt.current = colouredRevision;
  }, [colouredRevision, fieldColoured?.table]);

  const showSeamHorizon = useCallback((id: string, code: string, variable: string | null = null) => {
    setStatus({ text: `Reading the grids for ${code}…` });
    // Blocks coloured by another grid's bands would no longer read against what's shown.
    setBlocksColouredBy(null);
    setFieldColoured(null);
    void syncRef.current?.colourBlocks(null);
    void syncRef.current?.showHorizon(id, code, variable).then(() => setStatus({ text: `Showing ${code}, coloured by ${variable ?? 'thickness'}` }));
  }, []);

  const colourSeamBlocks = useCallback((view: SeamShown | null) => {
    const r = view?.result;
    if (!view || !r?.variable) {
      setBlocksColouredBy(null);
      void syncRef.current?.colourBlocks(null);
      return;
    }
    const variable = r.variable;
    void blockValues(view.id, r.horizon, 'seam', variable)
      .then(async (pairs) => {
        await syncRef.current?.colourBlocks({ values: new Map(pairs), bands: valueBands(r.value_range[0], r.value_range[1]) });
        setBlocksColouredBy(`${view.id}#${r.horizon}#${variable}`);
        setFieldColoured(null);
        setStatus({
          text:
            pairs.length === 0
              ? `No blocks of ${r.horizon} have a ${variable}: interrogate them first (Reserves)`
              : `${pairs.length} blocks of ${r.horizon} coloured by their ${variable}, on the grid's bands`,
        });
      })
      .catch((e) => setStatus({ text: String(e), error: true }));
  }, []);

  /** Shows the horizon an issue is on and flies to its worst node. */
  const typeVertex = useCallback(async (text: string) => (stringsRef.current ? stringsRef.current.addTyped(text) : 'The viewport is not ready'), []);
  const pickPoint = useCallback((prompt: string) => stringsRef.current?.pickPoint(prompt) ?? Promise.resolve(null), []);
  const pickLine = useCallback((first: string, second: string) => stringsRef.current?.pickLine(first, second) ?? Promise.resolve(null), []);
  const pickString = useCallback((prompt: string) => stringsRef.current?.pickString(prompt) ?? Promise.resolve(null), []);

  const focusIssue = useCallback(
    (id: string, horizon: string, issue: number) => {
      const view = seamsRef.current.find((s) => s.id === id);
      if (view?.result.horizon === horizon) {
        const marker = view.result.markers.find((m) => m.issue === issue);
        if (marker) viewportRef.current?.focusOn(marker.position);
        return;
      }
      pendingFocus.current = { id, horizon, issue };
      showSeamHorizon(id, horizon);
    },
    [showSeamHorizon],
  );

  /** World coordinates to the scene's local frame, which the viewport draws in. */
  const toLocal = useCallback((p: Vec3): Vec3 => {
    const o = summaryRef.current?.origin ?? [0, 0, 0];
    return [p[0] - o[0], p[1] - o[1], p[2] - o[2]];
  }, []);
  /** Runs a zoom; what it says back, if anything, goes on the status line. */
  const attemptZoom = async (zoom: () => Promise<string | null>) => {
    try {
      const why = await zoom();
      if (why) setStatus({ text: why });
    } catch (e) {
      setStatus({ text: String(e), error: true });
    }
  };
  const markCrossings = useCallback((sites: Vec3[]) => crossingMarkersRef.current?.set(sites.map(toLocal)), [toLocal]);
  const flyTo = useCallback((point: Vec3) => viewportRef.current?.focusOn(toLocal(point)), [toLocal]);
  const frameLine = useCallback(
    (points: Vec3[]) => {
      if (points.length === 0) return;
      const local = points.map(toLocal);
      const min = [0, 1, 2].map((k) => Math.min(...local.map((p) => p[k]))) as Vec3;
      const max = [0, 1, 2].map((k) => Math.max(...local.map((p) => p[k]))) as Vec3;
      viewportRef.current?.frameBox(min, max);
    },
    [toLocal],
  );
  const preferLines = useCallback((ids: string[]) => stringsRef.current?.setPreferred(ids), []);

  /** Shows a new project summary: panels, strings, and any surfaces whose geometry changed. */
  const show = useCallback((next: ProjectSummary) => {
    // At once, not at the next render: a panel selecting what a command just made looks it up here straight after.
    summaryRef.current = next;
    setSummary(next);
    void runLog().then(setLog);
    stringsRef.current?.sync(next);
    roadsRef.current?.sync(next);
    void sectionRef.current?.refresh(next);
    const sync = syncRef.current;
    const viewport = viewportRef.current;
    if (!sync || !viewport) return;
    const first = sync.count === 0;
    return sync.apply(next).then(async (loaded) => {
      if (loaded === 0) return;
      if (first) viewport.fitView();
      await buildSnapIndex();
    });
  }, []);

  /** Opens the section through two picked points, in place of the view it was picked in. */
  const openSection = useCallback((a: Vec3, b: Vec3) => {
    const [section, strings, project] = [sectionRef.current, stringsRef.current, summaryRef.current];
    if (!section || !project) return;
    strings?.setMode('select');
    strings?.setSection(section.tools);
    roadsRef.current?.setShown(false);
    setModeState('select');
    setSectionOpen(true);
    setStatus({ text: 'Section: [ and ] step along it, or drag on the plan map; Draw string works on the plane; the wheel zooms where the cursor is, left drag pans' });
    void section.show(planPoint(a), planPoint(b), project).catch((e) => setStatus({ text: String(e), error: true }));
  }, []);

  const closeSection = useCallback(() => {
    sectionRef.current?.close();
    stringsRef.current?.setSection(null);
    roadsRef.current?.setShown(true);
    setSectionOpen(false);
  }, []);

  const runRecord = useCallback(
    async (name: string, params: Record<string, unknown>) => {
      try {
        const reply = await runCommand(name, params);
        show(reply.summary);
        setStatus({ text: describe(reply.record) });
        return { record: reply.record, summary: reply.summary };
      } catch (e) {
        setStatus({ text: String(e), error: true });
        // A failed run is in the log too.
        void runLog().then(setLog);
        return { error: String(e) };
      }
    },
    [show],
  );

  const run = useCallback(async (name: string, params: Record<string, unknown>) => !!(await runRecord(name, params)).record, [runRecord]);

  const clearRefusal = useCallback(() => setStatus((was) => (was.error ? { text: '' } : was)), []);

  // Workflows (decision 0065): run, capture and add a step, each showing the project after it as any run does.
  const runWorkflow = useCallback(
    async (workflow: string, from: number, workflowCase?: string) => {
      try {
        const reply = await runWorkflowCall(workflow, from, workflowCase);
        show(reply.summary);
        const r = reply.record;
        setStatus({ text: workflowWords(r), error: r.status === 'refused' });
        return { record: reply.record };
      } catch (e) {
        setStatus({ text: String(e), error: true });
        void runLog().then(setLog);
        return { error: String(e) };
      }
    },
    [show],
  );
  const runAllCases = useCallback(
    async (workflow: string) => {
      try {
        const reply = await runAllCasesCall(workflow);
        show(reply.summary);
        setStatus({ text: reply.records.map(workflowWords).join(' · '), error: reply.records.some((r) => r.status === 'refused') });
        return { records: reply.records };
      } catch (e) {
        setStatus({ text: String(e), error: true });
        void runLog().then(setLog);
        return { error: String(e) };
      }
    },
    [show],
  );
  const workflowRun = useCallback(
    async (call: () => Promise<{ record: RunRecord; summary: ProjectSummary }>) => {
      try {
        const reply = await call();
        show(reply.summary);
        setStatus({ text: describe(reply.record) });
        return { record: reply.record };
      } catch (e) {
        setStatus({ text: String(e), error: true });
        void runLog().then(setLog);
        return { error: String(e) };
      }
    },
    [show],
  );
  const captureWorkflow = useCallback(
    async (name: string, runs: string[]) => {
      const outcome = await workflowRun(() => captureWorkflowCall(name, runs));
      if (outcome.record && dockRef.current) showPanel(dockRef.current, 'workflow');
      return outcome;
    },
    [workflowRun],
  );
  const [addingTo, setAddingTo] = useState<AddingTo | null>(null);
  const [toolAsk, setToolAsk] = useState<{ panel: PanelId; mode?: string; n: number } | null>(null);
  const openTool = useCallback((panel: PanelId, mode?: string) => {
    if (dockRef.current) showPanel(dockRef.current, panel);
    setToolAsk((was) => ({ panel, mode, n: (was?.n ?? 0) + 1 }));
  }, []);
  // The column the Notebook tab shows (decision 0081), asked for from the Reserves table's Open its column.
  const [notebookOf, setNotebookOf] = useState<{ table: string; node: string[]; n: number; edited: NotebookEdit | null } | null>(null);
  const openNotebook = useCallback((table: string, node: string[]) => {
    if (dockRef.current) showPanel(dockRef.current, 'notebook');
    setNotebookOf((was) => ({ table, node, n: (was?.n ?? 0) + 1, edited: null }));
  }, []);
  // The open notebook's column again, on a formula as edited (decision 0081, point 7); nothing to do with none open.
  const runInNotebook = useCallback((edited: NotebookEdit | null) => {
    if (dockRef.current) showPanel(dockRef.current, 'notebook');
    setNotebookOf((was) => (was ? { ...was, n: was.n + 1, edited } : was));
  }, []);
  // The block picked in the Reserves table: the Notebook follows it, a formula run as edited carried on to it.
  const followInNotebook = useCallback((table: string, node: string[]) => {
    setNotebookOf((was) =>
      was && was.table === table && was.node.length === node.length && was.node.every((w, i) => w === node[i]) ? was : { table, node, n: (was?.n ?? 0) + 1, edited: was?.edited ?? null },
    );
  }, []);
  // The Parameters tab's unsaved edits, by object (decision 0082): the Dataflow tab runs on them live.
  const [drafts, setDrafts] = useState<Record<string, Draft>>({});
  const setDraft = useCallback((id: string, draft: Draft | null) => {
    setDrafts((was) => {
      if (draft === null) {
        if (!(id in was)) return was;
        const { [id]: _gone, ...rest } = was;
        return rest;
      }
      return { ...was, [id]: draft };
    });
  }, []);
  const [workflowPick, setWorkflowPick] = useState<{ id: string | null; case: string }>({ id: null, case: 'Base' });
  // What the ribbon lights: the panels in front in their docks, and what each is set to do, as the panels say.
  const [panelsShown, setPanelsShown] = useState<PanelId[]>([]);
  const [panelModes, setPanelModes] = useState<Partial<Record<PanelId, string>>>({});
  const toolMode = useCallback((panel: PanelId, mode: string | null) => {
    setPanelModes((was) => {
      if ((was[panel] ?? null) === mode) return was;
      const { [panel]: _gone, ...rest } = was;
      return mode === null ? rest : { ...rest, [panel]: mode };
    });
  }, []);
  // Find a command, open or not.
  const [finding, setFinding] = useState(false);
  // The stage whose tools the ribbon shows, remembered for each project on this machine.
  const [stage, setStageState] = useState<StageId>(() => stageOf(null));
  const setStage = useCallback((next: StageId) => {
    setStageState(next);
    rememberStage(summaryRef.current?.path ?? null, next);
  }, []);

  const addStep = useCallback(
    async (command: string, params: Record<string, unknown>) => {
      if (!addingTo) return { error: 'Pick a workflow to add to first' };
      const outcome = await workflowRun(() => addWorkflowStep(addingTo.id, command, params, undefined, addingTo.at));
      // The next step added goes after this one, so steps added one after another stay in that order.
      if (outcome.record && addingTo.at !== undefined) setAddingTo({ ...addingTo, at: addingTo.at + 1 });
      return outcome;
    },
    [addingTo, workflowRun],
  );

  const viewportMounted = useCallback(() => setHostMounted(true), []);

  const setSelection = useCallback((ids: string[]) => {
    // Once each, where it was last: the last is the one a panel showing one takes.
    const once = ids.filter((id, k) => ids.lastIndexOf(id) === k);
    selectionRef.current = once;
    setSelectionState(once);
  }, []);

  const select = useCallback((id: string | null) => setSelection(id ? [id] : []), [setSelection]);

  const toggleSelected = useCallback(
    (id: string) => {
      const was = selectionRef.current;
      setSelection(was.includes(id) ? was.filter((other) => other !== id) : [...was, id]);
    },
    [setSelection],
  );

  /** Frames objects from the current direction, as a line from a panel is; in plan, straight above them. */
  const zoomTo = useCallback(
    (ids: string[]) =>
      void attemptZoom(async () => {
        if (sectionRef.current?.open) return 'Close the section to zoom to what is selected';
        if (ids.length === 0) return 'Nothing is selected to zoom to';
        const box = await objectsBounds(ids);
        if (!box) return 'Nothing selected has a place to zoom to';
        viewportRef.current?.frameBox(toLocal(box[0]), toLocal(box[1]));
        return null;
      }),
    [toLocal],
  );

  /**
   * Show for a note (decision 0059): picks what it's about and takes the view to its place, a point, the stretch it
   * names, or a point in plan at the middle of its object's RLs, as a point in plan has none of its own.
   */
  const showNote = useCallback(
    (note: Note) => {
      if (note.object) select(note.object);
      const place = note.place;
      if (!place) {
        if (note.object) zoomTo([note.object]);
        return;
      }
      if (place.kind === 'point') return flyTo(place.at);
      if (place.kind === 'stretch') return frameLine([place.from, place.to]);
      void attemptZoom(async () => {
        const box = note.object ? await objectsBounds([note.object]) : null;
        flyTo([place.at[0], place.at[1], box ? (box[0][2] + box[1][2]) / 2 : 0]);
        return null;
      });
    },
    [select, zoomTo, flyTo, frameLine],
  );

  /** Frames everything loaded (Shift+F). */
  const zoomToExtents = useCallback(() => {
    if (sectionRef.current?.open) return setStatus({ text: 'Close the section to zoom to everything' });
    viewportRef.current?.fitView();
  }, []);
  /** F: the selection if there is one, else everything, as Spry's F. */
  const zoomToSelection = useCallback(() => (selectionRef.current.length > 0 ? zoomTo(selectionRef.current) : zoomToExtents()), [zoomTo, zoomToExtents]);

  /** Selects what lies wholly inside a box on the viewport, or adds it to what's selected. */
  const boxSelect = useCallback(
    async (min: [number, number], max: [number, number], add: boolean) => {
      const viewport = viewportRef.current;
      if (!viewport) return;
      const camera = viewport.camera;
      camera.updateMatrixWorld();
      const matrix = camera.projectionMatrix.clone().multiply(camera.matrixWorldInverse).elements;
      try {
        const inside = await objectsInBox([...matrix], min, max);
        setSelection(add ? [...selectionRef.current, ...inside] : inside);
        const count = inside.length === 1 ? 'One object' : `${inside.length} objects`;
        setStatus({ text: inside.length === 0 ? 'Nothing on show lies wholly inside that box' : `${count} inside the box${add ? ' added' : ''}` });
      } catch (e) {
        setStatus({ text: String(e), error: true });
      }
    },
    [setSelection],
  );

  // The viewport is made once its canvas host is in the page (the viewport panel mounts it).
  useEffect(() => {
    if (!hostMounted) return;
    let viewport: Viewport | null = null;
    let detach = () => {};
    let cancelled = false;
    // In bench mode `--webgl` runs the viewport on WebGL2, so the fallback can be checked where WebGPU is there.
    void benchConfig().then((config) => Viewport.create(viewportHost, { webgl: config?.webgl ?? false })).then(async (created) => {
      if (cancelled) return created.dispose();
      viewport = created;
      viewportRef.current = created;
      setFieldOfView(created.fieldOfView);
      created.onPlanChange = setPlan;
      created.onStats = setStats;
      setBackend(`${created.backend} · ${created.adapter}`);
      markersRef.current = new IssueMarkers(created);
      crossingMarkersRef.current = new IssueMarkers(created);
      const sync = new SceneSync(created);
      sync.onSeams = onSeams;
      sync.onRl = setRlShown;
      sync.onError = (text) => setStatus({ text, error: true });
      syncRef.current = sync;
      const section = new SectionView(created);
      sectionRef.current = section;
      const strings = new StringLayer(created, {
        run,
        section: (a, b) => void openSection(a, b),
        snap: (x, y) => {
          const [nx, ny] = created.clientToNdc(x, y);
          const ray = created.rayFromNdc(nx, ny);
          return snapRay(ray.origin, ray.direction);
        },
        snapDown: (x, y) => snapRay([x, y, 1e6], [0, 0, -1]),
        status: (text, error) => setStatus({ text, error }),
        layer: () => layerRef.current,
        toSelect: () => setMode('select'),
        remove: () => void deleteSelection(),
        pick: (x, y, add) =>
          void objectAt(created, sync, section, x, y).then((id) => {
            if (!add) select(id);
            else if (id) toggleSelected(id);
          }),
        choose: (id, add) => (add && id ? toggleSelected(id) : select(id)),
        box: (min, max, add) => void boxSelect(min, max, add),
        menu: (x, y, string) =>
          void (string ? Promise.resolve(string) : objectAt(created, sync, section, x, y)).then((id) => {
            const now = selectionRef.current;
            // A right click on something not picked picks it alone, as a left click would; on nothing, the menu is for what's picked.
            if (id && !now.includes(id)) select(id);
            setViewMenu({ x, y, ids: id && !now.includes(id) ? [id] : now, n: Date.now() });
          }),
      });
      strings.onSnapping = setSnapping;
      section.mapShows = [strings.lines];
      section.onUnfilled = (count) => setStatus({ text: `${count === 1 ? 'A closed loop' : `${count} closed loops`} in this section could not be filled: the lines show as cut` });
      created.pickPoint = (x, y) => strings.pick(x, y);
      stringsRef.current = strings;
      roadsRef.current = new RoadLayer(created);
      detach = strings.attach();
      show(await projectSummary());
      const config = await benchConfig();
      if (config) {
        // Phase 0 timings were taken with the viewport filling the window; solids go through the panels.
        if (!config.solids && !config.shell) dockRef.current?.getPanel('viewport')?.api.maximize();
        await created.nextFrame();
        await created.nextFrame();
        await runChecks(created, config, (text) => setStatus({ text }), show, () => sectionRef.current, () => stringsRef.current, () => selectionRef.current, () => dockRef.current);
      }
    });
    return () => {
      cancelled = true;
      detach();
      viewport?.dispose();
    };
  }, [hostMounted, viewportHost, run, show, onSeams, openSection, select, toggleSelected, boxSelect]);

  // What's selected lights up. A section shows the last one's cut lines over the rest.
  useEffect(() => {
    void syncRef.current?.select(selection);
    sectionRef.current?.setSelected(selection.at(-1) ?? null);
  }, [selection]);

  // The last one picked, if it's a string, gets handles to drag; the other strings light up over their layers.
  useEffect(() => {
    const strings = stringsRef.current;
    const last = selection.at(-1) ?? null;
    strings?.showSelected(last);
    strings?.setMarked(selection.filter((id) => id !== last));
  }, [selection, summary]);

  // What a run deleted, or an undo took back, isn't selected any longer.
  useEffect(() => {
    if (!summary) return;
    const live = new Set(summary.objects.map((o) => o.id));
    if (selectionRef.current.some((id) => !live.has(id))) setSelection(selectionRef.current.filter((id) => live.has(id)));
  }, [summary, setSelection]);

  const attempt = useCallback(async (work: () => Promise<void>) => {
    try {
      await work();
    } catch (e) {
      setStatus({ text: String(e), error: true });
    }
  }, []);

  const setActiveLayer = useCallback((name: string) => {
    layerRef.current = name.trim() || 'design';
    setActiveLayerState(name);
  }, []);

  const onUndo = () => attempt(async () => show(await undo()));
  const onRedo = () => attempt(async () => show(await redo()));

  const onSave = (saveAs: boolean) =>
    attempt(async () => {
      let path: string | undefined;
      if (saveAs || !summary?.path) {
        const chosen = await save({ title: 'Save project as', defaultPath: 'project.zdp', filters: [{ name: 'Project', extensions: ['zdp'] }] });
        if (!chosen) return;
        path = chosen.toLowerCase().endsWith('.zdp') ? chosen : `${chosen}.zdp`;
      }
      setStatus({ text: 'Saving…' });
      const reply = await saveProject(path);
      setStatus({
        text: reply.unchanged
          ? `Nothing has changed since the last save of ${reply.path}`
          : `Saved ${reply.path}: ${(reply.bytes / 1e6).toFixed(1)} MB in ${reply.ms.toFixed(0)} ms`,
      });
      show(await projectSummary());
    });

  /**
   * Seam models' grids checked where they are now (decision 0058, stage 4). What opening relinked is said; a folder
   * with grids missing is pointed at where it is now, the whole folder in one step, every file checked there first.
   * `asked` is the menu's Relink geology, which says so when everything is where it was.
   */
  const relinkGeology = async (asked: boolean) => {
    const { notes, models } = await checkLinks();
    const lost = new Map<string, { missing: number; files: number; models: ModelLinks[] }>();
    for (const m of models)
      for (const f of m.folders.filter((f) => f.missing > 0)) {
        const was = lost.get(f.folder) ?? { missing: 0, files: 0, models: [] };
        lost.set(f.folder, { missing: was.missing + f.missing, files: was.files + f.files, models: [...was.models, m] });
      }
    const changed = models.filter((m) => m.changed.length > 0);
    if (notes.length > 0) void runLog().then(setLog);
    const lines = [...notes];
    for (const [folder, { missing, files, models: by }] of lost)
      lines.push(`${by.map((m) => m.name).join(', ')}: ${missing} of ${files} grid files aren't in ${folder}`);
    for (const m of changed)
      lines.push(`${m.name}: ${m.changed.length} grid files have changed since it was built, so it is out of date until it is rebuilt`);
    if (lines.length === 0) {
      if (asked) await message(models.length ? "Every seam model's grids are where they were, as they were." : 'This project has no seam models.', { title: 'Geology links' });
      return;
    }
    if (lost.size === 0) {
      await message(lines.join('\n\n'), { title: 'Geology links', kind: changed.length > 0 ? 'warning' : 'info' });
      return;
    }
    const go = await confirm(`${lines.join('\n\n')}\n\nPick where each folder is now?`, { title: 'Geology links', kind: 'warning', okLabel: 'Relink…', cancelLabel: 'Not now' });
    if (!go) return;
    for (const [folder, { models: by }] of lost) {
      const to = await open({ directory: true, title: `Where are the grids that were in ${folder}?` });
      if (!to || Array.isArray(to)) continue;
      setStatus({ text: `Checking the grids in ${to}…` });
      await runRecord('seam.relink', { from: folder, to, models: by.map((m) => m.object) });
    }
  };

  /** Opens a `.zdp` project file, or an older `.zdproj` folder, which saves as a `.zdp` beside it (decision 0058). */
  const onOpen = (folder: boolean) =>
    attempt(async () => {
      const path = folder
        ? await open({ directory: true, title: 'Open an older project folder (.zdproj)' })
        : await open({ title: 'Open project', filters: [{ name: 'Project', extensions: ['zdp'] }] });
      if (!path) return;
      setStatus({ text: `Opening ${path}…` });
      select(null);
      const opened = await openProject(path);
      show(opened);
      setStatus({ text: `Opened ${path}` });
      if (opened.locked_by)
        await message(`${opened.locked_by} has this project open. You can look and work, but a save to it is refused: use Save as to keep your changes in another file.`, {
          title: 'Project in use',
          kind: 'warning',
        });
      await relinkGeology(false);
    });

  const onNew = () =>
    attempt(async () => {
      if (!(await confirm('Start a new project? Anything unsaved in this one is lost.', { title: 'New project', kind: 'warning' }))) return;
      select(null);
      show(await newProject());
      setStatus({ text: 'New project' });
    });

  const onImportSurface = () =>
    attempt(async () => {
      const picked = await open({ multiple: true, filters: [{ name: 'Surfaces', extensions: ['00t', 'dxf', 'xyz', 'csv', 'txt', 'asc'] }] });
      const paths = picked === null ? [] : Array.isArray(picked) ? picked : [picked];
      for (const path of paths) {
        setStatus({ text: `Importing ${path}…` });
        await run('surface.import', { path, layer: 'surfaces' });
      }
    });

  /**
   * Imports solids from files and zips, or from folders, all in one run (decision 0019); a refused
   * file says why in the run log. With `repair`, hanging sheets and fins come off, cracks are joined
   * and holes up to 10 % of the surface are filled, each noted on the solid (decisions 0013, 0015).
   */
  const onImportSolids = (from: 'files' | 'folder', repair: boolean) =>
    attempt(async () => {
      const picked = await open(
        from === 'folder' ? { directory: true, multiple: true } : { multiple: true, filters: [{ name: 'Solids', extensions: ['zip', '00t', 'dxf'] }] },
      );
      const paths = picked === null ? [] : Array.isArray(picked) ? picked : [picked];
      if (paths.length === 0) return;
      setStatus({ text: `Importing solids${repair ? ', repairing' : ''} from ${paths.length === 1 ? paths[0] : `${paths.length} places`}…` });
      await run('solid.import_many', { paths, layer: 'solids', repair });
    });

  /** Lists a Deswik drawing's layers to pick from; the dialog then imports those ticked (decision 0036). */
  const onImportDeswik = () =>
    attempt(async () => {
      const picked = await open({ title: 'Pick a Deswik drawing', filters: [{ name: 'Deswik drawings', extensions: ['dcf'] }] });
      if (!picked || Array.isArray(picked)) return;
      setStatus({ text: `Listing the layers of ${picked}…` });
      const layers = await deswikLayers(picked);
      setStatus({ text: `${layers.length} layers in ${picked}: tick those to import` });
      setDeswikFile({ path: picked, layers });
    });

  const importDeswik = (choice: DeswikChoice) => {
    const file = deswikFile;
    setDeswikFile(null);
    if (!file) return;
    void attempt(async () => {
      setStatus({ text: `Importing ${choice.layers.length} layers from ${file.path}…` });
      offerRepair(await runRecord('deswik.import', { path: file.path, layers: choice.layers, meshes: choice.meshes, repair: choice.repair, layer: choice.layer || null }));
    });
  };

  /** Lists a Spry project's Solid fields and design layers to pick from; the dialog then imports those ticked (decision 0039). */
  const onImportSpry = () =>
    attempt(async () => {
      const picked = await open({ title: 'Pick a Spry project', filters: [{ name: 'Spry projects', extensions: ['spry'] }] });
      if (!picked || Array.isArray(picked)) return;
      setStatus({ text: `Listing what ${picked} holds…` });
      const contents = await spryContents(picked);
      const fields = contents.solidFields.map((f) => ({ name: f.name, bytes: f.bytes, detail: `${f.solids.toLocaleString()} solids` }));
      setStatus({ text: `${fields.length} Solid fields and ${contents.layers.length} layers in ${picked}: tick those to import` });
      setSpryFile({ path: picked, layers: [...fields, ...contents.layers], solidFields: new Set(fields.map((f) => f.name)) });
    });

  const importSpry = (choice: DeswikChoice) => {
    const file = spryFile;
    setSpryFile(null);
    if (!file) return;
    void attempt(async () => {
      setStatus({ text: `Importing ${choice.layers.length} Solid fields and layers from ${file.path}…` });
      offerRepair(await runRecord('spry.import', {
        path: file.path,
        solid_fields: choice.layers.filter((name) => file.solidFields.has(name)),
        layers: choice.layers.filter((name) => !file.solidFields.has(name)),
        meshes: choice.meshes,
        repair: choice.repair,
        layer: choice.layer || null,
      }));
    });
  };

  /** After an import, offers to fix the closed meshes it kept as surfaces, as they couldn't be solids (decision 0040). */
  const offerRepair = (reply: { record?: RunRecord; summary?: ProjectSummary }) => {
    if (!reply.record || !reply.summary) return;
    const made = new Set(reply.record.outputs.filter((o) => o.change === 'created').map((o) => o.id));
    const flagged = reply.summary.objects.flatMap((o) => {
      const why = o.attributes.not_a_solid;
      return made.has(o.id) && typeof why === 'string' ? [{ id: o.id, name: o.name, why }] : [];
    });
    if (flagged.length > 0) setNotSolids(flagged);
  };

  /** Repairs those alone, each one fixed replacing its surface; undo brings them back. */
  const fixNotSolids = (ids: string[]) => {
    setNotSolids(null);
    void attempt(async () => {
      setStatus({ text: `Repairing ${ids.length} closed meshes into solids…` });
      await run('solid.repair', { surfaces: ids });
    });
  };

  const onImportStrings = () =>
    attempt(async () => {
      const picked = await open({ multiple: true, filters: [{ name: 'Strings', extensions: ['dxf', 'csv'] }] });
      const paths = picked === null ? [] : Array.isArray(picked) ? picked : [picked];
      for (const path of paths) {
        setStatus({ text: `Importing strings from ${path}…` });
        // Each string keeps its DXF or CSV layer.
        await run('string.import', { path });
      }
    });

  /** Writes objects to a file picked in the save dialog, which has already asked before replacing one. */
  const exportObjects = (objects: ObjectSummary[], format: 'dxf' | 'csv', suggested: string) =>
    attempt(async () => {
      if (objects.length === 0) {
        setStatus({ text: format === 'dxf' ? 'Nothing to export: no strings, surfaces or solids are visible.' : 'Nothing to export: no strings are visible.' });
        return;
      }
      const name = format === 'dxf' ? 'DXF R12' : 'CSV';
      const path = await save({ title: `Export to ${name}`, defaultPath: fileName(suggested, format), filters: [{ name, extensions: [format] }] });
      if (!path) return;
      setStatus({ text: `Writing ${path}…` });
      await run(`export.${format}`, { objects: objects.map((o) => o.id), path, overwrite: true });
    });

  /** Opens the Export solids dialog on these solids, to pick how their files are named (decision 0057). */
  const askSolidsOut = (solids: ObjectSummary[], suggested: string) => {
    if (solids.length === 0) setStatus({ text: 'Nothing to export: no solids are visible.' });
    else setSolidsOut({ solids, suggested });
  };

  /** Writes the dialog's solids to a zip picked in the save dialog: a DXF a solid, with a list of them. */
  const exportSolids = (options: SolidExportOptions) =>
    attempt(async () => {
      if (!solidsOut) return;
      const { solids, suggested } = solidsOut;
      const path = await save({ title: 'Export solids to a zip', defaultPath: fileName(suggested, 'zip'), filters: [{ name: 'Zip of DXF R12, a file a solid', extensions: ['zip'] }] });
      if (!path) return;
      setSolidsOut(null);
      setStatus({ text: `Writing ${solids.length} solids to ${path}…` });
      await run('solid.export_many', { objects: solids.map((o) => o.id), path, overwrite: true, ...options });
    });

  // A set: a box over a pit's blocks selects tens of thousands.
  const selectedObjects = useMemo(() => {
    const ids = new Set(selection);
    return summary?.objects.filter((o) => ids.has(o.id)) ?? [];
  }, [summary, selection]);
  const selectedDxf = selectedObjects.filter((o) => DXF_KINDS.has(o.kind));
  const selectedStrings = selectedObjects.filter((o) => o.kind === 'string');
  const selectedSolids = selectedObjects.filter((o) => o.kind === 'solid');
  const visibleObjects = (kinds: Set<string>) => {
    // Objects carry their layer's id.
    const shown = new Set(summary?.layers.filter((l) => l.visible).map((l) => l.id));
    return summary?.objects.filter((o) => kinds.has(o.kind) && shown.has(o.layer) && !o.hidden) ?? [];
  };

  const onImportVulcan = () =>
    attempt(async () => {
      const picked = await open({ title: 'Pick a roof, floor or topography grid: the grids of that name come with it', filters: [{ name: 'Vulcan grids', extensions: ['srg', 'sfg', 'tpg'] }] });
      if (!picked || Array.isArray(picked)) return;
      const [dir, file] = splitPath(picked);
      const stem = file.slice(0, file.lastIndexOf('.'));
      setStatus({ text: `Importing ${stem}…` });
      await run('grid.import_vulcan_seam', { dir, prefix: '', seam: stem, layer: stem.toUpperCase() });
    });

  /** Writes a starting stack definition for a folder of Vulcan grids, to check and edit in Excel. */
  const onSuggestSeamDefinition = () =>
    attempt(async () => {
      const picked = await open({ title: 'Pick any grid in the folder of seam grids: roofs and floors are found from there', filters: [{ name: 'Vulcan roof and floor grids', extensions: ['srg', 'sfg'] }] });
      if (!picked || Array.isArray(picked)) return;
      const [dir] = splitPath(picked);
      const path = await save({ title: 'Save the seam stack definition (CSV)', defaultPath: 'seam-stack.csv', filters: [{ name: 'CSV', extensions: ['csv'] }] });
      if (!path) return;
      setStatus({ text: `Reading the seam grids in ${dir}…` });
      // The save dialog has already asked before replacing a file.
      await run('seam.suggest_definition', { dir, prefix: '', path, overwrite: true });
    });

  /** Builds and checks a seam model from a stack definition, then selects it to show its issues. */
  const onBuildSeamModel = () =>
    attempt(async () => {
      const picked = await open({ title: 'Pick a seam stack definition (CSV)', filters: [{ name: 'CSV', extensions: ['csv'] }] });
      if (!picked || Array.isArray(picked)) return;
      setStatus({ text: `Building the seam model from ${picked}…` });
      try {
        // Each model on a layer of its own, so it can be told apart and hidden.
        const [, file] = splitPath(picked);
        const reply = await runCommand('seam.build_model', { definition: picked, layer: `Seam models\\${file.slice(0, file.lastIndexOf('.'))}` });
        show(reply.summary);
        setStatus({ text: describe(reply.record) });
        const made = reply.record.outputs.find((o) => o.change === 'created');
        if (made) select(made.id);
      } finally {
        void runLog().then(setLog);
      }
    });

  const setMode = (next: Mode) => {
    stringsRef.current?.setMode(next);
    setModeState(next);
    setStatus({
      text:
        next === 'section'
          ? 'Section: pick the first point'
          : next === 'measure'
            ? 'Measure: click two points for the distance, RL difference, grade and bearing between them; click on for a running total; right-click ends one. Backspace takes a point off, Esc clears, M stops measuring'
          : next === 'draw'
            ? 'Draw: click on a surface to add vertices; right-click, Enter or double-click finishes, Backspace removes the last, Esc abandons'
            : 'Select: click a string, drag its handles; Alt+click inserts a vertex, Delete removes the one under the cursor',
    });
  };

  /**
   * Delete (the key, or Find a command): everything selected, as one run and one undo (decision 0080), and it says so.
   * The Delete key over a handle of the selected string deletes that vertex instead (`strings.ts`).
   */
  const deleteSelection = async () => {
    const ids = selectionRef.current;
    if (ids.length === 0) return setStatus({ text: 'Nothing selected to delete' });
    if (!(await run('object.delete_many', { objects: ids }))) return;
    select(null);
    setStatus({ text: `Deleted ${ids.length === 1 ? '1 object' : `${ids.length} objects`}: Ctrl+Z brings ${ids.length === 1 ? 'it' : 'them'} back` });
  };

  /** Plan or 3D over the same spot (T, or the view bar). A section open is closed first. */
  const togglePlan = () => {
    const viewport = viewportRef.current;
    if (!viewport) return;
    if (sectionOpen) closeSection();
    viewport.setPlanView(!viewport.planView);
  };

  /** Section (2, or the view bar): picking its two points starts, or stops if it had started; an open one closes. */
  const toggleSection = () => {
    if (sectionOpen) return closeSection();
    setMode(mode === 'section' ? 'select' : 'section');
  };

  /** Measure (M, or the view bar): starts measuring between clicks, or ends it. */
  const toggleMeasure = () => setMode(mode === 'measure' ? 'select' : 'measure');

  const stepSection = (direction: number) => {
    const distance = Number(sectionStep);
    if (Number.isFinite(distance) && distance > 0) void sectionRef.current?.step(direction * distance);
  };

  // Shortcuts: undo, redo, save, zoom, and stepping a section.
  useEffect(() => {
    const key = (e: KeyboardEvent) => {
      // Find a command, from anywhere, typing in a box included.
      if ((e.ctrlKey || e.metaKey) && !e.altKey && e.key.toLowerCase() === 'k') {
        e.preventDefault();
        return setFinding((was) => !was);
      }
      if (e.target instanceof HTMLInputElement) return;
      if (sectionOpen && (e.key === '[' || e.key === ']')) return stepSection(e.key === ']' ? 1 : -1);
      const plain = !(e.ctrlKey || e.metaKey || e.altKey) && !typing(e.target);
      if (e.key.toLowerCase() === 'f' && plain) {
        e.preventDefault();
        return e.shiftKey ? zoomToExtents() : zoomToSelection();
      }
      if (e.key.toLowerCase() === 't' && plain && !e.shiftKey) {
        e.preventDefault();
        return togglePlan();
      }
      if (e.key === ALL_PANELS_KEY && plain) {
        e.preventDefault();
        return dockRef.current && toggleAllPanels(dockRef.current);
      }
      if (e.key === '2' && plain) {
        e.preventDefault();
        return toggleSection();
      }
      if (e.key.toLowerCase() === 'm' && plain && !e.shiftKey) {
        e.preventDefault();
        return toggleMeasure();
      }
      if (e.key.toLowerCase() === 'e' && plain && !e.shiftKey) {
        e.preventDefault();
        return zoomToExtents();
      }
      if (!(e.ctrlKey || e.metaKey)) return;
      const k = e.key.toLowerCase();
      if (k === 'z' && !e.shiftKey) {
        e.preventDefault();
        void onUndo();
      } else if (k === 'y' || (k === 'z' && e.shiftKey)) {
        e.preventDefault();
        void onRedo();
      } else if (k === 's') {
        e.preventDefault();
        void onSave(false);
      }
    };
    window.addEventListener('keydown', key);
    return () => window.removeEventListener('keydown', key);
  });

  const setVisible = useCallback(
    (layer: string, visible: boolean) => void attempt(async () => show(await setLayerVisible(layer, visible))),
    [attempt, show],
  );

  const setLayersShown = useCallback(
    async (on: string[], off: string[]) => {
      let last: ProjectSummary | null = null;
      try {
        for (const layer of on) last = await setLayerVisible(layer, true);
        for (const layer of off) last = await setLayerVisible(layer, false);
      } catch (e) {
        setStatus({ text: String(e), error: true });
      }
      if (last) await show(last);
    },
    [show],
  );

  const setObjectsShown = useCallback(
    (objects: string[], visible: boolean) => void attempt(async () => show(await setObjectsVisible(objects, visible))),
    [attempt, show],
  );

  /** Puts new looks on screen: no geometry moves, so only the summary's display and the materials change. */
  const restyle = useCallback((display: Display) => {
    const current = summaryRef.current;
    if (!current) return;
    const next = { ...current, display };
    summaryRef.current = next;
    setSummary(next);
    void syncRef.current?.apply(next);
  }, []);

  const changeLooks = useCallback(
    (objects: string[], colouring: Colouring | null, opacity: number | null, commit = true) => {
      const current = summaryRef.current;
      if (!current) return;
      restyle(withLooks(current.display, objects, colouring, opacity));
      if (commit) void attempt(async () => restyle(await setLooks(objects, colouring, opacity)));
    },
    [attempt, restyle],
  );

  const changeLayerOpacity = useCallback(
    (layer: string, opacity: number, commit = true) => {
      const current = summaryRef.current;
      if (!current) return;
      restyle(withLayerOpacity(current.display, layer, opacity));
      if (commit) void attempt(async () => restyle(await setLayerOpacity(layer, opacity)));
    },
    [attempt, restyle],
  );

  const changeContours = useCallback(
    (contours: Contours) => {
      const current = summaryRef.current;
      if (!current) return;
      restyle({ ...current.display, contours });
      void attempt(async () => restyle(await setContours(contours)));
    },
    [attempt, restyle],
  );

  const workspace = useMemo<WorkspaceState>(
    () => ({
      summary,
      log,
      selected,
      selection,
      activeLayer,
      viewportHost,
      viewportMounted,
      select,
      setSelection,
      toggleSelected,
      zoomTo,
      setActiveLayer,
      setLayerVisible: setVisible,
      setLayersVisible: setLayersShown,
      setObjectsVisible: setObjectsShown,
      setLooks: changeLooks,
      setLayerOpacity: changeLayerOpacity,
      setContours: changeContours,
      rlShown,
      openPanel: (id) => dockRef.current && showPanel(dockRef.current, id),
      openTool,
      toolAsk,
      toolMode,
      notebookOf,
      openNotebook,
      runInNotebook,
      followInNotebook,
      drafts,
      setDraft,
      placeBeside: (id, beside) => dockRef.current && placeBeside(dockRef.current, id, beside),
      isBeside: (id, beside) => !!dockRef.current && isBeside(dockRef.current, id, beside),
      run,
      runRecord,
      seamViews,
      colourSeamBlocks,
      blocksColouredBy,
      colourBlocksByField,
      fieldColoured,
      sectionOpen,
      lineWeight: (layer) => sectionRef.current?.weightOf(layer) ?? null,
      setLineWeight: (layer, px) => sectionRef.current?.setWeight(layer, px),
      showSeamHorizon,
      focusIssue,
      clearRefusal,
      runWorkflow,
      runAllCases,
      captureWorkflow,
      addingTo,
      setAddingTo,
      workflowPick,
      setWorkflowPick,
      addStep,
      markCrossings,
      flyTo,
      frameLine,
      preferLines,
      pickPoint,
      pickLine,
      pickString,
      drawing: mode === 'draw',
      typeVertex,
      viewMenu,
      closeViewMenu: () => setViewMenu(null),
    }),
    [viewMenu, summary, log, selected, selection, activeLayer, viewportHost, viewportMounted, select, setSelection, toggleSelected, zoomTo, setActiveLayer, setVisible, setLayersShown, setObjectsShown, changeLooks, changeLayerOpacity, changeContours, rlShown, run, runRecord, clearRefusal, runWorkflow, runAllCases, captureWorkflow, addingTo, workflowPick, addStep, seamViews, colourSeamBlocks, blocksColouredBy, colourBlocksByField, fieldColoured, sectionOpen, showSeamHorizon, focusIssue, markCrossings, flyTo, frameLine, preferLines, pickPoint, pickLine, pickString, mode, typeVertex, openTool, toolAsk, toolMode, notebookOf, openNotebook, runInNotebook, drafts],
  );

  /** The view's own tools, for the bar on the view (decision 0076). The keys below do the same. */
  const viewTools: ViewTools = {
    mode,
    setMode,
    snapping,
    toggleSnapping: () => stringsRef.current?.toggleSnapping(),
    plan,
    togglePlan,
    hasSelection: selection.length > 0,
    zoomToSelection: () => zoomTo(selection),
    zoomToExtents,
    fieldOfView,
    setFieldOfView: (degrees) => {
      viewportRef.current?.setFieldOfView(degrees);
      setFieldOfView(viewportRef.current?.fieldOfView ?? degrees);
    },
    sectionOpen,
    toggleSection,
    stepSection,
    sectionStep,
    setSectionStep: (text) => {
      setSectionStep(text);
      const distance = Number(text);
      if (sectionRef.current && Number.isFinite(distance) && distance > 0) sectionRef.current.stepSize = distance;
    },
    sectionFill,
    setSectionFill: (on) => {
      setSectionFill(on);
      if (sectionRef.current) sectionRef.current.filled = on;
    },
  };

  const projectPath = summary?.path ?? null;
  useEffect(() => setStageState(stageOf(projectPath)), [projectPath]);

  const projectName = summary?.path ? splitPath(summary.path)[1] : 'Untitled project';
  /** A file name for what's selected: its name if it's one, else the project's. */
  const selectionName = (objects: ObjectSummary[]) => (objects.length === 1 ? objects[0].name : `${projectName} selection`);

  /** What App does for each tool that opens no panel (the tool list, decision 0076). One with nothing here doesn't compile. */
  const toolActions: Record<ToolAction, () => unknown> = {
    'project.new': onNew,
    'project.open': () => onOpen(false),
    'project.open_folder': () => onOpen(true),
    'project.save': () => onSave(false),
    'project.save_as': () => onSave(true),
    'edit.undo': onUndo,
    'edit.delete': () => void deleteSelection(),
    'edit.redo': onRedo,
    'import.surfaces': onImportSurface,
    'import.vulcan_grid': onImportVulcan,
    'import.strings': onImportStrings,
    'import.solids': () => onImportSolids('files', false),
    'import.solids_folder': () => onImportSolids('folder', false),
    'import.solids_repaired': () => onImportSolids('files', true),
    'import.solids_folder_repaired': () => onImportSolids('folder', true),
    'import.deswik': onImportDeswik,
    'import.spry': onImportSpry,
    'seam.suggest': onSuggestSeamDefinition,
    'seam.build': onBuildSeamModel,
    'seam.relink': () => attempt(() => relinkGeology(true)),
    'export.selected_dxf': () => exportObjects(selectedDxf, 'dxf', selectionName(selectedDxf)),
    'export.selected_strings': () => exportObjects(selectedStrings, 'csv', selectionName(selectedStrings)),
    'export.selected_solids': () => askSolidsOut(selectedSolids, selectionName(selectedSolids)),
    'export.visible_dxf': () => exportObjects(visibleObjects(DXF_KINDS), 'dxf', projectName),
    'export.visible_strings': () => exportObjects(visibleObjects(new Set(['string'])), 'csv', projectName),
    'export.visible_solids': () => askSolidsOut(visibleObjects(new Set(['solid'])), projectName),
    'solids.repair': () => {
      // Every closed mesh kept as a surface because it couldn't be a solid (decision 0040), wherever it came from.
      const flagged = (summary?.objects ?? []).flatMap((o) => (typeof o.attributes.not_a_solid === 'string' ? [{ id: o.id, name: o.name, why: o.attributes.not_a_solid }] : []));
      if (flagged.length > 0) setNotSolids(flagged);
      else setStatus({ text: 'Nothing to repair: no closed mesh is kept as a surface for not being a solid.' });
    },
    'panels.hide': () => dockRef.current && toggleAllPanels(dockRef.current),
    'panels.reset': () => dockRef.current && resetLayout(dockRef.current),
    'view.select': () => setMode('select'),
    'view.draw': () => setMode('draw'),
    'view.snap': () => stringsRef.current?.toggleSnapping(),
    'view.plan': togglePlan,
    'view.zoom_selection': zoomToSelection,
    'view.zoom_all': zoomToExtents,
    'view.section': toggleSection,
    'view.measure': toggleMeasure,
  };
  const toolCounts: Record<ToolNeeds, number> = {
    undo: summary?.can_undo ? 1 : 0,
    redo: summary?.can_redo ? 1 : 0,
    selected: selection.length,
    'selected-dxf': selectedDxf.length,
    'selected-strings': selectedStrings.length,
    'selected-solids': selectedSolids.length,
  };
  /** Carries a tool out: its action, or its panel opened and set to it. */
  const runTool = (tool: Tool) => {
    if (tool.action) return void toolActions[tool.action]();
    if (tool.panel) openTool(tool.panel, tool.mode);
  };
  const toolUse: ToolUse = {
    run: runTool,
    count: (needs) => toolCounts[needs],
    // The tool in use: its panel in front in its dock, set to it.
    lit: (tool) => !!tool.panel && panelsShown.includes(tool.panel) && inUse(tool, panelModes[tool.panel]),
  };
  // Two-letter shortcuts (NS, decision 0080), carried out as a click on their tool would be.
  const runToolRef = useRef(runTool);
  runToolRef.current = runTool;
  useEffect(() => listenForChords((tool) => runToolRef.current(tool), (text) => setStatus({ text })), []);

  return (
    <TooltipProvider delayDuration={400}>
      <div className="flex h-full flex-col">
        <Ribbon projectName={projectName} projectPath={summary?.path ?? null} stage={stage} onStage={setStage} onFind={() => setFinding(true)} use={toolUse} />
        <FindCommand open={finding} onOpenChange={setFinding} use={toolUse} />

        <main className="min-h-0 flex-1">
          <WorkspaceContext.Provider value={workspace}>
            <ViewToolsContext.Provider value={viewTools}>
              <ShowNote.Provider value={showNote}>
                <Workspace onReady={(api) => (dockRef.current = api)} onShown={setPanelsShown} />
              </ShowNote.Provider>
            </ViewToolsContext.Provider>
          </WorkspaceContext.Provider>
        </main>

        <footer className="flex shrink-0 items-center gap-3 border-t bg-card px-3 py-1 text-xs">
          <span className={cn('min-w-0 flex-1 truncate', status.error ? 'text-destructive' : 'text-muted-foreground')} title={status.text}>
            {status.text}
          </span>
          <RunGoing />
          <span className="shrink-0 text-muted-foreground tabular-nums" title={backend}>
            {stats ? `${stats.fps.toFixed(0)} fps · ${stats.triangles.toLocaleString()} triangles` : '–'}
          </span>
        </footer>
      </div>
      <DeswikImportDialog file={deswikFile} onCancel={() => setDeswikFile(null)} onImport={importDeswik} />
      <DeswikImportDialog file={spryFile} source={SPRY} onCancel={() => setSpryFile(null)} onImport={importSpry} />
      <RepairDialog meshes={notSolids} onLeave={() => setNotSolids(null)} onFix={fixNotSolids} />
      <ExportSolidsDialog
        solids={solidsOut?.solids ?? null}
        layerName={(solid) => summary?.layers.find((l) => l.id === solid.layer)?.name ?? ''}
        onCancel={() => setSolidsOut(null)}
        onExport={exportSolids}
      />
    </TooltipProvider>
  );
}
