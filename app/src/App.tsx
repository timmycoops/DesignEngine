import { useCallback, useEffect, useRef, useState } from 'react';
import { confirm, open, save } from '@tauri-apps/plugin-dialog';
import {
  benchConfig,
  buildSnapIndex,
  newProject,
  openProject,
  type ProjectSummary,
  projectSummary,
  redo,
  type RunRecord,
  runCommand,
  saveProject,
  setLayerVisible,
  snapRay,
  undo,
} from './bridge';
import { runChecks } from './bench/runChecks';
import { SceneSync } from './viewport/sceneSync';
import { type Mode, StringLayer } from './viewport/strings';
import { type FrameStats, Viewport } from './viewport/Viewport';

interface Status {
  text: string;
  error?: boolean;
}

/** One line per run: what it made or changed, how long it took, and its first few messages. */
function describe(record: RunRecord): string {
  const outputs = record.outputs.map((o) => `${o.name} (${o.change})`);
  const head = `${record.command}: ${outputs.slice(0, 4).join(', ') || 'done'}${outputs.length > 4 ? ` and ${outputs.length - 4} more` : ''} in ${record.duration_ms.toFixed(0)} ms`;
  const messages = record.messages.slice(0, 3);
  const more = record.messages.length > 3 ? [`(${record.messages.length - 3} more messages in the run log)`] : [];
  return [head, ...messages, ...more].join('\n');
}

/** Splits a file path into its folder and name, for either separator. */
function splitPath(path: string): [string, string] {
  const cut = Math.max(path.lastIndexOf('/'), path.lastIndexOf('\\'));
  return [path.slice(0, cut), path.slice(cut + 1)];
}

