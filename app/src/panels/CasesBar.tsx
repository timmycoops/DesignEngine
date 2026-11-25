import { Copy, Plus, Workflow } from 'lucide-react';
import { useEffect, useMemo, useState } from 'react';
import { type CaseHeadline, caseHeadlines, type Dataflow, dataflowStarter, objectDetails, objectsAttributes, type ParameterTable, reservesRerunParams, tableSetupView } from '@/bridge';
import { CellInput, FilterInput, Select } from '@/design-system/kit';
import { Button } from '@/design-system/ui/button';
import { Checkbox } from '@/design-system/ui/checkbox';
import { cn } from 'cn';
import type { Attributes, When } from '../filters';
import { useWorkspace } from '@/workspace/context';
import { againstBase, blocksFitting, EMPTY_ROW_DATAFLOW, freeName, HEADLINES, headlineWords } from './newCase';

/**
 * The cases (Reserves case plan, decision 13, workstream 7): every reserves table as a case, its headline figures against
 * Base, a click opening it; and the way in, New case, Copy case and Add a dataflow, each a short form here, each what it
 * makes its own undo. New case runs a reserves dataflow over the blocks on a layer that a filter keeps (the starter made
 * from the seam model where the project has none); Copy case runs the case open again into a new table under a new name,
 * its settings tables shared or copied; Add a dataflow puts an empty row dataflow, or a copy of one, after the case's.
 */

type Form = 'new' | 'copy' | 'add' | null;
type Said = { text: string; refused: boolean } | null;

