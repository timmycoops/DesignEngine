import { ArrowDown, ArrowUp, RotateCw, X } from 'lucide-react';
import { type ReactNode, useEffect, useMemo, useState } from 'react';
import { type FieldSource, reservesRerunParams, type SetupView, type TableSetup, tableSetupView } from '@/bridge';
import { CellInput, IconButton, PanelHead, Select } from '@/design-system/kit';
import { Button } from '@/design-system/ui/button';
import { cn } from 'cn';
import { useWorkspace } from '@/workspace/context';

/**
 * The Setup tab (Table setup plan, decision 5): what fills a reserves table, on one page. What it's run on (its blocks
 * by layer and filter, its seam model or stack), its dataflow and add-ons in order, the steps they run, each field and
 * where it comes from (the table's own marked), its parameter tables in the order read with what it reads from each,
 * its table scripts and calculated columns, and Run again. Every edit is `table.setup`: the table's next revision, one
 * undo. A table made before setups shows the one its run gives, and takes it as its own on the first edit.
 */
export function SetupPanel() {
  const { summary, selection, runRecord, openTool } = useWorkspace();
  const tables = useMemo(() => (summary?.objects ?? []).filter((o) => o.kind === 'table' && !o.objects_own), [summary]);
  const [pick, setPick] = useState<string | null>(null);
  const shown = tables.some((t) => t.id === pick) ? pick : (tables[tables.length - 1]?.id ?? null);
  const last = selection[selection.length - 1] ?? null;
  useEffect(() => {
    if (last && tables.some((t) => t.id === last)) setPick(last);
  }, [last, tables]);

  const [view, setView] = useState<SetupView | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [said, setSaid] = useState<{ text: string; refused: boolean } | null>(null);
  const [running, setRunning] = useState(false);
  // Anything it names may change its view: the table, its dataflows, its parameter tables, a layer's blocks.
  const changed = (summary?.objects ?? []).map((o) => `${o.id}:${o.revision}`).join(',');
  useEffect(() => {
    let live = true;
    if (!shown) return setView(null);
    void tableSetupView(shown)
      .then((v) => live && (setView(v), setError(null)))
      .catch((e) => live && (setView(null), setError(String(e))));
    return () => {
      live = false;
    };
  }, [shown, changed]);

  const save = async (edit: (s: TableSetup) => void, what: string) => {
    if (!view) return;
    const next: TableSetup = structuredClone(view.setup);
    edit(next);
    const reply = await runRecord('table.setup', { table: view.table.id, setup: next });
    setSaid(reply.error ? { text: reply.error, refused: true } : { text: `${what}: saved as ${view.table.name}'s next revision.`, refused: false });
  };
  const runAgain = async () => {
    if (!view) return;
    setRunning(true);
    try {
      const reply = await runRecord('reserves.run', await reservesRerunParams(view.table.id, []));
      setSaid(reply.error ? { text: reply.error, refused: true } : { text: `${view.table.name} run again from its setup.`, refused: false });
    } catch (e) {
      setSaid({ text: String(e), refused: true });
    } finally {
      setRunning(false);
    }
  };

  const head = <PanelHead line="What fills a reserves table: its blocks, dataflows, field sources and parameter tables; each edit its next revision" more="Run again runs the table from what's here. A table made before setups shows the one its run gives, and takes it as its own on the first edit." />;
  if (!shown) {
    return (
      <div className="flex h-full flex-col text-xs">
        {head}
        <p className="p-3 text-muted-foreground">No reserves tables yet: run a dataflow on blocks (Reserves, or a workflow) to make one, then set it up here.</p>
      </div>
    );
  }
  return (
    <div className="flex h-full min-h-0 flex-col text-xs" data-slot="setup">
      {head}
      <div className="flex h-8 shrink-0 items-center gap-2 border-b px-3">
        <Select className="h-6 w-56" aria-label="Table" data-slot="setup-table" value={shown} onChange={(e) => setPick(e.target.value)}>
          {tables.map((t) => (
            <option key={t.id} value={t.id}>
              {t.name}
            </option>
          ))}
        </Select>
        <Button size="sm" variant="outline" className="h-6 px-2 text-xs" disabled={running || !view} onClick={() => void runAgain()} title="Run every block of the table again from this setup, into the table: its next revision, one undo">
          <RotateCw className={cn('size-3.5', running && 'animate-spin')} /> {running ? 'Running…' : 'Run again'}
        </Button>
        {view && !view.held && <span className="text-warning">From the run that made it: saved as its own on the first edit.</span>}
        {said && (
          <span className={cn('min-w-0 truncate', said.refused ? 'text-destructive' : 'text-muted-foreground')} title={said.text} data-slot="setup-said">
            {said.text}
          </span>
        )}
      </div>
      {error && <p className="p-3 text-destructive">{error}</p>}
      {view && (
        <div className="min-h-0 flex-1 overflow-auto">
          {view.problems.length > 0 && (
            <ul className="space-y-0.5 border-b px-3 py-2 text-destructive" data-slot="setup-problems">
              {view.problems.map((p) => (
                <li key={p}>{p}</li>
              ))}
            </ul>
          )}
          <div className="grid gap-x-6 gap-y-3 p-3 lg:grid-cols-2">
            <RunOn view={view} save={save} />
            <Dataflows view={view} save={save} />
            <ParameterTables view={view} save={save} open={(id) => openTool('params', `show:${id}`)} />
            <Steps view={view} edit={(flow, step) => openTool('params', `formula:${flow}:${step}`)} />
            <Fields view={view} save={save} />
          </div>
        </div>
      )}
    </div>
  );
}

