import { type ComponentProps, useCallback, useEffect, useMemo, useRef, useState, type ReactNode } from 'react';
import { confirm, open, save } from '@tauri-apps/plugin-dialog';
import type { DockviewApi } from 'dockview-react';
import {
  Box,
  FileInput,
  FileOutput,
  FilePlus,
  FolderOpen,
  Layers,
  Layers3,
  Magnet,
  Map as MapIcon,
  MousePointer2,
  PanelsTopLeft,
  PenLine,
  Redo2,
  Save,
  SaveAll,
  Undo2,
} from 'lucide-react';
import { cn } from 'cn';
import {
  benchConfig,
  buildSnapIndex,
  type LogEntry,
  newProject,
  type ObjectSummary,
  openProject,
  type ProjectSummary,
  projectSummary,
  redo,
  type RunRecord,
  runCommand,
  runLog,
  saveProject,
  setLayerVisible,
  snapRay,
  undo,
  type Vec3,
} from './bridge';
import { runChecks } from './bench/runChecks';
import { Button } from './design-system/ui/button';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from './design-system/ui/dropdown-menu';
import { Separator } from './design-system/ui/separator';
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from './design-system/ui/tooltip';
import { IssueMarkers } from './viewport/markers';
import { type SeamShown, SceneSync } from './viewport/sceneSync';
import { type Mode, StringLayer } from './viewport/strings';
import { type FrameStats, Viewport } from './viewport/Viewport';
import { type Workspace as WorkspaceState, WorkspaceContext } from './workspace/context';
import { PANELS, resetLayout, showPanel, Workspace } from './workspace/Workspace';

interface Status {
  text: string;
  error?: boolean;
}

