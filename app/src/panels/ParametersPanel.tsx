import { ArrowDown, ArrowLeft, ArrowRight, ArrowUp, Copy, CopyPlus, Plus, Save, Trash2, Undo2, X } from 'lucide-react';
import { type ClipboardEvent, type KeyboardEvent, useEffect, useMemo, useState } from 'react';
import { type Dataflow, dataflowCheck, objectDetails, type ObjectSummary, type ParameterTable, type ParametersPicked, parametersCheck, parametersPicked } from '@/bridge';
import { CellInput, Choice, IconButton, Select, StatusBadge } from '@/design-system/kit';
import { Button } from '@/design-system/ui/button';
import { cn } from 'cn';
import { useWorkspace } from '@/workspace/context';
import {
  ADD_UP_LABELS,
  ADD_UPS,
  type AddUpChoice,
  addColumn,
  cellEdited,
  columnEdited,
  copyText,
  deleteColumn,
  deleteRows,
  editColumn,
  FROM_LABELS,
  type FromChoice,
  FROMS,
  fromGrid,
  type Grid,
  gridEdited,
  insertRow,
  KINDS,
  moveColumn,
  moved,
  moveRow,
  paste,
  parseClipboard,
  setCell,
  toGrid,
  withAddUp,
  withFrom,
} from './parametersGrid';

/**
 * The Parameters tab (decision 0075, workstream 5d of 0070): the project's parameter tables and dataflows edited as grids.
 * A parameter table's filters then its values, rows read top down; cells typed or pasted from Excel; edits held, in blue,
 * until Save writes the next revision (`parameters.save`, `dataflow.save`), one undo. A solid picked in the view marks the
 * rows it fits and, in gold, the cell each value comes from. Saving never reruns the reserves: a new answer is a new run.
 */

const isEditable = (o: ObjectSummary) => o.kind === 'parameter_table' || o.kind === 'dataflow';

export function Parameters() {
  const { summary, selection } = useWorkspace();
  const objects = useMemo(() => (summary?.objects ?? []).filter(isEditable), [summary]);
  const [id, setId] = useState<string | null>(null);
  const last = selection[selection.length - 1] ?? null;
  // A table or dataflow picked anywhere is the one shown.
  useEffect(() => {
    if (last && objects.some((o) => o.id === last)) setId(last);
  }, [last, objects]);
  const object = objects.find((o) => o.id === id) ?? objects.find((o) => o.kind === 'parameter_table') ?? objects[0] ?? null;

  if (!object) {
    return <div className="p-3 text-xs text-muted-foreground">No parameter tables or dataflows in this project yet. They're made from Python (parameters.save, dataflow.save, or the starter dataflow) and edited here.</div>;
  }
  const tables = objects.filter((o) => o.kind === 'parameter_table');
  const flows = objects.filter((o) => o.kind === 'dataflow');
  const item = (o: ObjectSummary) => (
    <button
      key={o.id}
      type="button"
      data-slot="parameters-item"
      data-name={o.name}
      className={cn('block w-full truncate px-3 py-0.5 text-left hover:bg-accent', o.id === object.id && 'bg-selection text-primary')}
      onClick={() => setId(o.id)}
      title={`${o.name}, revision ${o.revision}`}
    >
      {o.name}
    </button>
  );
  return (
    <div className="flex h-full min-h-0 text-xs" data-tool-panel="parameters">
      <div className="w-52 shrink-0 overflow-auto border-r border-border py-1">
        <div className="px-2 py-0.5 font-semibold text-muted-foreground">Parameter tables</div>
        {tables.length ? tables.map(item) : <div className="px-3 text-muted-foreground">none</div>}
        <div className="mt-2 px-2 py-0.5 font-semibold text-muted-foreground">Dataflows</div>
        {flows.length ? flows.map(item) : <div className="px-3 text-muted-foreground">none</div>}
      </div>
      <div className="flex min-w-0 flex-1 flex-col">
        {object.kind === 'parameter_table' ? (
          <TableEditor key={`${object.id}@${object.revision}`} object={object} onMade={setId} />
        ) : (
          <DataflowEditor key={`${object.id}@${object.revision}`} object={object} tables={tables} />
        )}
      </div>
    </div>
  );
}

