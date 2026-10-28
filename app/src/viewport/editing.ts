// Click on a surface to add a polyline vertex; drag a vertex to move it, snapping to the nearest
// surface under the cursor (BVH ray cast in Rust). Escape clears the polyline.
import { snapRay } from '../bridge';
import type { PolylineTool } from './PolylineTool';
import type { Viewport } from './Viewport';

const CLICK_SLOP_PX = 4;

export function attachPolylineEditing(
  viewport: Viewport,
  tool: PolylineTool,
  onDragUpdate: (ms: number) => void,
): () => void {
  const canvas = viewport.renderer.domElement;
  let dragging = -1;
  let down: { x: number; y: number } | null = null;
  let latest: { x: number; y: number } | null = null;
  let inFlight = false;

  const snapAt = (x: number, y: number) => {
    const [nx, ny] = viewport.clientToNdc(x, y);
    const ray = viewport.rayFromNdc(nx, ny);
    return snapRay(ray.origin, ray.direction);
  };

  // One snap request in flight; pointer moves that arrive meanwhile collapse to the latest.
  const pump = async () => {
    if (inFlight || !latest || dragging < 0) return;
    inFlight = true;
    const target = latest;
    latest = null;
    const start = performance.now();
    const hit = await snapAt(target.x, target.y);
    if (hit && dragging >= 0) {
      tool.move(dragging, hit.local);
      await viewport.nextFrame();
      onDragUpdate(performance.now() - start);
    }
    inFlight = false;
    void pump();
  };

  // Capture phase runs before OrbitControls' own listener on the canvas.
  const onPointerDown = (e: PointerEvent) => {
    if (e.button !== 0) return;
    const index = tool.pick(e.clientX, e.clientY);
    if (index >= 0) {
      dragging = index;
      viewport.controls.enabled = false;
      canvas.setPointerCapture(e.pointerId);
      e.stopImmediatePropagation();
    } else {
      down = { x: e.clientX, y: e.clientY };
    }
  };

  const onPointerMove = (e: PointerEvent) => {
    if (dragging < 0) return;
    latest = { x: e.clientX, y: e.clientY };
    void pump();
  };

  const onPointerUp = async (e: PointerEvent) => {
    if (dragging >= 0) {
      dragging = -1;
      viewport.controls.enabled = true;
      canvas.releasePointerCapture(e.pointerId);
      return;
    }
    if (down && Math.hypot(e.clientX - down.x, e.clientY - down.y) <= CLICK_SLOP_PX) {
      const hit = await snapAt(e.clientX, e.clientY);
      if (hit) tool.add(hit.local);
    }
    down = null;
  };

  const onKeyDown = (e: KeyboardEvent) => {
    if (e.key === 'Escape') tool.clear();
  };

  canvas.addEventListener('pointerdown', onPointerDown, { capture: true });
  canvas.addEventListener('pointermove', onPointerMove);
  canvas.addEventListener('pointerup', onPointerUp);
  window.addEventListener('keydown', onKeyDown);
  return () => {
    canvas.removeEventListener('pointerdown', onPointerDown, { capture: true });
    canvas.removeEventListener('pointermove', onPointerMove);
    canvas.removeEventListener('pointerup', onPointerUp);
    window.removeEventListener('keydown', onKeyDown);
  };
}