export function CasesBar({ open, onOpen, onOpenDataflow }: { open: string | null; onOpen: (table: string) => void; onOpenDataflow: (dataflow: string) => void }) {
  const { summary, runRecord, openTool } = useWorkspace();
  const [heads, setHeads] = useState<CaseHeadline[]>([]);
  const [form, setForm] = useState<Form>(null);
  const [said, setSaid] = useState<Said>(null);
  const [busy, setBusy] = useState(false);
  const changed = (summary?.objects ?? []).map((o) => `${o.id}:${o.revision}`).join(',');
  useEffect(() => {
    let current = true;
    void caseHeadlines()
      .then((h) => current && setHeads(h))
      .catch(() => current && setHeads([]));
    return () => {
      current = false;
    };
  }, [changed]);
  const base = heads.find((h) => h.base) ?? null;
  const names = (summary?.objects ?? []).map((o) => o.name);

  // Each form's run, its own undos; what it said.
  const go = async (what: () => Promise<string>) => {
    setBusy(true);
    setSaid(null);
    try {
      setSaid({ text: await what(), refused: false });
      setForm(null);
    } catch (e) {
      setSaid({ text: e instanceof Error ? e.message : String(e), refused: true });
    } finally {
      setBusy(false);
    }
  };
  const made = (reply: { record?: { outputs: { id: string; change: string }[] }; error?: string }, kind: string) => {
    if (reply.error || !reply.record) throw new Error(reply.error ?? `no ${kind} made`);
    const id = reply.record.outputs.find((o) => o.change === 'created')?.id;
    if (!id) throw new Error(`no ${kind} made`);
    return id;
  };

  return (
    <div className="shrink-0 border-b border-border px-2 py-1.5" data-slot="cases">
      <div className="flex flex-wrap items-center gap-2">
        <span className="text-muted-foreground">Cases, against {base?.name ?? 'Base'}</span>
        <Button size="sm" variant="outline" className="h-6 px-2" disabled={busy} onClick={() => setForm(form === 'new' ? null : 'new')} data-slot="case-new">
          <Plus className="size-3" /> New case
        </Button>
        <Button size="sm" variant="outline" className="h-6 px-2" disabled={busy || !open} onClick={() => setForm(form === 'copy' ? null : 'copy')} data-slot="case-copy">
          <Copy className="size-3" /> Copy case
        </Button>
        <Button size="sm" variant="outline" className="h-6 px-2" disabled={busy || !open} onClick={() => setForm(form === 'add' ? null : 'add')} data-slot="case-add-dataflow">
          <Workflow className="size-3" /> Add a dataflow
        </Button>
        {said && (
          <span className={cn('min-w-0 truncate', said.refused ? 'text-destructive' : 'text-ok')} title={said.text} data-slot="cases-said">
            {said.text}
          </span>
        )}
      </div>
      {heads.length > 0 && (
        <table className="mt-1 border-collapse tabular-nums" data-slot="cases-table">
          <thead>
            <tr className="text-muted-foreground">
              <th className="pr-3 text-left font-normal">Case</th>
              {HEADLINES.map((h) => (
                <th key={h.key} className="px-2 text-right font-normal">
                  {h.label} ({h.unit})
                </th>
              ))}
            </tr>
          </thead>
          <tbody>
            {heads.map((c) => (
              <tr key={c.table} className={cn(c.table === open && 'bg-selection')} data-slot="case-headline" data-case={c.name} data-base={c.base ? 'yes' : 'no'}>
                <td className="pr-3">
                  <button type="button" className={cn('hover:underline', c.table === open && 'font-semibold text-primary')} onClick={() => onOpen(c.table)} title={`Open ${c.name}`}>
                    {c.name}
                    {c.base && <span className="ml-1 text-[10px] text-muted-foreground uppercase">Base</span>}
                  </button>
                </td>
                {HEADLINES.map((h) => {
                  const change = againstBase(c[h.key], base?.[h.key] ?? null, c.base);
                  return (
                    <td key={h.key} className="px-2 text-right whitespace-nowrap" data-field={h.key} data-value={c[h.key] ?? ''}>
                      {headlineWords(c[h.key], h.dp)}
                      {change && <span className={cn('ml-1 text-[11px]', change.startsWith('−') ? 'text-destructive' : change.startsWith('+') ? 'text-ok' : 'text-muted-foreground')}>{change}</span>}
                    </td>
                  );
                })}
              </tr>
            ))}
          </tbody>
        </table>
      )}
      {form === 'new' && <NewCase busy={busy} names={names} onMake={(run) => void go(() => run(made))} onOpen={onOpen} />}
      {form === 'copy' && (
        <CopyCase
          busy={busy}
          from={heads.find((h) => h.table === open) ?? null}
          names={names}
          onMake={(name, copyTables) =>
            void go(async () => {
              if (!open) throw new Error('Open a case to copy first');
              const setup = await tableSetupView(open);
              let tables = setup.setup.tables;
              if (copyTables) {
                tables = [];
                for (const t of setup.tables) {
                  const d = await objectDetails(t.id);
                  if (d.measures.kind !== 'parameter_table') throw new Error(`${t.name} isn't a parameter table`);
                  tables.push(made(await runRecord('parameters.save', { name: freeName(`${t.name} (${name})`, names), content: d.measures.parameters as ParameterTable }), 'table'));
                }
              }
              const p = await reservesRerunParams(open, []);
              const id = made(await runRecord('reserves.run', { ...p, table: null, previous: null, only: [], name, ...(tables ? { tables } : {}) }), 'table');
              // Its dataflows after the reserves one, as the case copied has them, out of date until run.
              if ((setup.setup.row_dataflows ?? []).length > 0) {
                const now = await tableSetupView(id);
                const reply = await runRecord('table.setup', { table: id, setup: { ...now.setup, row_dataflows: setup.setup.row_dataflows } });
                if (reply.error) throw new Error(reply.error);
              }
              onOpen(id);
              return `${name} made from ${setup.table.name}${copyTables ? ', its settings tables copied' : ', sharing its settings tables'}.`;
            })
          }
        />
      )}
      {form === 'add' && (
        <AddDataflow
          busy={busy}
          names={names}
          onMake={(name, from, range) =>
            void go(async () => {
              let content: object = EMPTY_ROW_DATAFLOW;
              if (from) {
                const d = await objectDetails(from);
                if (d.measures.kind !== 'dataflow') throw new Error('that is not a dataflow');
                content = d.measures.dataflow as Dataflow;
              }
              if (!open) throw new Error('Open a case to add a dataflow to first');
              const id = made(await runRecord('dataflow.save', { name, content }), 'dataflow');
              const now = await tableSetupView(open);
              const reply = await runRecord('table.setup', { table: open, setup: { ...now.setup, row_dataflows: [...(now.setup.row_dataflows ?? []), { dataflow: id, ...(Object.keys(range).length ? { range } : {}) }] } });
              if (reply.error) throw new Error(reply.error);
              onOpenDataflow(id);
              if (!from) openTool('params', `show:${id}`);
              return from ? `${name} added after the case's dataflows, out of date until it runs.` : `${name} added with one step to write over: its fields and steps are in the Parameters tab.`;
            })
          }
        />
      )}
    </div>
  );
}

