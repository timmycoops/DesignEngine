import { ArrowDown, ArrowUp, ChevronDown, ChevronRight, X } from 'lucide-react';
import { type ReactNode, useEffect, useState } from 'react';
import { type FieldSource, type SetupView, type TableSetup, tableSetupView } from '@/bridge';
import { CellInput, FilterInput, IconButton, Select } from '@/design-system/kit';
import { Button } from '@/design-system/ui/button';
import { cn } from 'cn';
import { useWorkspace } from '@/workspace/context';
import { countWords, foldsNames } from './reservesCase';

/**
 * A case's setup (Table setup plan, decision 5; in the Reserves case tab since the Reserves case plan's workstream 7,
 * which took the Setup tab out): what fills the case's reserves table, opened from the band's Setup. What it's run on (its
 * blocks by layer and filter, its seam model or stack), its dataflow and add-ons in order, the dataflows after it with the
 * rows each runs on, the steps they run, each field and where it comes from (the table's own marked), its parameter tables
 * in the order read with what it reads from each. Every edit is `table.setup`: the table's next revision, one undo. A
 * table made before setups shows the one its run gives, and takes it as its own on the first edit.
 *
 * Its own view (Case setup and unit plan, decisions 1 to 4): it takes the working area under the header, framed, with
 * Done back to the case; it has no Run of its own, an edit putting the case out of date for Run to run; it reads top
 * down in the order a run uses it, each section saying what it decides; a long list of names folds to a count.
 */
