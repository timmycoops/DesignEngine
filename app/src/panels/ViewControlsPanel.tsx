import { useEffect, useState } from 'react';
import { Choice, PanelHead } from '@/design-system/kit';
import { Button } from '@/design-system/ui/button';
import {
  DEFAULT_VIEW_CONTROLS,
  keyClash,
  keyLabel,
  type MouseLayout,
  setViewControls,
  VIEW_CONTROLS_CHANGED,
  VIEW_KEY_NAMES,
  VIEW_KEYS,
  type ViewKey,
  viewControls,
} from '@/viewport/viewControls';

const LAYOUTS: readonly MouseLayout[] = ['spry', 'ours'];
const LAYOUT_WORDS: Record<MouseLayout, string> = { spry: "Spry's", ours: 'Ours' };

/** What each button does in a layout, as a row of the panel says it. */
const LAYOUT_ROWS: Record<MouseLayout, [string, string][]> = {
  spry: [
    ['Turn about the cursor', 'Middle and then left, or hold the turn key and left'],
    ['Pan', 'Middle drag, middle and right, right drag, or hold the pan key and left'],
    ['Select', 'Left click; left drag for a box'],
    ['Zoom', 'The wheel, towards the cursor'],
  ],
  ours: [
    ['Turn about the cursor', 'Left drag, or hold the turn key and left'],
    ['Pan', 'Right drag (left in plan and section), or hold the pan key and left'],
    ['Select', 'Left click; Shift and a left drag for a box'],
    ['Zoom', 'The wheel towards the cursor, or a middle drag'],
  ],
};

/**
 * View controls (decision 0044, section 7): Spry's mouse layout or ours, and the view's keys, each changed by pressing
 * the new one. Saved on this machine, not in the project.
 */
export function ViewControlsPanel() {
  const [controls, setControls] = useState(viewControls);
  const [listening, setListening] = useState<ViewKey | null>(null);
  const [refusal, setRefusal] = useState<string | null>(null);

  useEffect(() => {
    const changed = () => setControls(viewControls());
    window.addEventListener(VIEW_CONTROLS_CHANGED, changed);
    return () => window.removeEventListener(VIEW_CONTROLS_CHANGED, changed);
  }, []);

  // The next key pressed is the new one, before the view or any shortcut sees it; Esc stops listening.
  useEffect(() => {
    if (!listening) return;
    const pressed = (e: KeyboardEvent) => {
      e.preventDefault();
      e.stopImmediatePropagation();
      if (['Shift', 'Control', 'Alt', 'Meta'].includes(e.key)) return;
      if (e.key === 'Escape') {
        setListening(null);
        setRefusal(null);
        return;
      }
      if (e.ctrlKey || e.metaKey || e.altKey) return setRefusal('A view key is one key alone, without Ctrl or Alt');
      const key = e.key.toLowerCase();
      const why = keyClash(listening, key, controls);
      if (why) return setRefusal(why);
      setViewControls({ ...controls, keys: { ...controls.keys, [listening]: key } });
      setRefusal(null);
      setListening(null);
    };
    window.addEventListener('keydown', pressed, { capture: true });
    return () => window.removeEventListener('keydown', pressed, { capture: true });
  }, [listening, controls]);

  const setLayout = (layout: MouseLayout) => setViewControls({ ...controls, layout });
  const changedFromDefaults = controls.layout !== DEFAULT_VIEW_CONTROLS.layout || VIEW_KEYS.some((k) => controls.keys[k] !== DEFAULT_VIEW_CONTROLS.keys[k]);

  return (
    <div className="flex h-full flex-col text-xs">
      <PanelHead
        line="How the mouse and keys move the view: Spry's layout or ours, and the view's keys"
        more="Saved on this machine, not in the project. A key is changed by clicking it and pressing the new one; one that another key or a tool already uses is refused, saying which. Esc stops without changing it."
      />
      <div className="flex flex-col gap-3 overflow-auto p-3">
        <div className="flex flex-col gap-1.5">
          <div className="flex items-center gap-2">
            <span className="w-20 shrink-0 text-muted-foreground">Mouse</span>
            <Choice value={controls.layout} options={LAYOUTS} labels={LAYOUT_WORDS} onChange={setLayout} label="Mouse layout" />
          </div>
          <dl className="grid grid-cols-[7.5rem_1fr] gap-x-2 gap-y-0.5">
            {LAYOUT_ROWS[controls.layout].map(([what, how]) => (
              <div key={what} className="contents">
                <dt className="text-muted-foreground">{what}</dt>
                <dd>{how}</dd>
              </div>
            ))}
          </dl>
        </div>
        <div className="flex flex-col gap-1">
          <span className="text-muted-foreground">Keys</span>
          {VIEW_KEYS.map((which) => (
            <div key={which} className="flex min-h-7 items-center gap-2">
              <span className="min-w-0 flex-1">{VIEW_KEY_NAMES[which]}</span>
              <Button
                size="sm"
                variant={listening === which ? 'default' : 'outline'}
                className="h-6 w-28 font-mono"
                aria-label={`${VIEW_KEY_NAMES[which]}: ${keyLabel(controls.keys[which])}`}
                onClick={(e) => {
                  // Not left in focus: Space, pressed as the new key, would press the button again.
                  e.currentTarget.blur();
                  setRefusal(null);
                  setListening(listening === which ? null : which);
                }}
              >
                {listening === which ? 'Press a key…' : keyLabel(controls.keys[which])}
              </Button>
            </div>
          ))}
          {refusal && <div className="text-destructive">{refusal}</div>}
        </div>
        <Button size="sm" variant="outline" className="self-start" disabled={!changedFromDefaults} onClick={() => setViewControls(structuredClone(DEFAULT_VIEW_CONTROLS))}>
          Back to the defaults
        </Button>
      </div>
    </div>
  );
}
