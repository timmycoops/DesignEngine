import { type ComponentProps, useCallback, useEffect, useMemo, useRef, useState, type ReactNode } from 'react';
import { confirm, message, open, save } from '@tauri-apps/plugin-dialog';
import type { DockviewApi } from 'dockview-react';
import {
  Aperture,
  Box,
  ChevronDown,
  Expand,
  FileInput,
  FileOutput,
  FilePlus,
  Focus,
  FolderOpen,
  Hammer,
  Magnet,
  Map as MapIcon,
  MousePointer2,
  PaintBucket,
  MountainSnow,
  PanelsTopLeft,
  PenLine,
  Redo2,
  Save,
  SaveAll,
  Scissors,
  Slice,
  StepBack,
  StepForward,
  Undo2,
} from 'lucide-react';
import { cn } from 'cn';
import {
  benchConfig,
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
  type RunRecord,
  type ModelLinks,
  runCommand,
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
import { lookAt, ShowNote } from './design-system/kit';
import { Button } from './design-system/ui/button';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuShortcut,
  DropdownMenuSub,
  DropdownMenuSubContent,
  DropdownMenuSubTrigger,
  DropdownMenuTrigger,
} from './design-system/ui/dropdown-menu';
import { Separator } from './design-system/ui/separator';
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from './design-system/ui/tooltip';
import { type DeswikChoice, DeswikImportDialog, type ImportSource, type PickableLayer } from './panels/DeswikImportDialog';
import { ExportSolidsDialog, type SolidExportOptions } from './panels/ExportSolidsDialog';
import { type NotASolid, RepairDialog } from './panels/RepairDialog';
import { IssueMarkers } from './viewport/markers';
import { type RlShown, type SeamShown, SceneSync } from './viewport/sceneSync';
import { planPoint, SectionView } from './viewport/section';
import { RoadLayer } from './viewport/roads';
import { type Mode, StringLayer } from './viewport/strings';
import { typing } from './viewport/camera';
import { FIELD_OF_VIEW_DEFAULT, type FrameStats, Viewport } from './viewport/Viewport';
import { Input } from '@/design-system/ui/input';
import { type Workspace as WorkspaceState, WorkspaceContext } from './workspace/context';
import { ALL_PANELS_KEY, PANELS, resetLayout, showPanel, toggleAllPanels, Workspace } from './workspace/Workspace';

interface Status {
  text: string;
  error?: boolean;
}

/** One line per run: what it made or changed, how long it took, how many of its notes to look at, and its first few. */
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
/** Fields of view offered in the toolbar, in degrees. */
const FIELD_OF_VIEW_CHOICES = [2, 4, 6, 10, 15, 20, 30, 45];

const DXF_KINDS = new Set(['string', 'surface', 'solid']);

/** Splits a file path into its folder and name, for either separator. */
function splitPath(path: string): [string, string] {
  const cut = Math.max(path.lastIndexOf('/'), path.lastIndexOf('\\'));
  return [path.slice(0, cut), path.slice(cut + 1)];
}