type Made = (reply: { record?: { outputs: { id: string; change: string }[] }; error?: string }, kind: string) => string;

/** New case: a name, the blocks (a layer and a filter), the seam model and the reserves dataflow, or the starter. */
function NewCase({ busy, names, onMake, onOpen }: { busy: boolean; names: string[]; onMake: (run: (made: Made) => Promise<string>) => void; onOpen: (table: string) => void }) {
  const { summary, runRecord } = useWorkspace();
  const objects = summary?.objects ?? [];
  const layers = (summary?.layers ?? []).filter((l) => objects.some((o) => o.layer === l.id && o.kind === 'solid'));
  const models = objects.filter((o) => o.kind === 'seam_model');
  const [flows, setFlows] = useState<{ id: string; name: string }[]>([]);
  useEffect(() => {
    let current = true;
    const all = objects.filter((o) => o.kind === 'dataflow');
    void Promise.all(all.map((o) => objectDetails(o.id).then((d) => (d.measures.kind === 'dataflow' && !d.measures.dataflow.rows ? { id: o.id, name: o.name } : null)))).then(
      (got) => current && setFlows(got.filter((x): x is { id: string; name: string } => !!x)),
    );
    return () => {
      current = false;
    };
  }, [objects.length]);
  const [name, setName] = useState(() => freeName('Reserves New case', names));
  const [layer, setLayer] = useState(layers[layers.length - 1]?.id ?? '');
  const [when, setWhen] = useState<When>({});
  const [model, setModel] = useState(models[0]?.id ?? '');
  const [flow, setFlow] = useState('');
  const solids = useMemo(() => objects.filter((o) => o.kind === 'solid' && o.layer === layer), [objects, layer]);
  const [attributes, setAttributes] = useState<Attributes[]>([]);
  useEffect(() => {
    let current = true;
    void objectsAttributes(solids.map((s) => s.id)).then((a) => current && setAttributes(a as Attributes[]));
    return () => {
      current = false;
    };
  }, [solids.map((s) => s.id).join(',')]);
  const picked = blocksFitting(solids, attributes, layer, when);
  const starter = flow === '' || flows.length === 0;
  const make = () =>
    onMake(async (made) => {
      if (picked.length === 0) throw new Error('No blocks on that layer fit the filter');
      if (!model) throw new Error('Pick the seam model the blocks were cut from');
      let dataflow = flow;
      if (starter) dataflow = (await dataflowStarter(model, freeName('Reserves', names))).dataflow;
      const id = made(await runRecord('reserves.run', { dataflow, blocks: picked, model, layer: 'reserves', name }), 'table');
      // The blocks kept as the layer and filter, so a block added to the layer is taken up by Run again.
      const now = await tableSetupView(id);
      const reply = await runRecord('table.setup', { table: id, setup: { ...now.setup, blocks: { layers: [layer], ...(Object.keys(when).length ? { filter: when } : {}) } } });
      if (reply.error) throw new Error(reply.error);
      onOpen(id);
      return `${name} made on ${picked.length} blocks${starter ? ', with the starter dataflow and its settings table' : ''}.`;
    });
  return (
    <div className="mt-1.5 flex flex-wrap items-center gap-2 rounded border border-border p-1.5" data-slot="new-case-form">
      <CellInput className="w-48 border-input" value={name} label="The new case's name" onChange={setName} />
      <span className="text-muted-foreground">on</span>
      <Select className="h-6 w-40" aria-label="The layer of blocks it runs on" value={layer} onChange={(e) => setLayer(e.target.value)}>
        {layers.map((l) => (
          <option key={l.id} value={l.id}>
            {l.name}
          </option>
        ))}
      </Select>
      <FilterInput className="w-48" rows={attributes} value={when} onChange={setWhen} label="Which blocks: a filter, none for all" />
      <span className="text-muted-foreground" data-slot="new-case-blocks">
        {picked.length} blocks
      </span>
      <Select className="h-6 w-36" aria-label="The seam model" value={model} onChange={(e) => setModel(e.target.value)}>
        {models.map((m) => (
          <option key={m.id} value={m.id}>
            {m.name}
          </option>
        ))}
      </Select>
      <Select className="h-6 w-44" aria-label="The reserves dataflow it runs" value={flow} onChange={(e) => setFlow(e.target.value)}>
        <option value="">The starter, made from the seam model</option>
        {flows.map((f) => (
          <option key={f.id} value={f.id}>
            {f.name}
          </option>
        ))}
      </Select>
      <Button size="sm" className="h-6 px-2" disabled={busy || picked.length === 0 || !name.trim()} onClick={make} data-slot="new-case-make">
        {busy ? 'Making…' : 'Make the case'}
      </Button>
    </div>
  );
}