/** Ctrl+Enter saves, as any tool panel's button. */
const saveKey = (save: () => void) => (e: KeyboardEvent) => {
  if (e.key === 'Enter' && (e.ctrlKey || e.metaKey)) {
    e.preventDefault();
    save();
  }
};

function TableEditor({ object, onMade }: { object: ObjectSummary; onMade: (id: string) => void }) {
  const { summary, selection, runRecord } = useWorkspace();
  const [saved, setSaved] = useState<ParameterTable | null>(null);
  const [grid, setGrid] = useState<Grid | null>(null);
  const [savedColumns, setSavedColumns] = useState<string[]>([]);
  const [rowsPicked, setRowsPicked] = useState<Set<string>>(new Set());
  const [engineWhy, setEngineWhy] = useState<string | null>(null);
  const [said, setSaid] = useState<{ text: string; tone: 'ok' | 'warning' | 'refused' } | null>(null);
  const [picked, setPicked] = useState<ParametersPicked | null>(null);

  useEffect(() => {
    let live = true;
    void objectDetails(object.id).then((d) => {
      if (!live || d.measures.kind !== 'parameter_table') return;
      const g = toGrid(d.measures.parameters);
      setSaved(d.measures.parameters);
      setGrid(g);
      setSavedColumns(g.columns.map((c) => c.key));
    });
    return () => {
      live = false;
    };
  }, [object.id]);

  // The last solid picked, and where it takes its values from.
  const solids = useMemo(() => new Map((summary?.objects ?? []).filter((o) => o.kind === 'solid').map((o) => [o.id, o.name])), [summary]);
  const solid = [...selection].reverse().find((s) => solids.has(s)) ?? null;
  useEffect(() => {
    let live = true;
    void parametersPicked(object.id, solid).then((p) => live && setPicked(p));
    return () => {
      live = false;
    };
  }, [object.id, solid]);

  const checked = useMemo(() => (grid ? fromGrid(grid) : null), [grid]);
  // The engine's own check, once the grid's own checks pass.
  useEffect(() => {
    if (!checked || checked.problems.length) return setEngineWhy(null);
    let live = true;
    const t = setTimeout(() => {
      parametersCheck(checked.table)
        .then(() => live && setEngineWhy(null))
        .catch((e) => live && setEngineWhy(String(e)));
    }, 150);
    return () => {
      live = false;
      clearTimeout(t);
    };
  }, [checked]);

  if (!grid || !saved || !checked) return <div className="p-3 text-muted-foreground">Opening {object.name}…</div>;

  const edited = gridEdited(grid, saved.rows.length, savedColumns);
  const why = (row: string | null, column: string) => checked.problems.find((p) => p.row === row && p.column === column)?.why ?? null;
  const refused = checked.problems.length > 0 || engineWhy !== null;
  const here = picked?.tables.findIndex(([t]) => t === object.id) ?? -1;
  const fits = new Set(picked?.picked && here >= 0 ? picked.picked.fits[here] : []);
  const givenHere = (column: string, savedRow: number | null) => {
    const g = picked?.picked?.gives[column];
    return !!g && g.table === here && savedRow !== null && g.row === savedRow;
  };
  const fromElsewhere = (column: string) => {
    const g = picked?.picked?.gives[column];
    return g && g.table !== here ? `${picked?.tables[g.table]?.[1] ?? 'another table'}, row ${g.row + 1}` : null;
  };

  const update = (g: Grid) => {
    setGrid(g);
    setSaid(null);
  };
  const save = async () => {
    if (!edited || refused) return;
    const reply = await runRecord('parameters.save', { table: object.id, name: object.name, content: checked.table });
    if (reply.error) return setSaid({ text: reply.error, tone: 'refused' });
    setSaid({ text: picked?.dataflow ? `Saved. Reserves tables made before this used the last revision: run reserves again to see it.` : 'Saved.', tone: 'ok' });
  };
  const copyAsNew = async () => {
    const reply = await runRecord('parameters.save', { name: `${object.name} copy`, content: checked.table });
    const made = reply.record?.outputs[0]?.id;
    if (made) onMade(made);
  };
  const discard = () => {
    const g = toGrid(saved);
    setGrid(g);
    setSavedColumns(g.columns.map((c) => c.key));
    setRowsPicked(new Set());
    setSaid(null);
  };
  const onPaste = (row: string, column: string) => (e: ClipboardEvent<HTMLInputElement>) => {
    const text = e.clipboardData.getData('text/plain');
    if (!/[\t\n]/.test(text.replace(/\r?\n$/, ''))) return;
    e.preventDefault();
    const { grid: g, cut } = paste(grid, row, column, parseClipboard(text));
    update(g);
    if (cut) setSaid({ text: `${cut} pasted cell${cut === 1 ? '' : 's'} past the last column left out`, tone: 'warning' });
  };
  // Enter goes down a row, Shift+Enter up, as in a sheet.
  const onCellKey = (r: number, c: number) => (e: KeyboardEvent<HTMLInputElement>) => {
    if (e.key !== 'Enter' || e.ctrlKey || e.metaKey) return;
    e.preventDefault();
    const next = document.querySelector<HTMLInputElement>(`[data-tool-panel="parameters"] [data-cell="${r + (e.shiftKey ? -1 : 1)}:${c}"]`);
    next?.focus();
    next?.select();
  };
  const toggleRow = (key: string, add: boolean) =>
    setRowsPicked((s) => {
      const next = new Set(add ? s : []);
      if (s.has(key) && add) next.delete(key);
      else next.add(key);
      return next;
    });
  const copyRows = () => void navigator.clipboard.writeText(copyText(grid, rowsPicked.size ? rowsPicked : null)).then(() => setSaid({ text: `${rowsPicked.size || grid.rows.length} rows copied, with their headings, to paste into Excel`, tone: 'ok' }));

  return (
    <div className="flex min-h-0 flex-1 flex-col" onKeyDown={saveKey(() => void save())}>
      <div className="flex shrink-0 flex-wrap items-center gap-x-1.5 gap-y-1 border-b border-border px-2 py-1">
        <span className="font-semibold">{object.name}</span>
        {picked?.dataflow && <span className="text-muted-foreground">read by {picked.dataflow}{picked.tables.length > 1 ? `, table ${here + 1} of ${picked.tables.length}` : ''}</span>}
        {edited && <StatusBadge tone="edited">edited</StatusBadge>}
        <span className="ml-2 flex items-center gap-1">
          <Button variant="outline" size="sm" className="h-6 px-2 text-xs" onClick={() => update(insertRow(grid))} title="A blank row at the bottom; move it up to where it's read">
            <Plus className="size-3.5" /> Row
          </Button>
          <Button variant="outline" size="sm" className="h-6 px-2 text-xs" onClick={() => update(addColumn(grid, 'filter'))} title="A filter column: the attribute a row is chosen by">
            <Plus className="size-3.5" /> Filter
          </Button>
          <Button variant="outline" size="sm" className="h-6 px-2 text-xs" onClick={() => update(addColumn(grid, 'value'))} title="A value column: an assumption the rows give">
            <Plus className="size-3.5" /> Value
          </Button>
          <Button variant="outline" size="sm" className="h-6 px-2 text-xs" disabled={!rowsPicked.size} onClick={() => (update(deleteRows(grid, rowsPicked)), setRowsPicked(new Set()))} title="Take the picked rows out (pick a row by its number)">
            <Trash2 className="size-3.5" /> Rows
          </Button>
          <Button variant="outline" size="sm" className="h-6 px-2 text-xs" onClick={copyRows} title="Copy the picked rows, or all of them, with their headings, to paste into Excel">
            <Copy className="size-3.5" /> Copy
          </Button>
          <Button variant="outline" size="sm" className="h-6 px-2 text-xs" disabled={refused} onClick={() => void copyAsNew()} title="Save this table as a new one, as it stands: a case's own table to read before this one">
            <CopyPlus className="size-3.5" /> Copy as a new table
          </Button>
        </span>
        <span className="ml-auto flex items-center gap-1">
          <Button variant="ghost" size="sm" className="h-6 px-2 text-xs" disabled={!edited} onClick={discard} title="Put the table back as saved">
            <Undo2 className="size-3.5" /> Discard
          </Button>
          <Button size="sm" className="h-6 px-2 text-xs" disabled={!edited || refused} onClick={() => void save()} title="Save the table as its next revision (Ctrl+Enter); one undo">
            <Save className="size-3.5" /> Save
          </Button>
        </span>
      </div>
      {(said || engineWhy || checked.problems.length > 0 || picked?.picked) && (
        <div className="flex shrink-0 flex-col gap-0.5 border-b border-border px-2 py-1">
          {checked.problems.length > 0 && <div className="text-destructive">{checked.problems.length === 1 ? checked.problems[0].why : `${checked.problems.length} cells can't be saved as they are: ${checked.problems[0].why}, …`}</div>}
          {engineWhy && <div className="text-destructive">{engineWhy}</div>}
          {said && <div className={cn(said.tone === 'ok' && 'text-ok', said.tone === 'warning' && 'text-warning', said.tone === 'refused' && 'text-destructive')}>{said.text}</div>}
          {picked?.picked && solid && (
            <div className="text-muted-foreground" data-slot="parameters-picked">
              <span className="text-primary">{solids.get(solid)}</span> fits {fits.size ? `row${fits.size === 1 ? '' : 's'} ${[...fits].map((r) => r + 1).join(', ')}` : 'no row'} of this table; its values in gold{edited ? ', as saved' : ''}.
            </div>
          )}
        </div>
      )}
      <div className="min-h-0 flex-1 overflow-auto">
        <table className="border-separate border-spacing-0" data-slot="parameters-grid">
          <thead className="sticky top-0 z-10 bg-card">
            <tr>
              <th className="sticky left-0 z-20 w-16 border-b border-border bg-card" />
              {grid.columns.map((c, ci) => (
                <th key={c.key} className={cn('min-w-28 border-b border-border px-0.5 pt-1 align-bottom font-normal', c.kind === 'value' && ci > 0 && grid.columns[ci - 1].kind === 'filter' && 'border-l')} data-col={c.name}>
                  <div className="flex items-center gap-0.5">
                    <span className="shrink-0 text-muted-foreground">{c.kind === 'filter' ? 'when' : ''}</span>
                    <CellInput value={c.name} label={`Name of the ${c.kind === 'filter' ? 'filter' : 'value'} column`} edited={c.was === null || c.was.name !== c.name} why={why(null, c.key)} className="font-semibold" onChange={(name) => update(editColumn(grid, c.key, { name }))} />
                    <IconButton label="Move left" className="size-5" onClick={() => update(moveColumn(grid, c.key, -1))}>
                      <ArrowLeft />
                    </IconButton>
                    <IconButton label="Move right" className="size-5" onClick={() => update(moveColumn(grid, c.key, 1))}>
                      <ArrowRight />
                    </IconButton>
                    <IconButton label="Take this column out" className="size-5" onClick={() => update(deleteColumn(grid, c.key))}>
                      <X />
                    </IconButton>
                  </div>
                  <div className="flex items-center gap-1 pb-1" title={fromElsewhere(c.name) ? `The picked solid takes ${c.name} from ${fromElsewhere(c.name)}` : undefined}>
                    {c.kind === 'value' ? (
                      <>
                        <CellInput value={c.unit} label={`Unit of ${c.name}`} placeholder="unit" edited={c.was === null || c.was.unit !== c.unit} className="w-14 flex-none text-muted-foreground" onChange={(unit) => update(editColumn(grid, c.key, { unit }))} />
                        <Choice value={c.text ? 'words' : 'numbers'} options={['numbers', 'words'] as const} labels={{ numbers: '123', words: 'abc' }} label={`${c.name} holds numbers or words`} onChange={(v) => update(editColumn(grid, c.key, { text: v === 'words' }))} />
                        {fromElsewhere(c.name) && <span className="truncate text-primary">from {fromElsewhere(c.name)}</span>}
                      </>
                    ) : (
                      <span className="px-1 text-muted-foreground">filter{columnEdited(c) && c.was === null ? ', new' : ''}</span>
                    )}
                  </div>
                </th>
              ))}
            </tr>
          </thead>
          <tbody>
            {grid.rows.map((r, ri) => (
              <tr key={r.key} data-row={ri + 1} className={cn(rowsPicked.has(r.key) && 'bg-accent/60')}>
                <td className="sticky left-0 z-[1] border-b border-border bg-card px-1 whitespace-nowrap">
                  <span className="flex items-center">
                    <button
                      type="button"
                      className={cn('w-6 text-right tabular-nums text-muted-foreground hover:text-foreground', r.saved !== null && fits.has(r.saved) && 'font-semibold text-primary', rowsPicked.has(r.key) && 'text-foreground')}
                      title={`Row ${ri + 1}, read ${ri === 0 ? 'first' : `after row ${ri}`}${r.saved !== null && fits.has(r.saved) ? '; the picked solid fits it' : ''}. Click to pick, Ctrl+click to add`}
                      onClick={(e) => toggleRow(r.key, e.ctrlKey || e.metaKey)}
                    >
                      {ri + 1}
                    </button>
                    <IconButton label="Move up (read earlier)" className="size-5" onClick={() => update(moveRow(grid, r.key, -1))}>
                      <ArrowUp />
                    </IconButton>
                    <IconButton label="Move down (read later)" className="size-5" onClick={() => update(moveRow(grid, r.key, 1))}>
                      <ArrowDown />
                    </IconButton>
                  </span>
                </td>
                {grid.columns.map((c, ci) => (
                  <td key={c.key} className={cn('border-b border-border px-0.5', c.kind === 'value' && ci > 0 && grid.columns[ci - 1].kind === 'filter' && 'border-l')}>
                    <CellInput
                      value={r.cells[c.key] ?? ''}
                      label={`Row ${ri + 1}, ${c.kind === 'filter' ? `when ${c.name}` : c.name}`}
                      data-cell={`${ri}:${ci}`}
                      numeric={c.kind === 'value' && !c.text}
                      placeholder={c.kind === 'filter' ? 'any' : ''}
                      edited={cellEdited(r, c)}
                      why={why(r.key, c.key)}
                      picked={givenHere(c.name, r.saved)}
                      onChange={(text) => update(setCell(grid, r.key, c.key, text))}
                      onPaste={onPaste(r.key, c.key)}
                      onKeyDown={onCellKey(ri, ci)}
                    />
                  </td>
                ))}
              </tr>
            ))}
          </tbody>
        </table>
        <p className="px-3 py-2 text-muted-foreground">
          Read top down: each value comes from the first row that fits and holds it; a blank filter fits anything. A filter cell takes words, numbers and ranges as the bench rules do (U, 1-5, "U, M"). Paste cells from Excel into any cell.
        </p>
      </div>
    </div>
  );
}