type Save = (edit: (s: TableSetup) => void, what: string) => Promise<void>;

function Section({ title, children, slot, wide }: { title: string; children: ReactNode; slot: string; wide?: boolean }) {
  return (
    <section className={cn('min-w-0 space-y-1', wide && 'lg:col-span-2')} data-slot={slot}>
      <div className="font-semibold">{title}</div>
      {children}
    </section>
  );
}

/** A box that saves on Enter or as it's left, not at every key. */
function CommitInput({ value, label, onCommit, className }: { value: string; label: string; onCommit: (v: string) => void; className?: string }) {
  const [text, setText] = useState(value);
  useEffect(() => setText(value), [value]);
  const commit = () => text !== value && onCommit(text);
  return <CellInput className={cn('border-input', className)} value={text} label={label} onChange={setText} onBlur={commit} onKeyDown={(e) => e.key === 'Enter' && commit()} />;
}

/** A list in order, each moved up or down or taken out. */
function Ordered<T>({ items, label, render, onChange }: { items: T[]; label: (t: T) => string; render: (t: T, i: number) => ReactNode; onChange?: (next: T[]) => void }) {
  const move = (i: number, by: number) => {
    const next = items.slice();
    [next[i], next[i + by]] = [next[i + by], next[i]];
    onChange?.(next);
  };
  return (
    <ol className="space-y-0.5">
      {items.map((t, i) => (
        <li key={label(t) + i} className="flex items-start gap-1" data-item={label(t)}>
          <span className="w-4 shrink-0 text-right text-muted-foreground tabular-nums">{i + 1}</span>
          <span className="min-w-0 flex-1">{render(t, i)}</span>
          {onChange && (
            <span className="flex shrink-0">
              <IconButton label={`Move ${label(t)} up`} disabled={i === 0} onClick={() => move(i, -1)}>
                <ArrowUp className="size-3" />
              </IconButton>
              <IconButton label={`Move ${label(t)} down`} disabled={i === items.length - 1} onClick={() => move(i, 1)}>
                <ArrowDown className="size-3" />
              </IconButton>
              <IconButton label={`Take ${label(t)} out`} onClick={() => onChange(items.filter((_, k) => k !== i))}>
                <X className="size-3" />
              </IconButton>
            </span>
          )}
        </li>
      ))}
    </ol>
  );
}

