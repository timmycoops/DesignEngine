// `--pictures <folder>`: the `--surfaces` imported as the app imports them, drawn in a run of looks (colour by RL
// over each one's own RLs and over one range, see-through, the layer's colour) and saved as PNGs, so the look can
// be judged without anyone driving the app. The first is topography alone; the rest have every surface on show.
import * as THREE from 'three/webgpu';
import { type Contours, type Display, type ProjectSummary, runCommand, setContours, setLooks, setObjectsVisible, writePicture } from '../bridge';
import type { Viewport } from '../viewport/Viewport';

type Show = (summary: ProjectSummary) => Promise<void> | undefined;

/** Frames drawn before a picture, so a change of pipeline (solid to see-through) has compiled. */
const SETTLE_FRAMES = 20;

/** `strings` (DXF or CSV) are imported after the surfaces, to show that they still draw on top of see-through ones. */
export async function takePictures(viewport: Viewport, paths: string[], strings: string[], log: (line: string) => void, show: Show): Promise<string[]> {
  let summary: ProjectSummary | null = null;
  const ids: string[] = [];
  for (const path of paths) {
    log(`pictures: importing ${path}`);
    const reply = await runCommand('surface.import', { path, layer: 'surfaces' });
    summary = reply.summary;
    ids.push(reply.record.outputs[0].id);
    await show(summary);
  }
  for (const path of strings) {
    log(`pictures: importing ${path}`);
    summary = (await runCommand('string.import', { path })).summary;
    await show(summary);
  }
  if (!summary) return [];
  let current: ProjectSummary = summary;
  const restyle = async (display: Display) => {
    current = { ...current, display };
    await show(current);
  };
  const contours = (c: Partial<Contours>) => setContours({ ...current.display.contours, ...c });
  const saved: string[] = [];
  const picture = async (name: string) => {
    for (let i = 0; i < SETTLE_FRAMES; i++) await viewport.nextFrame();
    // Read straight after drawing, in the same task, while the canvas still holds the frame.
    viewport.draw();
    const path = await writePicture(name, viewport.renderer.domElement.toDataURL('image/png'));
    log(`pictures: ${path}`);
    saved.push(path);
  };

  // Topography alone, then everything, each over its own RLs.
  const [topography, ...others] = ids;
  if (others.length > 0) current = await setObjectsVisible(others, false);
  await show(current);
  viewport.fitView();
  await picture('01-topography-by-rl');
  if (others.length > 0) {
    current = await setObjectsVisible(others, true);
    await show(current);
  }
  viewport.fitView();
  await picture('02-all-by-rl-own-range');
  await restyle(await contours({ shared: true }));
  await picture('03-all-by-rl-same-colours');
  await restyle(await contours({ shared: false }));

  // Close in, from the same side, to see the contour lines at full size.
  const box = viewport.bounds;
  const centre = box.getCenter(new THREE.Vector3());
  const size = box.getSize(new THREE.Vector3());
  viewport.focusOn([centre.x, centre.y, centre.z], size.length() * 0.18);
  await picture('04-close-up-contours');
  await restyle(await contours({ interval: 5 }));
  await picture('05-close-up-5m');
  await restyle(await contours({ interval: null }));

  // See-through: the topography at 50 % over the rest, then everything at 50 %, from the usual angle and low down.
  viewport.fitView();
  await restyle(await setLooks([topography], null, 0.5));
  await picture('06-topography-50pc');
  await restyle(await setLooks(ids, null, 0.5));
  await picture('07-all-50pc');
  const low = new THREE.Vector3(0.6, -0.78, 0.18).normalize();
  viewport.perspective.position.copy(centre).addScaledVector(low, size.length() * 0.75);
  viewport.controls.target.copy(centre);
  viewport.controls.update();
  await picture('08-all-50pc-low-angle');
  await restyle(await setLooks(ids, null, 1));

  // From above, and in the layer's colour, as it was before.
  viewport.setPlanView(true);
  viewport.fitView();
  await picture('09-plan-by-rl');
  viewport.setPlanView(false);
  viewport.fitView();
  await restyle(await setLooks(ids, 'layer', null));
  await picture('10-all-layer-colour');
  await restyle(await setLooks(ids, 'by_rl', null));
  return saved;
}