/** Copy case: a name, and whether its settings tables are copied (else shared). */
function CopyCase({ busy, from, names, onMake }: { busy: boolean; from: CaseHeadline | null; names: string[]; onMake: (name: string, copyTables: boolean) => void }) {
  const [name, setName] = useState(() => freeName(`${from?.name ?? 'Reserves'} copy`, names));
  const [copyTables, setCopyTables] = useState(false);
  return (
    <div className="mt-1.5 flex flex-wrap items-center gap-2 rounded border border-border p-1.5" data-slot="copy-case-form">
      <span className="text-muted-foreground">{from?.name ?? 'The case'} as</span>
      <CellInput className="w-48 border-input" value={name} label="The copy's name" onChange={setName} />
      <label className="flex items-center gap-1">
        <Checkbox checked={copyTables} onCheckedChange={(v) => setCopyTables(v === true)} aria-label="Copy its settings tables" />
        Copy its settings tables (else the copy shares them)
      </label>
      <Button size="sm" className="h-6 px-2" disabled={busy || !name.trim() || !from} onClick={() => onMake(name.trim(), copyTables)} data-slot="copy-case-make">
        {busy ? 'Copying…' : 'Copy the case'}
      </Button>
    </div>
  );
}

/** Add a dataflow: a name, empty or a copy of a row dataflow, and the rows it runs on. */
function AddDataflow({ busy, names, onMake }: { busy: boolean; names: string[]; onMake: (name: string, from: string | null, range: When) => void }) {
  const { summary } = useWorkspace();
  const objects = summary?.objects ?? [];
  const [rowFlows, setRowFlows] = useState<{ id: string; name: string }[]>([]);
  useEffect(() => {
    let current = true;
    const all = objects.filter((o) => o.kind === 'dataflow');
    void Promise.all(all.map((o) => objectDetails(o.id).then((d) => (d.measures.kind === 'dataflow' && d.measures.dataflow.rows ? { id: o.id, name: o.name } : null)))).then(
      (got) => current && setRowFlows(got.filter((x): x is { id: string; name: string } => !!x)),
    );
    return () => {
      current = false;
    };
  }, [objects.length]);
  const [name, setName] = useState(() => freeName('Haulage', names));
  const [from, setFrom] = useState('');
  const [range, setRange] = useState<When>({});
  return (
    <div className="mt-1.5 flex flex-wrap items-center gap-2 rounded border border-border p-1.5" data-slot="add-dataflow-form">
      <CellInput className="w-40 border-input" value={name} label="The dataflow's name" onChange={setName} />
      <Select className="h-6 w-48" aria-label="Start it from" value={from} onChange={(e) => setFrom(e.target.value)}>
        <option value="">A first step to write over</option>
        {rowFlows.map((f) => (
          <option key={f.id} value={f.id}>
            A copy of {f.name}
          </option>
        ))}
      </Select>
      <span className="text-muted-foreground">on</span>
      <FilterInput className="w-44" rows={[]} value={range} onChange={setRange} label="The rows it runs on: a filter, none for every row" />
      <Button size="sm" className="h-6 px-2" disabled={busy || !name.trim()} onClick={() => onMake(freeName(name.trim(), names), from || null, range)} data-slot="add-dataflow-make">
        {busy ? 'Adding…' : 'Add after the case’s dataflows'}
      </Button>
    </div>
  );
}