/** A list to add one more from: the choices not there already. */
function AddFrom({ label, choices, onAdd }: { label: string; choices: { id: string; name: string }[]; onAdd: (id: string) => void }) {
  if (choices.length === 0) return null;
  return (
    <Select className="h-6 w-56" aria-label={label} value="" onChange={(e) => e.target.value && onAdd(e.target.value)}>
      <option value="">{label}</option>
      {choices.map((c) => (
        <option key={c.id} value={c.id}>
          {c.name}
        </option>
      ))}
    </Select>
  );
}

function RunOn({ view, save }: { view: SetupView; save: Save }) {
  const { summary } = useWorkspace();
  const layers = summary?.layers ?? [];
  const models = (summary?.objects ?? []).filter((o) => o.kind === 'seam_model');
  const b = view.blocks;
  const byLayers = b.named === 0;
  const filter = Object.entries(b.filter);
  const [newAttribute, setNewAttribute] = useState('');
  return (
    <Section title="Run on" slot="setup-run-on">
      {byLayers ? (
        <>
          <Ordered
            items={b.layers}
            label={([, name]) => name}
            render={([, name]) => name}
            onChange={(next) => void save((s) => (s.blocks.layers = next.map(([id]) => id)), 'Layers')}
          />
          <AddFrom label="+ Add a layer" choices={layers.filter((l) => !b.layers.some(([id]) => id === l.id))} onAdd={(id) => void save((s) => (s.blocks.layers = [...(s.blocks.layers ?? []), id]), 'Layer added')} />
          <div className="text-muted-foreground">Solids on these layers that fit the filter, a block added to a layer taken up at the next run:</div>
          {filter.map(([attribute, words]) => (
            <div key={attribute} className="flex items-center gap-1" data-filter={attribute}>
              <span className="w-24 shrink-0 truncate" title={attribute}>
                {attribute}
              </span>
              <CommitInput
                value={words}
                label={`Filter on ${attribute}`}
                onCommit={(v) =>
                  void save((s) => {
                    const f = { ...(s.blocks.filter ?? {}) };
                    if (v.trim()) f[attribute] = v;
                    else delete f[attribute];
                    s.blocks.filter = f;
                  }, `Filter on ${attribute}`)
                }
              />
              <IconButton
                label={`Take the filter on ${attribute} out`}
                onClick={() =>
                  void save((s) => {
                    const f = { ...(s.blocks.filter ?? {}) };
                    delete f[attribute];
                    s.blocks.filter = f;
                  }, 'Filter taken out')
                }
              >
                <X className="size-3" />
              </IconButton>
            </div>
          ))}
          <div className="flex items-center gap-1">
            <CellInput className="w-24 border-input" value={newAttribute} label="Attribute to filter on" placeholder="+ filter on…" onChange={setNewAttribute} />
            <CommitInput
              value=""
              label="Its words, as the bench rules take them"
              className="flex-1"
              onCommit={(v) => {
                const a = newAttribute.trim();
                if (!a || !v.trim()) return;
                void save((s) => (s.blocks.filter = { ...(s.blocks.filter ?? {}), [a]: v }), `Filter on ${a}`).then(() => setNewAttribute(''));
              }}
            />
          </div>
        </>
      ) : (
        <div>
          The {b.named} blocks named.{' '}
          <button
            type="button"
            className="text-primary hover:underline"
            title="Take every solid on these blocks' layers, so a block added to one is taken up"
            onClick={() =>
              void save((s) => {
                const ids = s.blocks.ids ?? [];
                const on = [...new Set((summary?.objects ?? []).filter((o) => ids.includes(o.id)).map((o) => o.layer))];
                s.blocks = { layers: on };
              }, 'Picked by their layers')
            }
          >
            Pick by their layers instead
          </button>
        </div>
      )}
      <div className={cn(b.picked === 0 && 'text-destructive')} data-slot="setup-picked">
        Picks {b.picked} block{b.picked === 1 ? '' : 's'} now.
      </div>
      <div className="flex items-center gap-2">
        <span className="text-muted-foreground">Ordered by</span>
        <Select
          className="h-6 w-56"
          aria-label="Seam model"
          value={view.model?.id ?? ''}
          onChange={(e) =>
            void save((s) => {
              if (e.target.value) s.model = e.target.value;
              else delete s.model;
            }, 'Seam model')
          }
        >
          {view.stack.length > 0 && <option value="">the stack: {view.stack.join(', ')}</option>}
          {models.map((m) => (
            <option key={m.id} value={m.id}>
              {m.name}
            </option>
          ))}
        </Select>
      </div>
      {view.own_rows > 0 && <div className="text-muted-foreground">{view.own_rows} rows of its own, read before every table.</div>}
    </Section>
  );
}