/** One line per run: what it made or changed, how long it took, and its first few messages. */
function describe(record: RunRecord): string {
  const outputs = record.outputs.map((o) => `${o.name} (${o.change})`);
  const head = `${record.command}: ${outputs.slice(0, 4).join(', ') || 'done'}${outputs.length > 4 ? ` and ${outputs.length - 4} more` : ''} in ${record.duration_ms.toFixed(0)} ms`;
  const messages = record.messages.slice(0, 2);
  const more = record.messages.length > 2 ? [`(${record.messages.length - 2} more messages in the run log)`] : [];
  return [head, ...messages, ...more].join(' · ');
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

export default function App() {
  const viewportRef = useRef<Viewport | null>(null);
  const syncRef = useRef<SceneSync | null>(null);
  const stringsRef = useRef<StringLayer | null>(null);
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
  const [snapping, setSnapping] = useState(true);
  const [activeLayer, setActiveLayerState] = useState('design');
  const [selected, setSelected] = useState<string | null>(null);
  const [seamView, setSeamView] = useState<SeamShown | null>(null);
  const summaryRef = useRef(summary);
  summaryRef.current = summary;
  const seamRef = useRef<SeamShown | null>(null);
  const markersRef = useRef<IssueMarkers | null>(null);
  const crossingMarkersRef = useRef<IssueMarkers | null>(null);
  /** An issue to fly to once its horizon has loaded. */
  const pendingFocus = useRef<{ id: string; issue: number } | null>(null);

  /** The seam horizon on show changed (or, with null, the model it belonged to went). */
  const onSeam = useCallback((id: string, shown: SeamShown | null) => {
    if (!shown && seamRef.current?.id !== id) return;
    seamRef.current = shown;
    setSeamView(shown);
    markersRef.current?.set(shown?.result.markers.map((m) => m.position) ?? []);
    const pending = pendingFocus.current;
    if (shown && pending?.id === id) {
      pendingFocus.current = null;
      const marker = shown.result.markers.find((m) => m.issue === pending.issue);
      if (marker) viewportRef.current?.focusOn(marker.position);
    }
  }, []);

  const showSeamHorizon = useCallback((id: string, code: string) => {
    setStatus({ text: `Reading the grids for ${code}…` });
    void syncRef.current?.showHorizon(id, code).then(() => setStatus({ text: `Showing ${code}, coloured by thickness` }));
  }, []);

  /** Shows the horizon an issue is on and flies to its worst node. */
  const typeVertex = useCallback(async (text: string) => (stringsRef.current ? stringsRef.current.addTyped(text) : 'The viewport is not ready'), []);

  const focusIssue = useCallback(
    (id: string, horizon: string, issue: number) => {
      const view = seamRef.current;
      if (view?.id === id && view.result.horizon === horizon) {
        const marker = view.result.markers.find((m) => m.issue === issue);
        if (marker) viewportRef.current?.focusOn(marker.position);
        return;
      }
      pendingFocus.current = { id, issue };
      showSeamHorizon(id, horizon);
    },
    [showSeamHorizon],
  );

  /** World coordinates to the scene's local frame, which the viewport draws in. */
  const toLocal = useCallback((p: Vec3): Vec3 => {
    const o = summaryRef.current?.origin ?? [0, 0, 0];
    return [p[0] - o[0], p[1] - o[1], p[2] - o[2]];
  }, []);
  const markCrossings = useCallback((sites: Vec3[]) => crossingMarkersRef.current?.set(sites.map(toLocal)), [toLocal]);
  const flyTo = useCallback((point: Vec3) => viewportRef.current?.focusOn(toLocal(point)), [toLocal]);

  /** Shows a new project summary: panels, strings, and any surfaces whose geometry changed. */
  const show = useCallback((next: ProjectSummary) => {
    setSummary(next);
    void runLog().then(setLog);
    stringsRef.current?.sync(next);
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

  const run = useCallback(
    async (name: string, params: Record<string, unknown>) => {
      try {
        const reply = await runCommand(name, params);
        show(reply.summary);
        setStatus({ text: describe(reply.record) });
        return true;
      } catch (e) {
        setStatus({ text: String(e), error: true });
        // A failed run is in the log too.
        void runLog().then(setLog);
        return false;
      }
    },
    [show],
  );

  const viewportMounted = useCallback(() => setHostMounted(true), []);

  // The viewport is made once its canvas host is in the page (the viewport panel mounts it).
  useEffect(() => {
    if (!hostMounted) return;
    let viewport: Viewport | null = null;
    let detach = () => {};
    let cancelled = false;
    void Viewport.create(viewportHost).then(async (created) => {
      if (cancelled) return created.dispose();
      viewport = created;
      viewportRef.current = created;
      created.onStats = setStats;
      setBackend(`${created.backend} · ${created.adapter}`);
      markersRef.current = new IssueMarkers(created);
      crossingMarkersRef.current = new IssueMarkers(created);
      const sync = new SceneSync(created);
      sync.onSeam = onSeam;
      sync.onError = (text) => setStatus({ text, error: true });
      syncRef.current = sync;
      const strings = new StringLayer(created, {
        run,
        snap: (x, y) => {
          const [nx, ny] = created.clientToNdc(x, y);
          const ray = created.rayFromNdc(nx, ny);
          return snapRay(ray.origin, ray.direction);
        },
        snapDown: (x, y) => snapRay([x, y, 1e6], [0, 0, -1]),
        status: (text) => setStatus({ text }),
        layer: () => layerRef.current,
      });
      strings.onSelect = setSelected;
      strings.onSnapping = setSnapping;
      created.pickPoint = (x, y) => strings.pick(x, y);
      stringsRef.current = strings;
      detach = strings.attach();
      show(await projectSummary());
      const config = await benchConfig();
      if (config) {
        // Phase 0 timings were taken with the viewport filling the window; solids go through the panels.
        if (!config.solids) dockRef.current?.getPanel('viewport')?.api.maximize();
        await created.nextFrame();
        await created.nextFrame();
        await runChecks(created, config, (text) => setStatus({ text }), show);
      }
    });
    return () => {
      cancelled = true;
      detach();
      viewport?.dispose();
    };
  }, [hostMounted, viewportHost, run, show, onSeam]);

  const attempt = useCallback(async (work: () => Promise<void>) => {
    try {
      await work();
    } catch (e) {
      setStatus({ text: String(e), error: true });
    }
  }, []);

  const select = useCallback((id: string | null) => {
    const strings = stringsRef.current;
    const isString = summaryRef.current?.objects.some((o) => o.id === id && o.kind === 'string');
    // The string layer reports its own selection (null for anything that isn't a string).
    strings?.select(isString ? id : null);
    if (!isString) setSelected(id);
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
        const chosen = await save({ title: 'Save project as (a .zdproj folder)', defaultPath: 'project.zdproj' });
        if (!chosen) return;
        path = chosen.endsWith('.zdproj') ? chosen : `${chosen}.zdproj`;
      }
      const reply = await saveProject(path);
      setStatus({
        text: `Saved ${reply.path}: ${reply.object_files_written} files written, ${reply.object_files_reused} reused, ${(reply.bytes_written / 1e6).toFixed(1)} MB in ${reply.ms.toFixed(0)} ms`,
      });
      show(await projectSummary());
    });

  const onOpen = () =>
    attempt(async () => {
      const path = await open({ directory: true, title: 'Open project (a .zdproj folder)' });
      if (!path) return;
      setStatus({ text: `Opening ${path}…` });
      select(null);
      show(await openProject(path));
      setStatus({ text: `Opened ${path}` });
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

  const selectedObject = summary?.objects.find((o) => o.id === selected) ?? null;
  const visibleObjects = (kinds: Set<string>) => {
    // Objects carry their layer's id.
    const shown = new Set(summary?.layers.filter((l) => l.visible).map((l) => l.id));
    return summary?.objects.filter((o) => kinds.has(o.kind) && shown.has(o.layer)) ?? [];
  };

  const onImportVulcan = () =>
    attempt(async () => {
      const picked = await open({ title: "Pick the seam's roof or floor grid", filters: [{ name: 'Vulcan grids', extensions: ['srg', 'sfg'] }] });
      if (!picked || Array.isArray(picked)) return;
      const [dir, file] = splitPath(picked);
      const stem = file.slice(0, file.lastIndexOf('.'));
      setStatus({ text: `Importing seam ${stem}…` });
      await run('grid.import_vulcan_seam', { dir, prefix: '', seam: stem, layer: stem.toUpperCase() });
    });

  /** Writes a starting stack definition for a folder of Vulcan grids, to check and edit in Excel. */
  const onSuggestSeamDefinition = () =>
    attempt(async () => {
      const picked = await open({ title: 'Pick any roof grid in the folder of seam grids', filters: [{ name: 'Vulcan roof grids', extensions: ['srg'] }] });
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
        const reply = await runCommand('seam.build_model', { definition: picked, layer: 'seam models' });
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
        next === 'draw'
          ? 'Draw: click on a surface to add vertices; Enter or double-click finishes, Backspace removes the last, Esc abandons'
          : 'Select: click a string, drag its handles; Alt+click inserts a vertex, Delete removes the one under the cursor',
    });
  };

  // Shortcuts: undo, redo, save.
  useEffect(() => {
    const key = (e: KeyboardEvent) => {
      if (!(e.ctrlKey || e.metaKey) || e.target instanceof HTMLInputElement) return;
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

  const workspace = useMemo<WorkspaceState>(
    () => ({
      summary,
      log,
      selected,
      activeLayer,
      viewportHost,
      viewportMounted,
      select,
      setActiveLayer,
      setLayerVisible: setVisible,
      run,
      seamView,
      showSeamHorizon,
      focusIssue,
      markCrossings,
      flyTo,
      drawing: mode === 'draw',
      typeVertex,
    }),
    [summary, log, selected, activeLayer, viewportHost, viewportMounted, select, setActiveLayer, setVisible, run, seamView, showSeamHorizon, focusIssue, markCrossings, flyTo, mode, typeVertex],
  );

  const projectName = summary?.path ? splitPath(summary.path)[1] : 'Untitled project';

  return (
    <TooltipProvider delayDuration={400}>
      <div className="flex h-full flex-col">
        <header className="flex shrink-0 items-center gap-0.5 border-b bg-card px-2 py-1">
          <span className="mr-2 max-w-56 truncate text-sm font-semibold" title={summary?.path ?? undefined}>
            {projectName}
          </span>
          <ToolButton tip="New project" onClick={onNew}>
            <FilePlus />
          </ToolButton>
          <ToolButton tip="Open a .zdproj project" onClick={onOpen}>
            <FolderOpen />
          </ToolButton>
          <ToolButton tip="Save (Ctrl+S)" onClick={() => onSave(false)}>
            <Save />
          </ToolButton>
          <ToolButton tip="Save as…" onClick={() => onSave(true)}>
            <SaveAll />
          </ToolButton>
          <Separator orientation="vertical" className="mx-1 h-5" />
          <ToolButton tip="Import surfaces (Vulcan .00t, DXF, XYZ or CSV grids)" onClick={onImportSurface}>
            <FileInput /> Surface
          </ToolButton>
          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <Button variant="ghost" size="sm" title="Import solids: closed triangulations in Vulcan .00t or DXF files, or a zip or folder of them">
                <FileInput /> Solids
              </Button>
            </DropdownMenuTrigger>
            <DropdownMenuContent align="start">
              <DropdownMenuItem onSelect={() => onImportSolids('files', false)}>Import a zip or files…</DropdownMenuItem>
              <DropdownMenuItem onSelect={() => onImportSolids('folder', false)}>Import a folder…</DropdownMenuItem>
              <DropdownMenuItem title="Sheets and fins off, cracks joined, holes up to 10 % filled" onSelect={() => onImportSolids('files', true)}>
                Import and repair a zip or files…
              </DropdownMenuItem>
              <DropdownMenuItem title="Sheets and fins off, cracks joined, holes up to 10 % filled" onSelect={() => onImportSolids('folder', true)}>
                Import and repair a folder…
              </DropdownMenuItem>
            </DropdownMenuContent>
          </DropdownMenu>
          <ToolButton tip="Import strings (DXF polylines and lines, or CSV with x, y and z columns)" onClick={onImportStrings}>
            <FileInput /> Strings
          </ToolButton>
          <ToolButton tip="Import a seam's Vulcan grids: pick its roof or floor grid" onClick={onImportVulcan}>
            <Layers3 /> Vulcan seam
          </ToolButton>
          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <Button variant="ghost" size="sm">
                <Layers /> Seam model
              </Button>
            </DropdownMenuTrigger>
            <DropdownMenuContent align="start">
              <DropdownMenuItem onSelect={onSuggestSeamDefinition}>Suggest a stack definition (CSV)…</DropdownMenuItem>
              <DropdownMenuItem onSelect={onBuildSeamModel}>Build and check from a definition…</DropdownMenuItem>
            </DropdownMenuContent>
          </DropdownMenu>
          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <Button variant="ghost" size="sm">
                <FileOutput /> Export
              </Button>
            </DropdownMenuTrigger>
            <DropdownMenuContent align="start">
              <DropdownMenuItem
                disabled={!selectedObject || !DXF_KINDS.has(selectedObject.kind)}
                onSelect={() => selectedObject && exportObjects([selectedObject], 'dxf', selectedObject.name)}
              >
                Selected object to DXF…
              </DropdownMenuItem>
              <DropdownMenuItem
                disabled={selectedObject?.kind !== 'string'}
                onSelect={() => selectedObject && exportObjects([selectedObject], 'csv', selectedObject.name)}
              >
                Selected string to CSV…
              </DropdownMenuItem>
              <DropdownMenuItem onSelect={() => exportObjects(visibleObjects(DXF_KINDS), 'dxf', projectName)}>
                Everything visible to DXF…
              </DropdownMenuItem>
              <DropdownMenuItem onSelect={() => exportObjects(visibleObjects(new Set(['string'])), 'csv', projectName)}>
                Visible strings to CSV…
              </DropdownMenuItem>
            </DropdownMenuContent>
          </DropdownMenu>
          <Separator orientation="vertical" className="mx-1 h-5" />
          <ToolButton tip="Select and edit strings" active={mode === 'select'} onClick={() => setMode('select')}>
            <MousePointer2 /> Select
          </ToolButton>
          <ToolButton tip="Digitise a string: snaps to string vertices and segments, then surfaces" active={mode === 'draw'} onClick={() => setMode('draw')}>
            <PenLine /> Draw string
          </ToolButton>
          <ToolButton
            tip={snapping ? 'Snap is on: to strings, then surfaces (S turns it off)' : 'Snap is off: points go on the working plane (S turns it on)'}
            active={snapping}
            onClick={() => stringsRef.current?.toggleSnapping()}
          >
            <Magnet /> Snap
          </ToolButton>
          <ToolButton
            tip={plan ? 'Back to the 3D view' : 'Plan view: straight down, north up; left-drag pans; the wheel zooms where the cursor is'}
            active={plan}
            onClick={() => {
              viewportRef.current?.setPlanView(!plan);
              setPlan(!plan);
            }}
          >
            {plan ? <MapIcon /> : <Box />} {plan ? 'Plan' : '3D'}
          </ToolButton>
          <Separator orientation="vertical" className="mx-1 h-5" />
          <ToolButton tip="Undo (Ctrl+Z)" disabled={!summary?.can_undo} onClick={onUndo}>
            <Undo2 />
          </ToolButton>
          <ToolButton tip="Redo (Ctrl+Y)" disabled={!summary?.can_redo} onClick={onRedo}>
            <Redo2 />
          </ToolButton>
          <div className="ml-auto">
            <DropdownMenu>
              <DropdownMenuTrigger asChild>
                <Button variant="ghost" size="sm">
                  <PanelsTopLeft /> Panels
                </Button>
              </DropdownMenuTrigger>
              <DropdownMenuContent align="end">
                {PANELS.map((p) => (
                  <DropdownMenuItem key={p.id} onSelect={() => dockRef.current && showPanel(dockRef.current, p.id)}>
                    {p.title}
                  </DropdownMenuItem>
                ))}
                <DropdownMenuSeparator />
                <DropdownMenuItem onSelect={() => dockRef.current && resetLayout(dockRef.current)}>Reset layout</DropdownMenuItem>
              </DropdownMenuContent>
            </DropdownMenu>
          </div>
        </header>

        <main className="min-h-0 flex-1">
          <WorkspaceContext.Provider value={workspace}>
            <Workspace onReady={(api) => (dockRef.current = api)} />
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
    </TooltipProvider>
  );
}
