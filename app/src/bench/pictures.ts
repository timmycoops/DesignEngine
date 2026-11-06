// `--pictures <folder>`: the `--surfaces` imported as the app imports them, drawn in a run of looks (colour by RL
// over each one's own RLs and over one range, see-through, the layer's colour) and saved as PNGs, so the look can
// be judged without anyone driving the app. The first is topography alone; the rest have every surface on show.
// With `--palettes` as well, the same few views are drawn on every RL ramp instead, and put on one sheet.
import * as THREE from 'three/webgpu';
import { type Contours, type Display, type ProjectSummary, runCommand, setContours, setLooks, setObjectsVisible, writePicture } from '../bridge';
import { paletteColour, RL_PALETTE_NAMES, RL_PALETTES } from '../viewport/rlPalettes';
import { rlPalette, setRlPalette } from '../viewport/rlColour';
import type { Viewport } from '../viewport/Viewport';

type Show = (summary: ProjectSummary) => Promise<void> | undefined;

/** Frames drawn before a picture, so a change of pipeline (solid to see-through) has compiled. */
const SETTLE_FRAMES = 20;

/** Imports the surfaces and then the strings, as the app does: the last summary, and the surfaces' ids in order. */
async function importAll(paths: string[], strings: string[], log: (line: string) => void, show: Show) {
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
  return { summary, ids };
}

/** Draws a few frames, then the frame as a PNG data URL, read straight after drawing while the canvas holds it. */
async function grab(viewport: Viewport): Promise<string> {
  for (let i = 0; i < SETTLE_FRAMES; i++) await viewport.nextFrame();
  viewport.draw();
  return viewport.renderer.domElement.toDataURL('image/png');
}

/** `strings` (DXF or CSV) are imported after the surfaces, to show that they still draw on top of see-through ones. */
export async function takePictures(viewport: Viewport, paths: string[], strings: string[], log: (line: string) => void, show: Show): Promise<string[]> {
  const { summary, ids } = await importAll(paths, strings, log, show);
  if (!summary) return [];
  let current: ProjectSummary = summary;
  const restyle = async (display: Display) => {
    current = { ...current, display };
    await show(current);
  };
  const contours = (c: Partial<Contours>) => setContours({ ...current.display.contours, ...c });
  const saved: string[] = [];
  const picture = async (name: string) => {
    const path = await writePicture(name, await grab(viewport));
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

/** A picture on the palette sheet, a tile this many pixels wide; its height keeps the canvas's shape. */
const TILE_WIDTH = 640;
const GAP = 8;
const LABEL_WIDTH = 210;
const HEADER = 44;

/**
 * `--palettes`: four views (topography alone, everything, a close-up of the contours, plan), each drawn on every
 * RL ramp from the same camera, saved one by one as `<view>-<palette>` and all together as `palette-sheet`:
 * a row per ramp, a column per view.
 */
export async function takePalettePictures(viewport: Viewport, paths: string[], strings: string[], log: (line: string) => void, show: Show): Promise<string[]> {
  const { summary, ids } = await importAll(paths, strings, log, show);
  if (!summary) return [];
  let current: ProjectSummary = summary;
  const [, ...others] = ids;
  const was = rlPalette();
  const canvas = viewport.renderer.domElement;
  const tileHeight = Math.round((TILE_WIDTH * canvas.height) / canvas.width);
  const views = ['topography', 'everything', 'close-up', 'plan'];
  const sheet = document.createElement('canvas');
  sheet.width = LABEL_WIDTH + views.length * (TILE_WIDTH + GAP) + GAP;
  sheet.height = HEADER + RL_PALETTE_NAMES.length * (tileHeight + GAP) + GAP;
  const ctx = sheet.getContext('2d');
  if (!ctx) throw new Error('no 2D canvas for the palette sheet');
  ctx.fillStyle = '#0b1220';
  ctx.fillRect(0, 0, sheet.width, sheet.height);
  ctx.fillStyle = '#e5e7eb';
  ctx.font = '600 22px system-ui, sans-serif';
  ctx.textBaseline = 'middle';
  views.forEach((v, col) => ctx.fillText(v, LABEL_WIDTH + GAP + col * (TILE_WIDTH + GAP), HEADER / 2));
  RL_PALETTE_NAMES.forEach((name, row) => {
    const top = HEADER + row * (tileHeight + GAP);
    ctx.fillStyle = '#e5e7eb';
    ctx.font = '600 22px system-ui, sans-serif';
    ctx.fillText(RL_PALETTES[name].label, GAP, top + 28);
    ctx.font = '16px system-ui, sans-serif';
    ctx.fillStyle = '#9ca3af';
    ctx.fillText('low RL → high', GAP, top + 58);
    // The ramp, low RL at the bottom, as a strip down the label.
    const stripTop = top + 80;
    const stripHeight = tileHeight - 92;
    for (let y = 0; y < stripHeight; y++) {
      const [r, g, b] = paletteColour(RL_PALETTES[name].stops, 1 - y / (stripHeight - 1));
      ctx.fillStyle = `rgb(${Math.round(r)} ${Math.round(g)} ${Math.round(b)})`;
      ctx.fillRect(GAP, stripTop + y, 36, 1);
    }
  });

  const saved: string[] = [];
  const shoot = async (view: string, col: number) => {
    for (const [row, name] of RL_PALETTE_NAMES.entries()) {
      setRlPalette(name);
      const png = await grab(viewport);
      const path = await writePicture(`${view}-${name}`, png);
      log(`pictures: ${path}`);
      saved.push(path);
      const image = new Image();
      image.src = png;
      await image.decode();
      ctx.drawImage(image, LABEL_WIDTH + GAP + col * (TILE_WIDTH + GAP), HEADER + row * (tileHeight + GAP), TILE_WIDTH, tileHeight);
    }
  };

  if (others.length > 0) current = await setObjectsVisible(others, false);
  await show(current);
  viewport.fitView();
  await shoot(views[0], 0);
  if (others.length > 0) current = await setObjectsVisible(others, true);
  await show(current);
  viewport.fitView();
  await shoot(views[1], 1);
  const box = viewport.bounds;
  const centre = box.getCenter(new THREE.Vector3());
  viewport.focusOn([centre.x, centre.y, centre.z], box.getSize(new THREE.Vector3()).length() * 0.18);
  await shoot(views[2], 2);
  viewport.setPlanView(true);
  viewport.fitView();
  await shoot(views[3], 3);
  viewport.setPlanView(false);
  setRlPalette(was);

  const path = await writePicture('palette-sheet', sheet.toDataURL('image/png'));
  log(`pictures: ${path}`);
  saved.push(path);
  return saved;
}