function Dataflows({ view, save }: { view: SetupView; save: Save }) {
  const ownNote = view.own_add_ons ? (
    <button type="button" className="text-primary hover:underline" onClick={() => void save((s) => delete s.add_ons, "The dataflow's add-ons")}>
      Use the dataflow's add-ons
    </button>
  ) : (
    <span className="text-muted-foreground">The dataflow's add-ons; a change makes them the table's own.</span>
  );
  const setAddOns = (next: string[], what: string) => void save((s) => (s.add_ons = next), what);
  return (
    <Section title="Dataflows" slot="setup-dataflows">
      <div className="flex items-center gap-2">
        <span className="text-muted-foreground">Main</span>
        <Select className="h-6 w-56" aria-label="Main dataflow" value={view.dataflow.id} onChange={(e) => void save((s) => (s.dataflow = e.target.value), 'Dataflow')}>
          {view.mains.map((m) => (
            <option key={m.id} value={m.id}>
              {m.name}
            </option>
          ))}
        </Select>
        <span className="text-muted-foreground">r{view.dataflow.revision}</span>
      </div>
      <div className="text-muted-foreground">Add-ons, run in turn at their stage:</div>
      {view.add_ons.length === 0 && <div className="text-muted-foreground">None.</div>}
      <Ordered
        items={view.add_ons}
        label={(a) => a.name}
        render={(a) => (
          <span className={cn(view.own_add_ons && 'text-edited', a.missing && 'text-destructive')}>
            {a.name} <span className="text-muted-foreground">r{a.revision}</span>
          </span>
        )}
        onChange={(next) => setAddOns(next.map((a) => a.id), 'Add-ons')}
      />
      <div className="flex flex-wrap items-center gap-2">
        <AddFrom label="+ Add an add-on" choices={view.add_on_choices.filter((c) => !view.add_ons.some((a) => a.id === c.id))} onAdd={(id) => setAddOns([...view.add_ons.map((a) => a.id), id], 'Add-on added')} />
        {ownNote}
      </div>
    </Section>
  );
}

