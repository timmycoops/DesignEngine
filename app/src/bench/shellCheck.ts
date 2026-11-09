// `--shell`: the panels and the toolbar, as Tim would use them. Panels minimised by their header's button to strips
// at the window's edges and brought back by a name on a strip, kept that way through the layout being saved and read
// again; every panel hidden by the ` key and brought back; and the toolbar's groups, with what came off it in the
// Import menu. The layout saved on this machine is put back as it was at the end.
import type { DockviewApi } from 'dockview-react';
import type { Viewport } from '../viewport/Viewport';
import { resetLayout } from '../workspace/Workspace';

const LAYOUT_KEYS = ['zd.workspace.layout.v3', 'zd.workspace.minimised.v1'];

const wait = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));

export async function checkShell(viewport: Viewport, log: (line: string) => void, dock: () => DockviewApi | null) {
  const api = dock();
  if (!api) return { error: 'no dock' };
  const results: Record<string, unknown> = {};
  const failures: string[] = [];
  const expect = (what: string, ok: boolean, detail?: unknown) => {
    results[what] = detail === undefined ? ok : { ok, detail };
    if (!ok) failures.push(what);
    log(`shell: ${what}: ${ok ? 'yes' : 'NO'}${detail === undefined ? '' : ` (${JSON.stringify(detail)})`}`);
  };
  const saved = LAYOUT_KEYS.map((key) => [key, localStorage.getItem(key)] as const);
  const canvas = viewport.renderer.domElement;
  const settle = async () => {
    await wait(300);
    await viewport.nextFrame();
  };
  const size = () => ({ width: canvas.clientWidth, height: canvas.clientHeight });
  const buttonLabelled = (label: string) => document.querySelector<HTMLButtonElement>(`button[aria-label="${label}"]`);
  const strip = (edge: string) => [...document.querySelectorAll(`[data-slot="minimised-strip"][data-edge="${edge}"] button`)].map((b) => b.textContent?.trim() ?? '');
  const stripButton = (name: string) => [...document.querySelectorAll<HTMLButtonElement>('[data-slot="minimised-strip"] button')].find((b) => b.textContent?.trim() === name);
  const key = async (name: string) => {
    window.dispatchEvent(new KeyboardEvent('keydown', { key: name, bubbles: true }));
    await settle();
  };

  try {
    if (api.hasMaximizedGroup()) api.exitMaximizedGroup();
    resetLayout(api);
    await settle();
    const start = size();
    log(`shell: the default layout, viewport ${start.width} x ${start.height}`);

    buttonLabelled('Minimise Properties')?.click();
    await settle();
    const noProperties = size();
    expect('minimising Properties gives the viewport its width', noProperties.width > start.width + 200, { before: start.width, after: noProperties.width });
    expect('Properties waits on a strip on the right', strip('right').includes('Properties'), strip('right'));

    buttonLabelled('Minimise Layers')?.click();
    buttonLabelled('Minimise Run log')?.click();
    await settle();
    const bare = size();
    expect('Layers on the left and the run log at the bottom', strip('left').includes('Layers') && strip('bottom').includes('Run log'), { left: strip('left'), bottom: strip('bottom') });
    expect('the viewport takes their room too', bare.width > noProperties.width + 150 && bare.height > noProperties.height + 120, { before: noProperties, after: bare });

    // Read back as the app does when it starts: the groups stay minimised, on the same strips.
    const layout = localStorage.getItem(LAYOUT_KEYS[0]);
    if (layout) {
      api.fromJSON(JSON.parse(layout));
      await settle();
    }
    expect('minimised groups stay so when the layout is read again', strip('right').includes('Properties') && strip('left').includes('Layers') && strip('bottom').includes('Run log'), {
      left: strip('left'),
      right: strip('right'),
      bottom: strip('bottom'),
    });

    stripButton('Properties')?.click();
    await settle();
    const back = size();
    expect('a click on its name brings Properties back at its size', !strip('right').includes('Properties') && Math.abs(bare.width - back.width - (noProperties.width - start.width)) < 4, {
      width_given_back: bare.width - back.width,
      width_it_took: noProperties.width - start.width,
    });

    await key('`');
    const full = size();
    expect('` hides every panel and the strips: the viewport fills the window', full.width >= window.innerWidth - 8 && strip('left').length === 0, { viewport: full, window: [window.innerWidth, window.innerHeight] });
    await key('`');
    // The strips go with the panels, so dockview shares their few pixels out again on the way back: within 10 px.
    expect('` again brings them back as they were', Math.abs(size().width - back.width) <= 10 && strip('left').includes('Layers'), { before: back.width, after: size().width });

    // The toolbar: groups, with the imports in one menu.
    const header = [...document.querySelectorAll('header button')].map((b) => b.textContent?.trim() ?? '');
    const gone = ['Surface', 'Solids', 'Strings', 'Deswik', 'Spry', 'Vulcan grid', 'Seam model'].filter((word) => header.includes(word));
    expect('the toolbar has Import, Export and Panels menus, and no import buttons of its own', ['Import', 'Export', 'Panels'].every((word) => header.includes(word)) && gone.length === 0, { still_there: gone });
    const trigger = [...document.querySelectorAll<HTMLButtonElement>('header button')].find((b) => b.textContent?.trim() === 'Import');
    trigger?.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, button: 0, pointerType: 'mouse', ctrlKey: false }));
    await settle();
    const items = [...document.querySelectorAll('[role="menuitem"]')].map((m) => m.textContent?.trim() ?? '');
    const wanted = ['Surfaces…', 'Vulcan grid…', 'Strings…', 'Solids', 'Deswik drawing (.dcf)…', 'Spry project (.spry)…', 'Suggest a stack definition (CSV)…', 'Build and check from a definition…'];
    expect('the Import menu holds every import and the seam model', wanted.every((w) => items.includes(w)), { missing: wanted.filter((w) => !items.includes(w)) });
    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
    await settle();
  } finally {
    // This machine's layout, as it was, in the app and in its storage.
    for (const [key, value] of saved) {
      if (value === null) localStorage.removeItem(key);
      else localStorage.setItem(key, value);
    }
    const layout = saved[0][1];
    if (layout) {
      try {
        api.fromJSON(JSON.parse(layout));
      } catch {
        // Stored, so it's what the app opens with next time.
      }
    }
  }
  results.failures = failures;
  return results;
}