export function CaseSetup({ table, onDone }: { table: string; onDone: () => void }) {
  const { summary, runRecord, openTool } = useWorkspace();
  const [view, setView] = useState<SetupView | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [said, setSaid] = useState<{ text: string; refused: boolean } | null>(null);
  // Anything it names may change its view: the table, its dataflows, its parameter tables, a layer's blocks.
  const changed = (summary?.objects ?? []).map((o) => `${o.id}:${o.revision}`).join(',');
  useEffect(() => {
    let live = true;
    void tableSetupView(table)
      .then((v) => live && (setView(v), setError(null)))
      .catch((e) => live && (setView(null), setError(String(e))));
    return () => {
      live = false;
    };
  }, [table, changed]);

  const save = async (edit: (s: TableSetup) => void, what: string) => {
    if (!view) return;
    const next: TableSetup = structuredClone(view.setup);
    edit(next);
    const reply = await runRecord('table.setup', { table: view.table.id, setup: next });
    setSaid(
      reply.error
        ? { text: reply.error, refused: true }
        : { text: `${what}: saved as ${view.table.name}'s next revision. The case is out of date until it's run from Run.`, refused: false },
    );
  };

  return (
    <div className="flex min-h-[20rem] flex-1 flex-col text-xs" data-slot="setup">
      {/* The frame (decision 1): what this is, and the way back. */}
      <div className="flex shrink-0 flex-wrap items-center gap-x-3 gap-y-1 border-b border-border bg-muted/40 px-3 py-2" data-slot="setup-title">
        <span className="text-sm font-semibold">Setup of {view?.table.name ?? 'the case'}</span>
        <span className="text-muted-foreground">What fills this case. Each change saves as the table's next revision, and the case runs from Run.</span>
        <Button size="sm" className="ml-auto h-6 px-3" onClick={onDone} data-slot="setup-done" title="Back to the case, on the same step and block (Esc)">
          Done
        </Button>
      </div>
      {(view && !view.held) || said || error ? (
        <div className="shrink-0 space-y-0.5 border-b border-border px-3 py-1">
          {view && !view.held && <div className="text-warning">From the run that made it: saved as the table's own on the first change.</div>}
          {said && (
            <div className={said.refused ? 'text-destructive' : 'text-muted-foreground'} data-slot="setup-said">
              {said.text}
            </div>
          )}
          {error && <div className="text-destructive">{error}</div>}
        </div>
      ) : null}
      {view && (
        <div className="min-h-0 flex-1 overflow-auto">
          {view.problems.length > 0 && (
            <ul className="space-y-0.5 border-b px-3 py-2 text-destructive" data-slot="setup-problems">
              {view.problems.map((p) => (
                <li key={p}>{p}</li>
              ))}
            </ul>
          )}
          {/* Top down in the order a run uses it (decision 3). */}
          <div className="max-w-[70rem] space-y-5 p-3">
            <RunOn view={view} save={save} />
            <Dataflows view={view} save={save} />
            <RowDataflows view={view} save={save} />
            <ParameterTables view={view} save={save} open={(id) => openTool('params', `show:${id}`)} />
            <Steps view={view} edit={(flow, step) => openTool('params', `formula:${flow}:${step}`)} />
            <Fields view={view} save={save} />
          </div>
        </div>
      )}
    </div>
  );
}

/**
 * The dataflows after the reserves one (Reserves case plan, decisions 2 and 4): in run order, each with the rows it runs
 * on (a filter), moved up or down or taken out. Its columns stay on the table when it's taken out.
 */
function RowDataflows({ view, save }: { view: SetupView; save: Save }) {
  const { summary } = useWorkspace();
  const rows = view.setup.row_dataflows ?? [];
  const nameOf = (id: string) => (summary?.objects ?? []).find((o) => o.id === id)?.name ?? 'a dataflow since deleted';
  return (
    <Section n={3} title="Dataflows after it, in run order" line="Dataflows run on the table's rows after the reserves run, each on the rows its filter picks; taken out, its columns stay on the table." slot="setup-row-dataflows">
      {rows.length === 0 && <div className="text-muted-foreground">None: Add a dataflow, beside the cases, puts one here.</div>}
      <Ordered
        items={rows}
        label={(r) => nameOf(r.dataflow)}
        onChange={(next) => void save((s) => (s.row_dataflows = next), 'Dataflows after it')}
        render={(r, i) => (
          <span className="flex flex-wrap items-center gap-1.5">
            <span>{nameOf(r.dataflow)}</span>
            <span className="text-muted-foreground">on</span>
            <FilterInput
              className="w-44"
              rows={[]}
              value={r.range ?? {}}
              label={`The rows ${nameOf(r.dataflow)} runs on: a filter, none for every row`}
              onChange={(range) =>
                void save((s) => {
                  const list = s.row_dataflows ?? [];
                  list[i] = { ...list[i], range: Object.keys(range).length ? range : undefined };
                  s.row_dataflows = list;
                }, `${nameOf(r.dataflow)}'s rows`)
              }
            />
          </span>
        )}
      />
    </Section>
  );
}

type Save = (edit: (s: TableSetup) => void, what: string) => Promise<void>;

/** A section of the setup: numbered in the order a run uses it, with a line saying what it decides (decision 3). */
function Section({ n, title, line, children, slot }: { n: number; title: string; line: string; children: ReactNode; slot: string }) {
  return (
    <section className="min-w-0 space-y-1" data-slot={slot} data-section={n}>
      <div className="flex items-baseline gap-2 border-b border-border/60 pb-0.5">
        <span className="w-4 shrink-0 text-right font-semibold text-muted-foreground tabular-nums">{n}</span>
        <span className="text-[13px] font-semibold">{title}</span>
      </div>
      <div className="pl-6 text-muted-foreground" data-slot="setup-line">
        {line}
      </div>
      <div className="space-y-1 pl-6">{children}</div>
    </section>
  );
}

/** Names, folded to a count past twelve and opened on a click (decision 4). */
function Names({ names, noun, lead }: { names: string[]; noun: string; lead: string }) {
  const [open, setOpen] = useState(false);
  if (!foldsNames(names.length)) return <span>{`${lead} ${names.join(', ')}`}</span>;
  return (
    <span data-slot="setup-names" data-count={names.length}>
      <button type="button" className="inline-flex items-center gap-0.5 hover:text-foreground" onClick={() => setOpen((o) => !o)} aria-expanded={open}>
        {open ? <ChevronDown className="size-3" /> : <ChevronRight className="size-3" />}
        {lead} {countWords(names.length, noun)}
      </button>
      {open && (
        <span className="mt-0.5 block columns-[12rem] gap-x-4" data-slot="setup-names-open">
          {names.map((n) => (
            <span key={n} className="block truncate font-mono text-[11px]" title={n}>
              {n}
            </span>
          ))}
        </span>
      )}
    </span>
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
  // Tables it may read in place of the blocks: any of the project's, but itself and the Objects table's own.
  const tables = (summary?.objects ?? []).filter((o) => o.kind === 'table' && !o.objects_own && o.id !== view.table.id);
  const b = view.blocks;
  const byLayers = b.named === 0;
  const filter = Object.entries(b.filter);
  const [newAttribute, setNewAttribute] = useState('');
  const fromTable = !!view.setup.input;
  return (
    <Section n={1} title="Blocks it runs on" line="Which block solids fill the case, and the seam order they're read in. A block added to a picked layer is taken up at the next run." slot="setup-run-on">
      <div className="flex items-center gap-2">
        <Select
          className="h-6 w-56"
          aria-label="What the table is run on"
          value={fromTable ? 'table' : 'blocks'}
          onChange={(e) =>
            void save((s) => {
              if (e.target.value === 'blocks') delete s.input;
              else if (tables[0]) s.input = { table: tables[0].id, map: { volume: 'volume' } };
            }, e.target.value === 'blocks' ? 'Run on block solids' : 'Run on a table')
          }
        >
          <option value="blocks">Block solids, volumes measured</option>
          {tables.length > 0 && <option value="table">A table, volumes imported</option>}
        </Select>
      </div>
      {fromTable && view.input ? (
        <InputMapping view={view} tables={tables} save={save} />
      ) : byLayers ? (
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
      {!fromTable && (
        <div className={cn(b.picked === 0 && 'text-destructive')} data-slot="setup-picked">
          Picks {b.picked} block{b.picked === 1 ? '' : 's'} now.
        </div>
      )}
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

/**
 * A table read in place of the blocks (Tables and scripts plan, decision 12): a row a piece. Which column is the volume
 * (always), the thickness or plan area, and each attribute the run reads; one left as its own name reads the column of
 * that name. With no solids behind it, its volumes are imported, not of record, and the run and report say so.
 */
function InputMapping({ view, tables, save }: { view: SetupView; tables: { id: string; name: string }[]; save: Save }) {
  const input = view.setup.input!;
  const shown = view.input!;
  const numbers = shown.columns.filter(([, n]) => n).map(([c]) => c);
  const all = shown.columns.map(([c]) => c);
  const setMap = (edit: (m: NonNullable<TableSetup['input']>['map']) => void, what: string) =>
    void save((s) => {
      if (s.input) edit(s.input.map);
    }, what);
  const pick = (label: string, value: string | undefined, choices: string[], onPick: (v: string) => void, blank: string) => (
    <Select className="h-6 w-44" aria-label={label} value={value ?? ''} onChange={(e) => onPick(e.target.value)}>
      <option value="">{blank}</option>
      {choices.map((c) => (
        <option key={c} value={c}>
          {c}
        </option>
      ))}
    </Select>
  );
  const row = (label: string, control: ReactNode) => (
    <div className="flex items-center gap-2" data-map={label}>
      <span className="w-28 shrink-0 truncate text-muted-foreground" title={label}>
        {label}
      </span>
      {control}
    </div>
  );
  return (
    <div className="space-y-0.5" data-slot="setup-input">
      {row(
        'table',
        <Select className="h-6 w-44" aria-label="The table read" value={input.table} onChange={(e) => void save((s) => s.input && (s.input.table = e.target.value), 'Table read')}>
          {tables.map((t) => (
            <option key={t.id} value={t.id}>
              {t.name}
            </option>
          ))}
        </Select>,
      )}
      <div className="text-warning">
        {shown.rows} rows, a row a piece: volumes imported from {shown.table.name}, not measured from solids.
      </div>
      {row('volume', pick('The volume column', input.map.volume, numbers, (v) => setMap((m) => (m.volume = v), 'Volume column'), 'pick a column'))}
      {row('thickness', pick('The thickness column', input.map.thickness, numbers, (v) => setMap((m) => (v ? (m.thickness = v) : delete m.thickness), 'Thickness column'), 'volume over area'))}
      {row('plan area', pick('The plan area column', input.map.area, numbers, (v) => setMap((m) => (v ? (m.area = v) : delete m.area), 'Plan area column'), 'volume over thickness'))}
      {row('horizon order', pick('The order column', input.map.order, numbers, (v) => setMap((m) => (v ? (m.order = v) : delete m.order), 'Order column'), 'the model or stack'))}
      <div className="pt-1 text-muted-foreground">What the run reads, from which column:</div>
      {shown.reads.map((attribute) =>
        row(
          attribute,
          pick(
            `The column ${attribute} is read from`,
            input.map.columns?.[attribute],
            all,
            (v) =>
              setMap((m) => {
                const next = { ...(m.columns ?? {}) };
                if (v) next[attribute] = v;
                else delete next[attribute];
                m.columns = next;
              }, `${attribute} column`),
            all.some((c) => c.toLowerCase() === attribute.toLowerCase()) ? `its own: ${attribute}` : 'none in the table',
          ),
        ),
      )}
    </div>
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
    <Section n={2} title="The dataflow and its add-ons" line="The steps that work each block column: the main dataflow's, then each add-on's at its stage." slot="setup-dataflows">
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
        {t.read.length ? <Names names={t.read} noun="setting" lead="Reads" /> : 'Nothing read from it here'}
        {t.unread.length > 0 && (
          <span className="text-warning">
            {' · '}
            <Names names={t.unread} noun="column" lead="not named here:" />
          </span>
        )}
        {t.shadowed.length > 0 && <span> · given first by an earlier table: {t.shadowed.map(([c, by]) => `${c} (${by})`).join(', ')}</span>}
      </div>
    </div>
  );
  return (
    <Section n={4} title="Settings tables, in the order read" line="Where the steps' settings come from: each value from the first table that gives it. A name opens its table in the Parameters tab." slot="setup-tables">
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
          <Names names={view.looked_up.map((t) => t.name)} noun="table" lead="Looked up by name in its scripts:" />
        </div>
      )}
    </Section>
  );
}

function Steps({ view, edit }: { view: SetupView; edit: (flow: string, step: string) => void }) {
  const flowId = (from: string) => (from === view.dataflow.name ? view.dataflow.id : view.add_ons.find((a) => a.name === from)?.id);
  const scripts = view.steps.filter((s) => s.kind === 'table script');
  return (
    <Section n={5} title="Steps, in the order run" line="Every step the case runs, from the dataflow and its add-ons. A formula opens in the Parameters tab to edit." slot="setup-steps">
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
          {view.calculated.length > 0 && <Names names={view.calculated} noun="column" lead="Calculated columns:" />}
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
  const [all, setAll] = useState(false);
  const own = view.fields.filter((f) => f.own).length;
  // Past twelve, the table's own fields and those found show; the rest on a click (decision 4).
  const folded = !find && !all && foldsNames(view.fields.length);
  const shown = view.fields.filter((f) => (find ? f.name.toLowerCase().includes(find.toLowerCase()) : !folded || f.own));
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
    <Section n={6} title="Fields and where each comes from" line="Each column of the table and what fills it: the solid, a block attribute, a parameter column or a step. A change makes it the table's own." slot="setup-fields">
      <div className="flex items-center gap-2">
        <CellInput className="w-48 border-input" value={find} label="Find a field" placeholder="Find a field…" onChange={setFind} />
        <span className="text-muted-foreground">
          {view.fields.length} fields, {own} the table's own.
        </span>
        {!find && foldsNames(view.fields.length) && (
          <button type="button" className="inline-flex items-center gap-0.5 text-muted-foreground hover:text-foreground" onClick={() => setAll((a) => !a)} aria-expanded={all} data-slot="setup-fields-all">
            {all ? <ChevronDown className="size-3" /> : <ChevronRight className="size-3" />}
            {all ? "Only the table's own" : `Every field (${view.fields.length})`}
          </button>
        )}
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
