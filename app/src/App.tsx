import { useEffect, useRef, useState } from 'react';
import { open } from '@tauri-apps/plugin-dialog';
import { benchConfig, buildSnapIndex, openSurface } from './bridge';
import { runChecks } from './bench/runChecks';
import { attachPolylineEditing } from './viewport/editing';
import { loadStaged } from './viewport/load';
import { PolylineTool } from './viewport/PolylineTool';
import { type FrameStats, Viewport } from './viewport/Viewport';

export default function App() {
  const hostRef = useRef<HTMLDivElement>(null);
  const viewportRef = useRef<Viewport | null>(null);
  const [stats, setStats] = useState<FrameStats | null>(null);
  const [backend, setBackend] = useState('starting…');
  const [status, setStatus] = useState('');

  useEffect(() => {
    let viewport: Viewport | null = null;
    let detach = () => {};
    let cancelled = false;
    void Viewport.create(hostRef.current!).then(async (vp) => {
      if (cancelled) return vp.dispose();
      viewport = vp;
      viewportRef.current = vp;
      vp.onStats = setStats;
      setBackend(`${vp.backend} · ${vp.adapter}`);
      const tool = new PolylineTool(vp);
      detach = attachPolylineEditing(vp, tool, (ms) => setStatus(`drag update ${ms.toFixed(1)} ms`));
      const config = await benchConfig();
      if (config) await runChecks(vp, tool, config, setStatus);
    });
    return () => {
      cancelled = true;
      detach();
      viewport?.dispose();
    };
  }, []);

  async function onOpen() {
    const viewport = viewportRef.current;
    if (!viewport) return;
    const picked = await open({
      multiple: true,
      filters: [{ name: 'Surfaces', extensions: ['dxf', 'xyz', 'csv', 'txt', 'asc'] }],
    });
    const paths = picked === null ? [] : Array.isArray(picked) ? picked : [picked];
    for (const path of paths) {
      setStatus(`loading ${path}…`);
      const start = performance.now();
      try {
        const staged = await openSurface(path);
        const { timings } = await loadStaged(viewport, staged);
        viewport.fitView();
        const t = staged.timings;
        setStatus(
          `${staged.name}: ${staged.triangles.toLocaleString()} triangles, ` +
            `${(performance.now() - start).toFixed(0)} ms to first frame ` +
            `(read ${t.read.toFixed(0)}, triangulate ${t.triangulate.toFixed(0)}, encode ${t.encode.toFixed(0)}, ` +
            `${(staged.bytes / 1e6).toFixed(1)} MB ${staged.kind} via ${timings.transport} ${timings.fetch_ms.toFixed(0)} ms)\n` +
            staged.log,
        );
      } catch (e) {
        setStatus(`could not open ${path}: ${String(e)}`);
      }
    }
    // Snapping needs the BVH; build it in the background so the surface shows first.
    void buildSnapIndex();
  }

  return (
    <>
      <div ref={hostRef} className="viewport" />
      <div className="hud">
        <div className="hud-row">
          <button onClick={onOpen}>Open surface…</button>
          <span className="fps">
            {stats ? `${stats.fps.toFixed(0)} fps · ${stats.triangles.toLocaleString()} triangles` : '–'}
          </span>
        </div>
        <div className="status">{backend}</div>
        {status && <div className="status">{status}</div>}
      </div>
    </>
  );
}