function ParameterTables({ view, save, open }: { view: SetupView; save: Save; open: (id: string) => void }) {
  const { summary } = useWorkspace();
  const all = (summary?.objects ?? []).filter((o) => o.kind === 'parameter_table');
  // The table's own list (or the dataflow's) can be edited; its add-ons' tables come after, as their add-on gives them.
  const listed = view.tables.filter((t) => t.from === 'table' || t.from === 'dataflow');
  const after = view.tables.filter((t) => t.from !== 'table' && t.from !== 'dataflow');
  const setTables = (next: string[], what: string) => void save((s) => (s.tables = next), what);
  const row = (t: SetupView['tables'][number]) => (
    <div className="min-w-0">
      <button type="button" className={cn('hover:underline', view.own_tables && t.from === 'table' && 'text-edited', t.missing && 'text-destructive')} title={`Open ${t.name} in the Parameters tab`} onClick={() => open(t.id)}>
        {t.name}
      </button>{' '}
      <span className="text-muted-foreground">r{t.revision}</span>
      <div className="text-muted-foreground" data-slot="setup-table-reads">
        {t.read.length ? `Reads ${t.read.join(', ')}` : 'Nothing read from it here'}
        {t.unread.length > 0 && <span className="text-warning"> · not named here: {t.unread.join(', ')}</span>}
        {t.shadowed.length > 0 && <span> · given first by an earlier table: {t.shadowed.map(([c, by]) => `${c} (${by})`).join(', ')}</span>}
      </div>
    </div>
  );
  return (
    <Section title="Parameter tables" slot="setup-tables" wide>
      <div className="text-muted-foreground">Read in this order, each value from the first that gives it{view.own_rows > 0 ? ', after the rows of its own' : ''}:</div>
      <Ordered items={listed} label={(t) => t.name} render={row} onChange={(next) => setTables(next.map((t) => t.id), 'Parameter tables')} />
      {after.length > 0 && (
        <div className="pl-5" data-slot="setup-add-on-tables">
          {after.map((t) => (
            <div key={t.id} className="flex gap-1">
              <span className="shrink-0 text-muted-foreground">{t.from}:</span>
              {row(t)}
            </div>
          ))}
        </div>
      )}
      <div className="flex flex-wrap items-center gap-2">
        <AddFrom label="+ Add a table" choices={all.filter((o) => !listed.some((t) => t.id === o.id))} onAdd={(id) => setTables([...listed.map((t) => t.id), id], 'Table added')} />
        {view.own_tables ? (
          <button type="button" className="text-primary hover:underline" onClick={() => void save((s) => delete s.tables, "The dataflow's tables")}>
            Use the dataflow's tables
          </button>
        ) : (
          <span className="text-muted-foreground">The dataflow's tables; a change makes them the table's own.</span>
        )}
      </div>
      {view.looked_up.length > 0 && (
        <div className="text-muted-foreground" data-slot="setup-looked-up">
          Looked up by name in its scripts: {view.looked_up.map((t) => t.name).join(', ')}.
        </div>
      )}
    </Section>
  );
}

function Steps({ view, edit }: { view: SetupView; edit: (flow: string, step: string) => void }) {
  const flowId = (from: string) => (from === view.dataflow.name ? view.dataflow.id : view.add_ons.find((a) => a.name === from)?.id);
  const scripts = view.steps.filter((s) => s.kind === 'table script');
  return (
    <Section title="Steps, in the order run" slot="setup-steps">
      <ol className="space-y-0.5">
        {view.steps.map((s, i) => {
          const id = flowId(s.from);
          const editable = (s.kind === 'formula' || s.kind === 'allocation') && id;
          return (
            <li key={s.step} className={cn('flex gap-1', !s.on && 'text-muted-foreground line-through')} data-step={s.step}>
              <span className="w-4 shrink-0 text-right text-muted-foreground tabular-nums">{i + 1}</span>
              {editable ? (
                <button type="button" className="hover:underline" title={`Edit ${s.step} in the Parameters tab`} onClick={() => edit(id, s.step)}>
                  {s.step}
                </button>
              ) : (
                <span>{s.step}</span>
              )}
              <span className="text-muted-foreground">
                · {s.kind}
                {s.from !== view.dataflow.name && ` · ${s.from}`}
              </span>
            </li>
          );
        })}
      </ol>
      {(scripts.length > 0 || view.calculated.length > 0) && (
        <div className="text-muted-foreground" data-slot="setup-scripts">
          {scripts.length > 0 && `Table scripts after final: ${scripts.map((s) => s.step).join(', ')}. `}
          {view.calculated.length > 0 && `Calculated columns: ${view.calculated.join(', ')}.`}
        </div>
      )}
    </Section>
  );
}