type FlowField = Dataflow['fields'][number];

function DataflowEditor({ object, tables }: { object: ObjectSummary; tables: ObjectSummary[] }) {
  const { runRecord } = useWorkspace();
  const [saved, setSaved] = useState<Dataflow | null>(null);
  const [flow, setFlow] = useState<Dataflow | null>(null);
  const [engineWhy, setEngineWhy] = useState<string | null>(null);
  const [said, setSaid] = useState<{ text: string; tone: 'ok' | 'refused' } | null>(null);

  useEffect(() => {
    let live = true;
    void objectDetails(object.id).then((d) => {
      if (!live || d.measures.kind !== 'dataflow') return;
      setSaved(d.measures.dataflow);
      setFlow(d.measures.dataflow);
    });
    return () => {
      live = false;
    };
  }, [object.id]);

  useEffect(() => {
    if (!flow) return;
    let live = true;
    const t = setTimeout(() => {
      dataflowCheck(flow)
        .then(() => live && setEngineWhy(null))
        .catch((e) => live && setEngineWhy(String(e)));
    }, 150);
    return () => {
      live = false;
      clearTimeout(t);
    };
  }, [flow]);

  if (!flow || !saved) return <div className="p-3 text-muted-foreground">Opening {object.name}…</div>;

  const same = (a: unknown, b: unknown) => JSON.stringify(a) === JSON.stringify(b);
  const edited = !same(flow, saved);
  const names = new Map(tables.map((t) => [t.id, t.name]));
  const densities = flow.fields.filter((f) => f.kind === 'density').map((f) => f.name);
  const levels = flow.levels ?? [];
  const set = (change: Partial<Dataflow>) => {
    setFlow({ ...flow, ...change });
    setSaid(null);
  };
  const setField = (i: number, f: FlowField) => set({ fields: flow.fields.map((g, j) => (j === i ? f : g)) });
  const fieldEdited = (f: FlowField) => !saved.fields.some((g) => same(g, f));
  const save = async () => {
    if (!edited || engineWhy) return;
    const reply = await runRecord('dataflow.save', { dataflow: object.id, name: object.name, content: flow });
    if (reply.error) return setSaid({ text: reply.error, tone: 'refused' });
    setSaid({ text: 'Saved. Reserves tables made before this used the last revision: run reserves again to see it.', tone: 'ok' });
  };
  const head = 'border-b border-border px-1 py-0.5 text-left font-medium text-muted-foreground';

  return (
    <div className="flex min-h-0 flex-1 flex-col" onKeyDown={saveKey(() => void save())}>
      <div className="flex shrink-0 items-center gap-1.5 border-b border-border px-2 py-1">
        <span className="font-semibold">{object.name}</span>
        {edited && <StatusBadge tone="edited">edited</StatusBadge>}
        <span className="ml-auto flex items-center gap-1">
          <Button variant="ghost" size="sm" className="h-6 px-2 text-xs" disabled={!edited} onClick={() => (setFlow(saved), setSaid(null))}>
            <Undo2 className="size-3.5" /> Discard
          </Button>
          <Button size="sm" className="h-6 px-2 text-xs" disabled={!edited || !!engineWhy} onClick={() => void save()} title="Save the dataflow as its next revision (Ctrl+Enter); one undo">
            <Save className="size-3.5" /> Save
          </Button>
        </span>
      </div>
      {(engineWhy || said) && (
        <div className="shrink-0 border-b border-border px-2 py-1">
          {engineWhy && <div className="text-destructive">{engineWhy}</div>}
          {said && <div className={said.tone === 'ok' ? 'text-ok' : 'text-destructive'}>{said.text}</div>}
        </div>
      )}
      <div className="flex min-h-0 flex-1 gap-4 overflow-auto p-2">
        <div className="flex w-72 shrink-0 flex-col gap-3">
          <section>
            <div className="mb-0.5 font-semibold">Levels</div>
            <p className="mb-1 text-muted-foreground">Block attributes, top down: the reserves table's tree. Those above seam make a column.</p>
            {levels.map((l, i) => (
              <div key={i} className="flex items-center gap-1" data-level={l.attribute}>
                <CellInput value={l.attribute} label={`Level ${i + 1}`} edited={!same(saved.levels?.[i], l)} onChange={(attribute) => set({ levels: levels.map((m, j) => (j === i ? { ...m, attribute } : m)) })} />
                <CellInput
                  value={l.default ?? ''}
                  placeholder="no default"
                  label={`Level ${i + 1}: the value a block without it takes`}
                  className="w-24 flex-none text-muted-foreground"
                  onChange={(d) => set({ levels: levels.map((m, j) => (j === i ? (d ? { ...m, default: d } : { attribute: m.attribute }) : m)) })}
                />
                <IconButton label="Move up" className="size-5" onClick={() => set({ levels: moved(levels, i, -1) })}>
                  <ArrowUp />
                </IconButton>
                <IconButton label="Move down" className="size-5" onClick={() => set({ levels: moved(levels, i, 1) })}>
                  <ArrowDown />
                </IconButton>
                <IconButton label="Take this level out" className="size-5" onClick={() => set({ levels: levels.filter((_, j) => j !== i) })}>
                  <X />
                </IconButton>
              </div>
            ))}
            <Button variant="outline" size="sm" className="mt-1 h-6 px-2 text-xs" onClick={() => set({ levels: [...levels, { attribute: 'attribute' }] })}>
              <Plus className="size-3.5" /> Level
            </Button>
          </section>
          <section>
            <div className="mb-0.5 font-semibold">Parameter tables</div>
            <p className="mb-1 text-muted-foreground">Read in this order: a value comes from the first table that gives it, so a case's own table goes first.</p>
            {flow.parameters.map((t, i) => (
              <div key={t} className="flex items-center gap-1" data-table={names.get(t) ?? t}>
                <span className={cn('min-w-0 flex-1 truncate', !names.has(t) && 'text-destructive', saved.parameters[i] !== t && 'text-edited')}>
                  {i + 1}. {names.get(t) ?? 'missing'}
                </span>
                <IconButton label="Read earlier" className="size-5" onClick={() => set({ parameters: moved(flow.parameters, i, -1) })}>
                  <ArrowUp />
                </IconButton>
                <IconButton label="Read later" className="size-5" onClick={() => set({ parameters: moved(flow.parameters, i, 1) })}>
                  <ArrowDown />
                </IconButton>
                <IconButton label="Stop reading this table" className="size-5" onClick={() => set({ parameters: flow.parameters.filter((_, j) => j !== i) })}>
                  <X />
                </IconButton>
              </div>
            ))}
            {tables.some((t) => !flow.parameters.includes(t.id)) && (
              <Select className="mt-1 w-full" value="" aria-label="Read another table" onChange={(e) => e.target.value && set({ parameters: [e.target.value, ...flow.parameters] })}>
                <option value="">Read another table first…</option>
                {tables
                  .filter((t) => !flow.parameters.includes(t.id))
                  .map((t) => (
                    <option key={t.id} value={t.id}>
                      {t.name}
                    </option>
                  ))}
              </Select>
            )}
          </section>
          <section>
            <div className="mb-0.5 font-semibold">Steps</div>
            <p className="mb-1 text-muted-foreground">Run in this fixed order. A setting names the field or parameter column a step reads, where it isn't the usual one.</p>
            {flow.steps.map((s, i) => {
              const settings = Object.entries(s.settings ?? {});
              const setStep = (change: Partial<typeof s>) => set({ steps: flow.steps.map((t, j) => (j === i ? { ...t, ...change } : t)) });
              const setSettings = (pairs: [string, string][]) => setStep({ settings: Object.fromEntries(pairs) });
              return (
                <div key={s.step} className="mb-1" data-step={s.step}>
                  <div className="flex items-center gap-1">
                    <Choice value={s.on === false ? 'off' : 'on'} options={['on', 'off'] as const} label={`${s.step} on or off`} onChange={(v) => setStep({ on: v === 'on' ? undefined : false })} />
                    <span className={cn('min-w-0 flex-1 truncate', s.on === false && 'text-muted-foreground line-through', !same(saved.steps[i], s) && 'text-edited')}>{s.step.replaceAll('_', ' ')}</span>
                    <IconButton label="Add a setting" className="size-5" onClick={() => setSettings([...settings, ['setting', '']])}>
                      <Plus />
                    </IconButton>
                  </div>
                  {settings.map(([k, v], n) => (
                    <div key={n} className="flex items-center gap-1 pl-6">
                      <CellInput value={k} label={`${s.step}: setting`} className="text-muted-foreground" onChange={(key) => setSettings(settings.map((p, m) => (m === n ? [key, p[1]] : p)))} />
                      <CellInput value={v} label={`${s.step}: ${k}`} onChange={(value) => setSettings(settings.map((p, m) => (m === n ? [p[0], value] : p)))} />
                      <IconButton label="Take this setting out" className="size-5" onClick={() => setSettings(settings.filter((_, m) => m !== n))}>
                        <X />
                      </IconButton>
                    </div>
                  ))}
                </div>
              );
            })}
          </section>
        </div>
        <section className="min-w-0 flex-1">
          <div className="mb-0.5 font-semibold">Fields</div>
          <p className="mb-1 text-muted-foreground">What each unit carries, how it adds up, and where its value comes from when a block becomes a unit.</p>
          <table className="border-separate border-spacing-0" data-slot="dataflow-fields">
            <thead>
              <tr>
                <th className={head}>Field</th>
                <th className={head}>Kind</th>
                <th className={head}>Adds up</th>
                <th className={head}>From</th>
                <th className={head} />
              </tr>
            </thead>
            <tbody>
              {flow.fields.map((f, i) => (
                <tr key={i} data-field={f.name}>
                  <td className="w-40 border-b border-border px-0.5">
                    <CellInput value={f.name} label={`Field ${i + 1}: name`} edited={fieldEdited(f)} onChange={(name) => setField(i, { ...f, name })} />
                  </td>
                  <td className="border-b border-border px-0.5">
                    <Select value={f.kind} aria-label={`${f.name}: kind`} onChange={(e) => setField(i, { ...f, kind: e.target.value })}>
                      {KINDS.map((k) => (
                        <option key={k} value={k}>
                          {k.replaceAll('_', ' ')}
                        </option>
                      ))}
                    </Select>
                  </td>
                  <td className="border-b border-border px-0.5 whitespace-nowrap">
                    <Select value={f.add_up.by} aria-label={`${f.name}: adds up`} onChange={(e) => setField(i, withAddUp(f, e.target.value as AddUpChoice, densities))}>
                      {ADD_UPS.map((a) => (
                        <option key={a} value={a}>
                          {ADD_UP_LABELS[a]}
                        </option>
                      ))}
                    </Select>
                    {f.add_up.by === 'mass' && (
                      <Select className="ml-1" value={f.add_up.density} aria-label={`${f.name}: the density its tonnes are worked with`} onChange={(e) => setField(i, { ...f, add_up: { by: 'mass', density: e.target.value } })}>
                        {[...new Set([...densities, f.add_up.density])].map((d) => (
                          <option key={d} value={d}>
                            {d}
                          </option>
                        ))}
                      </Select>
                    )}
                  </td>
                  <td className="border-b border-border px-0.5 whitespace-nowrap">
                    <span className="flex items-center gap-1">
                      <Select value={f.from} aria-label={`${f.name}: where its value comes from`} onChange={(e) => setField(i, withFrom(f, e.target.value as FromChoice))}>
                        {FROMS.map((a) => (
                          <option key={a} value={a}>
                            {FROM_LABELS[a]}
                          </option>
                        ))}
                      </Select>
                      {f.from === 'attribute' && <CellInput className="w-28" value={f.attribute ?? ''} label={`${f.name}: the attribute`} onChange={(attribute) => setField(i, { ...f, attribute })} />}
                      {f.from === 'parameter' && <CellInput className="w-28" value={f.column ?? ''} label={`${f.name}: the parameter column`} onChange={(column) => setField(i, { ...f, column })} />}
                    </span>
                  </td>
                  <td className="border-b border-border whitespace-nowrap">
                    <IconButton label="Move up" className="size-5" onClick={() => set({ fields: moved(flow.fields, i, -1) })}>
                      <ArrowUp />
                    </IconButton>
                    <IconButton label="Move down" className="size-5" onClick={() => set({ fields: moved(flow.fields, i, 1) })}>
                      <ArrowDown />
                    </IconButton>
                    <IconButton label="Take this field out" className="size-5" onClick={() => set({ fields: flow.fields.filter((_, j) => j !== i) })}>
                      <Trash2 />
                    </IconButton>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
          <Button variant="outline" size="sm" className="mt-1 h-6 px-2 text-xs" onClick={() => set({ fields: [...flow.fields, { name: 'new_field', kind: 'quality', add_up: { by: 'volume' }, from: 'attribute', attribute: 'new_field' }] })}>
            <Plus className="size-3.5" /> Field
          </Button>
        </section>
      </div>
    </div>
  );
}