function ToolButton({ tip, active, children, ...props }: { tip: string; active?: boolean; children: ReactNode } & ComponentProps<typeof Button>) {
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <Button variant="ghost" size="sm" className={cn(active && 'bg-selection text-primary')} {...props}>
          {children}
        </Button>
      </TooltipTrigger>
      <TooltipContent>{tip}</TooltipContent>
    </Tooltip>
  );
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

  const showSeamHorizon = useCallback((id: string, code: string) => {
    setStatus({ text: `Reading the grids for ${code}…` });
    void syncRef.current?.showHorizon(id, code).then(() => setStatus({ text: `Showing ${code}, coloured by thickness` }));
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
        pick: (x, y, add) =>
          void objectAt(created, sync, section, x, y).then((id) => {
            if (!add) select(id);
            else if (id) toggleSelected(id);
          }),
        choose: (id, add) => (add && id ? toggleSelected(id) : select(id)),
        box: (min, max, add) => void boxSelect(min, max, add),
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
          : next === 'draw'
            ? 'Draw: click on a surface to add vertices; Enter or double-click finishes, Backspace removes the last, Esc abandons'
            : 'Select: click a string, drag its handles; Alt+click inserts a vertex, Delete removes the one under the cursor',
    });
  };

  /** Plan or 3D over the same spot (T, or the toolbar). A section open is closed first. */
  const togglePlan = () => {
    const viewport = viewportRef.current;
    if (!viewport) return;
    if (sectionOpen) closeSection();
    viewport.setPlanView(!viewport.planView);
  };

  /** Section (2, or the toolbar): picking its two points starts, or stops if it had started; an open one closes. */
  const toggleSection = () => {
    if (sectionOpen) return closeSection();
    setMode(mode === 'section' ? 'select' : 'section');
  };

  const stepSection = (direction: number) => {
    const distance = Number(sectionStep);
    if (Number.isFinite(distance) && distance > 0) void sectionRef.current?.step(direction * distance);
  };

  // Shortcuts: undo, redo, save, zoom, and stepping a section.
  useEffect(() => {
    const key = (e: KeyboardEvent) => {
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
      setObjectsVisible: setObjectsShown,
      setLooks: changeLooks,
      setLayerOpacity: changeLayerOpacity,
      setContours: changeContours,
      rlShown,
      openPanel: (id) => dockRef.current && showPanel(dockRef.current, id),
      run,
      runRecord,
      seamViews,
      sectionOpen,
      lineWeight: (layer) => sectionRef.current?.weightOf(layer) ?? null,
      setLineWeight: (layer, px) => sectionRef.current?.setWeight(layer, px),
      showSeamHorizon,
      focusIssue,
      clearRefusal,
      markCrossings,
      flyTo,
      frameLine,
      preferLines,
      pickPoint,
      pickLine,
      pickString,
      drawing: mode === 'draw',
      typeVertex,
    }),
    [summary, log, selected, selection, activeLayer, viewportHost, viewportMounted, select, setSelection, toggleSelected, zoomTo, setActiveLayer, setVisible, setObjectsShown, changeLooks, changeLayerOpacity, changeContours, rlShown, run, runRecord, clearRefusal, seamViews, sectionOpen, showSeamHorizon, focusIssue, markCrossings, flyTo, frameLine, preferLines, pickPoint, pickLine, pickString, mode, typeVertex],
  );

  const projectName = summary?.path ? splitPath(summary.path)[1] : 'Untitled project';
  /** A file name for what's selected: its name if it's one, else the project's. */
  const selectionName = (objects: ObjectSummary[]) => (objects.length === 1 ? objects[0].name : `${projectName} selection`);

  return (
    <TooltipProvider delayDuration={400}>
      <div className="flex h-full flex-col">
        {/* In groups, left to right: the project, getting data in and out, the string tools, the view, the design panels. */}
        <header className="flex shrink-0 items-center gap-0.5 border-b bg-card px-2 py-1">
          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <Button variant="ghost" size="sm" className="max-w-60 font-semibold" title={summary?.path ?? 'Not saved yet'}>
                <span className="truncate">{projectName}</span>
                <ChevronDown className="text-muted-foreground" />
              </Button>
            </DropdownMenuTrigger>
            <DropdownMenuContent align="start">
              <DropdownMenuItem onSelect={onNew}>
                <FilePlus /> New project
              </DropdownMenuItem>
              <DropdownMenuItem onSelect={() => onOpen(false)}>
                <FolderOpen /> Open a project (.zdp)…
              </DropdownMenuItem>
              <DropdownMenuItem onSelect={() => onOpen(true)}>
                <FolderOpen /> Open an older project folder (.zdproj)…
              </DropdownMenuItem>
              <DropdownMenuSeparator />
              <DropdownMenuItem onSelect={() => onSave(false)}>
                <Save /> Save <DropdownMenuShortcut>Ctrl+S</DropdownMenuShortcut>
              </DropdownMenuItem>
              <DropdownMenuItem onSelect={() => onSave(true)}>
                <SaveAll /> Save as…
              </DropdownMenuItem>
            </DropdownMenuContent>
          </DropdownMenu>
          <ToolButton tip="Save (Ctrl+S)" onClick={() => onSave(false)}>
            <Save />
          </ToolButton>
          <ToolButton tip="Undo (Ctrl+Z)" disabled={!summary?.can_undo} onClick={onUndo}>
            <Undo2 />
          </ToolButton>
          <ToolButton tip="Redo (Ctrl+Y)" disabled={!summary?.can_redo} onClick={onRedo}>
            <Redo2 />
          </ToolButton>
          <Separator orientation="vertical" className="mx-1 h-5" />
          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <Button variant="ghost" size="sm" title="Bring in surfaces, grids, strings, solids, Deswik and Spry, and build a seam model">
                <FileInput /> Import <ChevronDown className="text-muted-foreground" />
              </Button>
            </DropdownMenuTrigger>
            <DropdownMenuContent align="start">
              <DropdownMenuItem title="Vulcan .00t, DXF 3DFACE or polyface, or an XYZ or CSV grid" onSelect={onImportSurface}>
                Surfaces…
              </DropdownMenuItem>
              <DropdownMenuItem title="Pick a roof, floor or topography, and the grids of that name come with it" onSelect={onImportVulcan}>
                Vulcan grid…
              </DropdownMenuItem>
              <DropdownMenuItem title="DXF polylines and lines, or CSV with x, y and z columns" onSelect={onImportStrings}>
                Strings…
              </DropdownMenuItem>
              <DropdownMenuSub>
                <DropdownMenuSubTrigger title="Closed triangulations in Vulcan .00t or DXF files, or a zip or folder of them">Solids</DropdownMenuSubTrigger>
                <DropdownMenuSubContent>
                  <DropdownMenuItem onSelect={() => onImportSolids('files', false)}>A zip or files…</DropdownMenuItem>
                  <DropdownMenuItem onSelect={() => onImportSolids('folder', false)}>A folder…</DropdownMenuItem>
                  <DropdownMenuSeparator />
                  <DropdownMenuLabel className="text-xs font-normal text-muted-foreground">Repaired as they come in</DropdownMenuLabel>
                  <DropdownMenuItem title="Sheets and fins off, cracks joined, holes up to 10 % filled" onSelect={() => onImportSolids('files', true)}>
                    A zip or files…
                  </DropdownMenuItem>
                  <DropdownMenuItem title="Sheets and fins off, cracks joined, holes up to 10 % filled" onSelect={() => onImportSolids('folder', true)}>
                    A folder…
                  </DropdownMenuItem>
                </DropdownMenuSubContent>
              </DropdownMenuSub>
              <DropdownMenuSeparator />
              <DropdownMenuItem title="Layers of a Deswik drawing: strings with their projection details, surfaces and solids" onSelect={onImportDeswik}>
                Deswik drawing (.dcf)…
              </DropdownMenuItem>
              <DropdownMenuItem title="Block solids named by their node paths with Spry's values, and design-layer strings and solids" onSelect={onImportSpry}>
                Spry project (.spry)…
              </DropdownMenuItem>
              <DropdownMenuSeparator />
              <DropdownMenuLabel className="text-xs font-normal text-muted-foreground">Seam model</DropdownMenuLabel>
              <DropdownMenuItem onSelect={onSuggestSeamDefinition}>Suggest a stack definition (CSV)…</DropdownMenuItem>
              <DropdownMenuItem onSelect={onBuildSeamModel}>Build and check from a definition…</DropdownMenuItem>
              <DropdownMenuItem onSelect={() => attempt(() => relinkGeology(true))}>Relink geology…</DropdownMenuItem>
            </DropdownMenuContent>
          </DropdownMenu>
          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <Button variant="ghost" size="sm">
                <FileOutput /> Export <ChevronDown className="text-muted-foreground" />
              </Button>
            </DropdownMenuTrigger>
            <DropdownMenuContent align="start">
              <DropdownMenuItem disabled={selectedDxf.length === 0} onSelect={() => exportObjects(selectedDxf, 'dxf', selectionName(selectedDxf))}>
                Selected to DXF{selectedDxf.length > 1 ? ` (${selectedDxf.length})` : ''}…
              </DropdownMenuItem>
              <DropdownMenuItem disabled={selectedStrings.length === 0} onSelect={() => exportObjects(selectedStrings, 'csv', selectionName(selectedStrings))}>
                Selected strings to CSV{selectedStrings.length > 1 ? ` (${selectedStrings.length})` : ''}…
              </DropdownMenuItem>
              <DropdownMenuItem disabled={selectedSolids.length === 0} onSelect={() => askSolidsOut(selectedSolids, selectionName(selectedSolids))}>
                Selected solids to a zip{selectedSolids.length > 1 ? ` (${selectedSolids.length})` : ''}…
              </DropdownMenuItem>
              <DropdownMenuItem onSelect={() => exportObjects(visibleObjects(DXF_KINDS), 'dxf', projectName)}>
                Everything visible to DXF…
              </DropdownMenuItem>
              <DropdownMenuItem onSelect={() => exportObjects(visibleObjects(new Set(['string'])), 'csv', projectName)}>
                Visible strings to CSV…
              </DropdownMenuItem>
              <DropdownMenuItem onSelect={() => askSolidsOut(visibleObjects(new Set(['solid'])), projectName)}>
                Visible solids to a zip…
              </DropdownMenuItem>
            </DropdownMenuContent>
          </DropdownMenu>
          <Separator orientation="vertical" className="mx-1 h-5" />
          <ToolButton tip="Select and edit strings" active={mode === 'select'} onClick={() => setMode('select')}>
            <MousePointer2 /> Select
          </ToolButton>
          <ToolButton tip={sectionOpen ? 'Digitise a string on the section: snaps to crossings, vertices and lines of the cut' : 'Digitise a string: snaps to string vertices and segments, then surfaces'} active={mode === 'draw'} onClick={() => setMode('draw')}>
            <PenLine /> Draw string
          </ToolButton>
          <ToolButton
            tip={snapping ? `Snap is on: to ${sectionOpen ? 'the cut lines' : 'strings, then surfaces'} (S turns it off)` : 'Snap is off: points go on the working plane (S turns it on)'}
            active={snapping}
            onClick={() => stringsRef.current?.toggleSnapping()}
          >
            <Magnet /> Snap
          </ToolButton>
          <Separator orientation="vertical" className="mx-1 h-5" />
          <ToolButton
            tip={plan ? 'Back to the 3D view (T). Or hold Z and drag to tilt up out of plan' : 'Plan view (T): straight down, north up; left-drag pans; the wheel zooms where the cursor is'}
            active={plan}
            onClick={togglePlan}
          >
            {plan ? <MapIcon /> : <Box />} {plan ? 'Plan' : '3D'}
          </ToolButton>
          <ToolButton tip="Zoom to what's selected (F), from the way you're looking" disabled={selection.length === 0 || sectionOpen} onClick={() => zoomTo(selection)}>
            <Focus />
          </ToolButton>
          <ToolButton tip="Zoom to everything (E, or Shift+F)" disabled={sectionOpen} onClick={zoomToExtents}>
            <Expand />
          </ToolButton>
          <DropdownMenu>
            <Tooltip>
              <TooltipTrigger asChild>
                <DropdownMenuTrigger asChild>
                  <Button variant="ghost" size="sm" className="tabular-nums">
                    <Aperture /> {fieldOfView}°
                  </Button>
                </DropdownMenuTrigger>
              </TooltipTrigger>
              <TooltipContent>3D field of view: narrower draws the far side of the pit nearer its true size. Saved on this machine</TooltipContent>
            </Tooltip>
            <DropdownMenuContent align="start">
              {FIELD_OF_VIEW_CHOICES.map((degrees) => (
                <DropdownMenuItem
                  key={degrees}
                  onSelect={() => {
                    viewportRef.current?.setFieldOfView(degrees);
                    setFieldOfView(viewportRef.current?.fieldOfView ?? degrees);
                  }}
                >
                  <span className="w-8 tabular-nums">{degrees}°</span>
                  {degrees === FIELD_OF_VIEW_DEFAULT ? 'default' : degrees === 45 ? 'old view' : ''}
                  {degrees === fieldOfView ? ' ✓' : ''}
                </DropdownMenuItem>
              ))}
            </DropdownMenuContent>
          </DropdownMenu>
          <ToolButton
            tip={sectionOpen ? 'Close the section (2)' : 'Section (2): pick two points for a true vertical slice'}
            active={sectionOpen || mode === 'section'}
            onClick={toggleSection}
          >
            <Scissors /> Section
          </ToolButton>
          {sectionOpen && (
            <>
              <ToolButton tip="Step back ([)" onClick={() => stepSection(-1)}>
                <StepBack />
              </ToolButton>
              <Input
                className="h-7 w-14 px-1.5 text-xs tabular-nums"
                value={sectionStep}
                onChange={(e) => {
                  setSectionStep(e.target.value);
                  const distance = Number(e.target.value);
                  if (sectionRef.current && Number.isFinite(distance) && distance > 0) sectionRef.current.stepSize = distance;
                }}
                title="Metres to step, square to the section"
                aria-label="Section step in metres"
              />
              <ToolButton tip="Step on (])" onClick={() => stepSection(1)}>
                <StepForward />
              </ToolButton>
              <ToolButton
                tip="Fill solids between their cut lines"
                active={sectionFill}
                onClick={() => {
                  setSectionFill(!sectionFill);
                  if (sectionRef.current) sectionRef.current.filled = !sectionFill;
                }}
              >
                <PaintBucket /> Fill
              </ToolButton>
            </>
          )}
          <Separator orientation="vertical" className="mx-1 h-5" />
          <ToolButton tip="Face design: lines, profiles and rules, run as one" onClick={() => dockRef.current && showPanel(dockRef.current, 'face')}>
            <MountainSnow /> Face
          </ToolButton>
          <ToolButton tip="Builder: pick a line, build one wall or berm from it, edit between steps" onClick={() => dockRef.current && showPanel(dockRef.current, 'builder')}>
            <Hammer /> Builder
          </ToolButton>
          <ToolButton tip="Cut solids: into strips by faces, by a surface, grid or RL, or one between two surfaces" onClick={() => dockRef.current && showPanel(dockRef.current, 'cut')}>
            <Slice /> Cut
          </ToolButton>
          <div className="ml-auto">
            <DropdownMenu>
              <DropdownMenuTrigger asChild>
                <Button variant="ghost" size="sm">
                  <PanelsTopLeft /> Panels <ChevronDown className="text-muted-foreground" />
                </Button>
              </DropdownMenuTrigger>
              <DropdownMenuContent align="end">
                {PANELS.map((p) => (
                  <DropdownMenuItem key={p.id} onSelect={() => dockRef.current && showPanel(dockRef.current, p.id)}>
                    {p.title}
                  </DropdownMenuItem>
                ))}
                <DropdownMenuSeparator />
                <DropdownMenuItem onSelect={() => dockRef.current && toggleAllPanels(dockRef.current)}>
                  Hide every panel, or bring them back <DropdownMenuShortcut>{ALL_PANELS_KEY}</DropdownMenuShortcut>
                </DropdownMenuItem>
                <DropdownMenuItem onSelect={() => dockRef.current && resetLayout(dockRef.current)}>Reset layout</DropdownMenuItem>
              </DropdownMenuContent>
            </DropdownMenu>
          </div>
        </header>

        <main className="min-h-0 flex-1">
          <WorkspaceContext.Provider value={workspace}>
            <ShowNote.Provider value={showNote}>
              <Workspace onReady={(api) => (dockRef.current = api)} />
            </ShowNote.Provider>
          </WorkspaceContext.Provider>
        </main>

        <footer className="flex shrink-0 items-center gap-3 border-t bg-card px-3 py-1 text-xs">
          <span className={cn('min-w-0 flex-1 truncate', status.error ? 'text-destructive' : 'text-muted-foreground')} title={status.text}>
            {status.text}
          </span>
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