export default function App() {
  const hostRef = useRef<HTMLDivElement>(null);
  const viewportRef = useRef<Viewport | null>(null);
  const syncRef = useRef<SceneSync | null>(null);
  const stringsRef = useRef<StringLayer | null>(null);
  const layerRef = useRef('design');
  const [summary, setSummary] = useState<ProjectSummary | null>(null);
  const [stats, setStats] = useState<FrameStats | null>(null);
  const [backend, setBackend] = useState('starting…');
  const [status, setStatus] = useState<Status>({ text: 'Open a project or import a surface to start.' });
  const [mode, setModeState] = useState<Mode>('select');
  const [activeLayer, setActiveLayerState] = useState('design');
  const [selected, setSelected] = useState<string | null>(null);

  /** Shows a new project summary: side panel, strings, and any surfaces whose geometry changed. */
  const show = useCallback((next: ProjectSummary) => {
    setSummary(next);
    stringsRef.current?.sync(next);
    const sync = syncRef.current;
    const viewport = viewportRef.current;
    if (!sync || !viewport) return;
    const first = sync.count === 0;
    void sync.apply(next).then((loaded) => {
      if (loaded === 0) return;
      if (first) viewport.fitView();
      void buildSnapIndex();
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
        return false;
      }
    },
    [show],
  );

  useEffect(() => {
    let viewport: Viewport | null = null;
    let detach = () => {};
    let cancelled = false;
    void Viewport.create(hostRef.current!).then(async (created) => {
      if (cancelled) return created.dispose();
      viewport = created;
      viewportRef.current = created;
      created.onStats = setStats;
      setBackend(`${created.backend} · ${created.adapter}`);
      syncRef.current = new SceneSync(created);
      const strings = new StringLayer(created, {
        run,
        snap: (x, y) => {
          const [nx, ny] = created.clientToNdc(x, y);
          const ray = created.rayFromNdc(nx, ny);
          return snapRay(ray.origin, ray.direction);
        },
        status: (text) => setStatus({ text }),
        layer: () => layerRef.current,
      });
      strings.onSelect = setSelected;
      stringsRef.current = strings;
      detach = strings.attach();
      show(await projectSummary());
      const config = await benchConfig();
      if (config) await runChecks(created, config, (text) => setStatus({ text }));
    });
    return () => {
      cancelled = true;
      detach();
      viewport?.dispose();
    };
  }, [run, show]);

  const attempt = useCallback(async (work: () => Promise<void>) => {
    try {
      await work();
    } catch (e) {
      setStatus({ text: String(e), error: true });
    }
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
      show(await openProject(path));
      setStatus({ text: `Opened ${path}` });
    });

  const onNew = () =>
    attempt(async () => {
      if (!(await confirm('Start a new project? Anything unsaved in this one is lost.', { title: 'New project', kind: 'warning' }))) return;
      show(await newProject());
      setStatus({ text: 'New project' });
    });

  const onImportSurface = () =>
    attempt(async () => {
      const picked = await open({ multiple: true, filters: [{ name: 'Surfaces', extensions: ['dxf', 'xyz', 'csv', 'txt', 'asc'] }] });
      const paths = picked === null ? [] : Array.isArray(picked) ? picked : [picked];
      for (const path of paths) {
        setStatus({ text: `Importing ${path}…` });
        await run('surface.import', { path, layer: 'surfaces' });
      }
    });

  const onImportVulcan = () =>
    attempt(async () => {
      const picked = await open({ title: "Pick the seam's roof or floor grid", filters: [{ name: 'Vulcan grids', extensions: ['srg', 'sfg'] }] });
      if (!picked || Array.isArray(picked)) return;
      const [dir, file] = splitPath(picked);
      const stem = file.slice(0, file.lastIndexOf('.'));
      setStatus({ text: `Importing seam ${stem}…` });
      await run('grid.import_vulcan_seam', { dir, prefix: '', seam: stem, layer: stem.toUpperCase() });
    });

  const setMode = (next: Mode) => {
    stringsRef.current?.setMode(next);
    setModeState(next);
    setStatus({ text: next === 'draw' ? 'Draw: click on a surface to add vertices; Enter or double-click finishes' : 'Select: click a string, drag its handles; Alt+click inserts, Delete removes a vertex' });
  };

  const setActiveLayer = (name: string) => {
    layerRef.current = name;
    setActiveLayerState(name);
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

  const selectedObject = summary?.objects.find((o) => o.id === selected);
  const projectName = summary?.path ? splitPath(summary.path)[1] : 'Untitled project';

  return (
    <>
      <div ref={hostRef} className="viewport" />
      <header className="toolbar">
        <span className="title">{projectName}</span>
        <button onClick={onNew}>New</button>
        <button onClick={onOpen}>Open…</button>
        <button onClick={() => onSave(false)}>Save</button>
        <button onClick={() => onSave(true)}>Save as…</button>
        <span className="sep" />
        <button onClick={onImportSurface}>Import surface…</button>
        <button onClick={onImportVulcan}>Import Vulcan seam…</button>
        <span className="sep" />
        <button className={mode === 'select' ? 'on' : ''} onClick={() => setMode('select')}>
          Select
        </button>
        <button className={mode === 'draw' ? 'on' : ''} onClick={() => setMode('draw')}>
          Draw string
        </button>
        <span className="sep" />
        <button disabled={!summary?.can_undo} onClick={onUndo} title="Ctrl+Z">
          Undo
        </button>
        <button disabled={!summary?.can_redo} onClick={onRedo} title="Ctrl+Y">
          Redo
        </button>
        <span className="fps" title={backend}>
          {stats ? `${stats.fps.toFixed(0)} fps · ${stats.triangles.toLocaleString()} triangles` : '–'}
        </span>
      </header>

      <aside className="side">
        <h3>Layers</h3>
        <div className="new-layer">
          New strings on
          <input value={activeLayer} onChange={(e) => setActiveLayer(e.target.value || 'design')} />
        </div>
        {summary?.layers.map((layer) => (
          <label key={layer.id} className={`layer ${layer.name === activeLayer ? 'active' : ''}`}>
            <input type="checkbox" checked={layer.visible} onChange={(e) => void attempt(async () => show(await setLayerVisible(layer.id, e.target.checked)))} />
            <span className="swatch" style={{ background: `rgb(${layer.colour.join(',')})` }} />
            <span className="name" onDoubleClick={() => setActiveLayer(layer.name)} title="Double-click to draw new strings on this layer">
              {layer.name}
            </span>
            <span className="count">{summary.objects.filter((o) => o.layer === layer.id).length}</span>
          </label>
        ))}
        <h3>Objects</h3>
        <ul className="objects">
          {summary?.objects.map((o) => (
            <li
              key={o.id}
              className={o.id === selected ? 'selected' : ''}
              onClick={() => o.kind === 'string' && stringsRef.current?.select(o.id)}
              title={`${o.kind}, revision ${o.revision}${o.drawn || o.kind === 'string' ? '' : ' (not drawn: a property grid)'}`}
            >
              <span className={`kind ${o.kind}`}>{o.kind}</span>
              {o.name}
            </li>
          ))}
        </ul>
        {selectedObject && (
          <div className="properties">
            <h3>{selectedObject.name}</h3>
            <div>
              {selectedObject.vertices_local?.length} vertices{selectedObject.closed ? ', closed' : ''} · revision {selectedObject.revision}
            </div>
            <div>layer {summary?.layers.find((l) => l.id === selectedObject.layer)?.name}</div>
          </div>
        )}
      </aside>

      <footer className={`status ${status.error ? 'error' : ''}`}>{status.text}</footer>
    </>
  );
}