const KINDS: { from: FieldSource['from']; words: string }[] = [
  { from: 'volume', words: "the solid's volume" },
  { from: 'plan_area', words: "the solid's plan area" },
  { from: 'thickness', words: "the solid's thickness" },
  { from: 'attribute', words: 'a block attribute' },
  { from: 'parameter', words: 'a parameter column' },
  { from: 'worked', words: 'worked by a step' },
];

/** Where a field comes from, in words. */
export function sourceWords(s: FieldSource): string {
  if (s.from === 'attribute') return `block attribute ${s.attribute}`;
  if (s.from === 'parameter') return `parameter column ${s.column}`;
  return KINDS.find((k) => k.from === s.from)?.words ?? s.from;
}

function Fields({ view, save }: { view: SetupView; save: Save }) {
  const [find, setFind] = useState('');
  const shown = view.fields.filter((f) => !find || f.name.toLowerCase().includes(find.toLowerCase()));
  const own = view.fields.filter((f) => f.own).length;
  const set = (name: string, source: FieldSource | null) =>
    void save((s) => {
      const next = { ...(s.sources ?? {}) };
      if (source) next[name] = source;
      else delete next[name];
      s.sources = next;
    }, `${name}'s source`);
  const kindPicked = (name: string, from: string, was: FieldSource) => {
    if (from === '') return set(name, null);
    const k = from as FieldSource['from'];
    if (k === 'attribute') return set(name, { from: k, attribute: was.from === 'attribute' ? was.attribute : name });
    if (k === 'parameter') return set(name, { from: k, column: was.from === 'parameter' ? was.column : name });
    set(name, { from: k } as FieldSource);
  };
  return (
    <Section title="Fields and where each comes from" slot="setup-fields" wide>
      <div className="flex items-center gap-2">
        <CellInput className="w-48 border-input" value={find} label="Find a field" placeholder="Find a field…" onChange={setFind} />
        <span className="text-muted-foreground">
          {view.fields.length} fields, {own} the table's own.
        </span>
      </div>
      <div className="grid grid-cols-[minmax(8rem,auto)_minmax(10rem,1fr)_auto_minmax(8rem,1fr)] items-center gap-x-2 gap-y-0.5">
        {shown.map((f) => {
          const source = f.own ?? f.source;
          return (
            <div key={f.name} className="contents" data-field={f.name}>
              <span className={cn('truncate', f.own && 'text-edited')} title={`${f.name}${f.words ? ' (words)' : ''}, from ${f.from}`}>
                {f.name}
                {f.words && <span className="text-muted-foreground"> · words</span>}
              </span>
              <span className="truncate text-muted-foreground" title={f.own ? `The dataflow's: ${sourceWords(f.source)}` : f.from}>
                {f.own ? `in place of ${sourceWords(f.source)}` : `${sourceWords(f.source)}${f.from !== view.dataflow.name ? ` · ${f.from}` : ''}`}
              </span>
              <Select className="h-6 w-40" aria-label={`Where ${f.name} comes from`} value={f.own ? f.own.from : ''} onChange={(e) => kindPicked(f.name, e.target.value, source)}>
                <option value="">the dataflow's</option>
                {KINDS.filter((k) => !f.words || k.from === 'attribute' || k.from === 'parameter' || k.from === 'worked').map((k) => (
                  <option key={k.from} value={k.from}>
                    {k.words}
                  </option>
                ))}
              </Select>
              {f.own && (f.own.from === 'attribute' || f.own.from === 'parameter') ? (
                <CommitInput
                  value={f.own.from === 'attribute' ? f.own.attribute : f.own.column}
                  label={`${f.name}'s ${f.own.from === 'attribute' ? 'attribute' : 'parameter column'}`}
                  onCommit={(v) => v.trim() && set(f.name, f.own!.from === 'attribute' ? { from: 'attribute', attribute: v.trim() } : { from: 'parameter', column: v.trim() })}
                />
              ) : (
                <span />
              )}
            </div>
          );
        })}
      </div>
    </Section>
  );
}
