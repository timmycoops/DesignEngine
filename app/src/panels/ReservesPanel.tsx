import { FolderOpen, Scale, Sigma } from 'lucide-react';
import { useEffect, useState } from 'react';
import { open } from '@tauri-apps/plugin-dialog';
import { Button } from '@/design-system/ui/button';
import { Input } from '@/design-system/ui/input';
import { Choice, Field, type RunReport, Select, type ToolAction, ToolPanel, ToolSection } from '@/design-system/kit';
import { useWorkspace } from '@/workspace/context';

/**
 * Reserves (decision 0061, workstream 5): interrogate blocks against a seam model, writing each block's seam grids
 * averaged over it onto it as attributes; or set the blocks beside Spry's or Deswik's numbers by a match file. Made from
 * the kit, as every tool panel is.
 */

type Mode = 'interrogate' | 'compare';
type Scope = 'picked' | 'layer' | 'visible';

const MODES: readonly Mode[] = ['interrogate', 'compare'];
const MODE_LABELS: Record<Mode, string> = { interrogate: 'Interrogate', compare: 'Compare' };
const SCOPES: readonly Scope[] = ['picked', 'layer', 'visible'];

export function ReservesPanel() {
  const { summary, selection, runRecord, clearRefusal, toolAsk } = useWorkspace();
  const [mode, setMode] = useState<Mode>('interrogate');
  // A tool dropped on the workflow canvas asks for one of what this does (decision 0067).
  useEffect(() => {
    if (toolAsk?.panel === 'reserves' && MODES.includes(toolAsk.mode as Mode)) setMode(toolAsk.mode as Mode);
  }, [toolAsk]);
  const [scope, setScope] = useState<Scope>('picked');
  const [model, setModel] = useState<string | null>(null);
  const [seamAttribute, setSeamAttribute] = useState('seam');
  const [prefix, setPrefix] = useState('');
  const [variables, setVariables] = useState('');
  const [matches, setMatches] = useState<string | null>(null);
  const [source, setSource] = useState('Spry');
  const [report, setReport] = useState<RunReport>({ notes: [] });
  const [busy, setBusy] = useState(false);

  const objects = summary?.objects ?? [];
  const layers = summary?.layers ?? [];

  // The blocks: those selected, all on the layer of the first of them, or all on show.
  const solids = objects.filter((o) => o.kind === 'solid');
  const pickedIds = new Set(selection);
  const picked = solids.filter((o) => pickedIds.has(o.id));
  const onLayer = picked.length > 0 ? solids.filter((o) => o.layer === picked[0].layer) : [];
  const shown = new Set(layers.filter((l) => l.visible).map((l) => l.id));
  const visible = solids.filter((o) => shown.has(o.layer) && !o.hidden);
  const chosen = scope === 'picked' ? picked : scope === 'layer' ? onLayer : visible;
  const scopeLabels: Record<Scope, string> = { picked: `Picked (${picked.length})`, layer: `Its layer (${onLayer.length})`, visible: `On show (${visible.length})` };

  const models = objects.filter((o) => o.kind === 'seam_model');
  const modelOf = models.find((o) => o.id === model) ?? models[models.length - 1] ?? null;

  const plural = (n: number, word: string) => `${n} ${word}${n === 1 ? '' : 's'}`;
  const codes = variables
    .split(/[\s,]+/)
    .map((v) => v.trim())
    .filter(Boolean);

  const runIt = async (command: string, params: Record<string, unknown>) => {
    setBusy(true);
    clearRefusal();
    const outcome = await runRecord(command, params);
    setBusy(false);
    setReport(outcome.record ? { notes: outcome.record.notes } : { notes: [], error: outcome.error });
  };

  const chooseMatches = async () => {
    const picked = await open({ multiple: false, filters: [{ name: 'Match file', extensions: ['csv'] }] });
    if (typeof picked === 'string') setMatches(picked);
  };

  // What the button runs, and the same as a step for a workflow (decision 0065).
  const calls = (command: string, params: Record<string, unknown>) => ({ run: () => runIt(command, params), step: { command, params } });
  const action: ToolAction =
    mode === 'interrogate'
      ? {
          label: `Interrogate ${plural(chosen.length, 'block')}`,
          icon: <Sigma className="size-3.5" />,
          disabled: chosen.length === 0 || !modelOf || seamAttribute.trim() === '',
          busy,
          busyLabel: 'Interrogating…',
          title: "Each block's seam grids averaged over it by exact partial cells, written onto it as attributes; one undo",
          ...calls('solid.interrogate', {
            solids: chosen.map((o) => o.id),
            model: modelOf?.id,
            seam_attribute: seamAttribute.trim(),
            ...(prefix.trim() ? { prefix: prefix.trim() } : {}),
            ...(codes.length > 0 ? { variables: codes } : {}),
          }),
        }
      : {
          label: `Compare ${plural(chosen.length, 'block')}`,
          icon: <Scale className="size-3.5" />,
          disabled: chosen.length === 0 || !matches,
          busy,
          busyLabel: 'Comparing…',
          title: 'Each value set beside theirs by the match file: how many are within tolerance, and the furthest. Nothing changes',
          hint: 'A report: nothing changes · Ctrl+Enter',
          run: () => runIt('solid.compare', { solids: chosen.map((o) => o.id), matches, source: source.trim() || 'theirs' }),
        };

  return (
    <ToolPanel
      title="Reserves"
      panel="reserves"
      summary="Interrogate blocks against a seam model, writing each seam grid's value onto them, or set them beside Spry's or Deswik's numbers."
      result={report}
      action={action}
    >
      <ToolSection>
        <Choice value={mode} options={MODES} labels={MODE_LABELS} onChange={setMode} label="What to do" />
      </ToolSection>

      <ToolSection title="Blocks">
        <Field label="Take" hint={chosen.length === 0 ? 'Pick a block in the view or the tree, or take those on show' : chosen.length === 1 ? chosen[0].name : `${chosen.length} blocks`}>
          <Choice value={scope} options={SCOPES} labels={scopeLabels} onChange={setScope} label="Which blocks" />
        </Field>
      </ToolSection>

      {mode === 'interrogate' && (
        <ToolSection title="Geology">
          <Field label="Seam model">
            {models.length === 0 ? (
              <span className="text-muted-foreground">Build a seam model first: Seam model on the toolbar</span>
            ) : (
              <Select value={modelOf?.id ?? ''} aria-label="Seam model" onChange={(e) => setModel(e.target.value)}>
                {models.map((o) => (
                  <option key={o.id} value={o.id}>
                    {o.name}
                  </option>
                ))}
              </Select>
            )}
          </Field>
          <Field label="Seam from" hint="The attribute naming each block's seam, as the grids name it (seam1 of grnseam1.asg)">
            <Input className="h-7 w-40 px-1.5 text-xs" value={seamAttribute} aria-label="Attribute naming each block's seam" onChange={(e) => setSeamAttribute(e.target.value)} />
          </Field>
          <Field label="Variables" hint="Codes to write, such as as rd; blank for every grid of each block's seam">
            <Input className="h-7 w-40 px-1.5 text-xs" value={variables} placeholder="every grid" aria-label="Variables to write" onChange={(e) => setVariables(e.target.value)} />
          </Field>
          <Field label="Prefix" hint="Put in front of each code, where the blocks already have an attribute of that name">
            <Input className="h-7 w-40 px-1.5 text-xs" value={prefix} placeholder="none" aria-label="Prefix for the attributes written" onChange={(e) => setPrefix(e.target.value)} />
          </Field>
          <div className="text-muted-foreground">Density by volume and every other grid by coal tonnes. The 2.5D column volume is written too, as a check.</div>
        </ToolSection>
      )}

      {mode === 'compare' && (
        <ToolSection title="Beside">
          <Field label="Their numbers">
            <Choice value={source === 'Deswik' ? 'Deswik' : 'Spry'} options={['Spry', 'Deswik'] as const} labels={{ Spry: 'Spry', Deswik: 'Deswik' }} onChange={setSource} label="Whose numbers" />
          </Field>
          <Field label="Match file" hint={matches ?? 'source,ours,tolerance: their field, our code or volume, and how near'}>
            <Button variant="outline" size="sm" className="h-7 px-2 text-xs" onClick={() => void chooseMatches()}>
              <FolderOpen className="size-3.5" /> {matches ? matches.split(/[\\/]/).pop() : 'Choose…'}
            </Button>
          </Field>
        </ToolSection>
      )}
    </ToolPanel>
  );
}
