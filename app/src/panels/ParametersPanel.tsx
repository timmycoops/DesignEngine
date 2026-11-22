import { ArrowDown, ArrowLeft, ArrowRight, ArrowUp, Braces, Copy, CopyPlus, Plus, Save, Table2, Trash2, Undo2, X } from 'lucide-react';
import { type ClipboardEvent, type KeyboardEvent, useEffect, useMemo, useState } from 'react';
import { type Dataflow, dataflowCheck, objectDetails, type ObjectSummary, type ParameterTable, type ParametersPicked, parametersCheck, parametersPicked } from '@/bridge';
import { CellInput, Choice, IconButton, ObjectMenu, Select, StatusBadge } from '@/design-system/kit';
import { ProjectScriptEditor } from './ProjectScriptEditor';
import { Button } from '@/design-system/ui/button';
import { cn } from 'cn';
import { useWorkspace } from '@/workspace/context';
import {
  ADD_UP_LABELS,
  ADD_UPS,
  type AddUpChoice,
  addColumn,
  cellEdited,
  copyTextTurned,
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
  pasteTurned,
  parseClipboard,
  setCell,
  toGrid,
  withAddUp,
  withFrom,
} from './parametersGrid';
import { FormulaEditor } from './FormulaEditor';
import { TABLE_STEP_TEMPLATE, TableStepEditor } from './TableStepEditor';
import { FORMULA_TEMPLATE } from './formulaText';

/** The built-in steps a formula step may follow (decision 0077): as the engine's FORMULA_STAGES. */
const FORMULA_STAGES = ['initialise', 'join_benches', 'working_sections', 'rom_dilution', 'rom'];

/**
 * The Parameters tab (decision 0075, workstream 5d of 0070): the project's parameter tables and dataflows edited as grids.
 * A parameter table turned (Tim, 16 November): its filters then its parameters down the side, its rows across as numbered
 * columns read left to right; cells typed or pasted from Excel; edits held, in blue,
 * until Save writes the next revision (`parameters.save`, `dataflow.save`), one undo. A solid picked in the view marks the
 * rows it fits and, in gold, the cell each value comes from. Saving never reruns the reserves: a new answer is a new run.
 */

const isEditable = (o: ObjectSummary) => o.kind === 'parameter_table' || o.kind === 'dataflow' || o.kind === 'script';

/** A new script being written, before it's saved (decision 0099). */
const NEW_SCRIPT = 'new-script';

export function Parameters() {
  const { summary, selection, toolAsk } = useWorkspace();
  const objects = useMemo(() => (summary?.objects ?? []).filter(isEditable), [summary]);
  const [id, setId] = useState<string | null>(null);
  const last = selection[selection.length - 1] ?? null;
  // A table or dataflow picked anywhere is the one shown.
  useEffect(() => {
    if (last && objects.some((o) => o.id === last)) setId(last);
  }, [last, objects]);
  // A formula asked for from elsewhere (a notebook cell's Edit, decision 0081): its dataflow shown, the formula opened.
  const [openStep, setOpenStep] = useState<{ step: string; n: number } | null>(null);
  // A setting asked for from the Dataflow tab (decision 0082, point 7): its table shown, the cell it came from in sight.
  const [openCell, setOpenCell] = useState<OpenCell | null>(null);
  useEffect(() => {
    if (toolAsk?.panel !== 'params' || !toolAsk.mode) return;
    if (toolAsk.mode.startsWith('formula:')) {
      const [, flowId, step] = toolAsk.mode.split(':');
      setId(flowId);
      setOpenStep({ step, n: toolAsk.n });
    } else if (toolAsk.mode.startsWith('cell:')) {
      const [, tableId, row, ...column] = toolAsk.mode.split(':');
      setId(tableId);
      setOpenCell({ row: Number(row), column: column.join(':'), n: toolAsk.n });
    }
  }, [toolAsk]);
  const object = id === NEW_SCRIPT ? null : (objects.find((o) => o.id === id) ?? objects.find((o) => o.kind === 'parameter_table') ?? objects[0] ?? null);

  if (!object && id !== NEW_SCRIPT) {
    return (
      <div className="space-y-2 p-3 text-xs text-muted-foreground">
        <button type="button" className="text-foreground hover:underline" onClick={() => setId(NEW_SCRIPT)}>
          + New script
        </button>
        <div>No parameter tables, dataflows or scripts in this project yet. Tables and dataflows are made from Python (parameters.save, dataflow.save, or the starter dataflow) and edited here.</div>
      </div>
    );
  }
  const tables = objects.filter((o) => o.kind === 'parameter_table');
  const flows = objects.filter((o) => o.kind === 'dataflow');
  const scripts = objects.filter((o) => o.kind === 'script');
  const item = (o: ObjectSummary) => (
    <div key={o.id} className={cn('flex items-center pr-1 hover:bg-accent', o.id === object?.id && 'bg-selection text-primary')}>
      <button
        type="button"
        data-slot="parameters-item"
        data-name={o.name}
        className="block min-w-0 flex-1 truncate py-0.5 pl-3 text-left"
        onClick={() => setId(o.id)}
        title={`${o.name}, revision ${o.revision}`}
      >
        {o.name}
      </button>
      {o.id === object?.id && <ObjectMenu object={o} what={o.kind === 'dataflow' ? 'dataflow' : o.kind === 'script' ? 'script' : 'parameter table'} onDeleted={() => setId(null)} />}
    </div>
  );
  return (
    <div className="flex h-full min-h-0 text-xs" data-tool-panel="parameters">
      <div className="w-52 shrink-0 overflow-auto border-r border-border py-1">
        <div className="px-2 py-0.5 font-semibold text-muted-foreground">Parameter tables</div>
        {tables.length ? tables.map(item) : <div className="px-3 text-muted-foreground">none</div>}
        <div className="mt-2 px-2 py-0.5 font-semibold text-muted-foreground">Dataflows</div>
        {flows.length ? flows.map(item) : <div className="px-3 text-muted-foreground">none</div>}
        <div className="mt-2 px-2 py-0.5 font-semibold text-muted-foreground" title="Starlark a dataflow's formula or a calculated column loads by its name: load(&quot;name&quot;, &quot;function&quot;)">Scripts</div>
        {scripts.map(item)}
        <button type="button" data-slot="new-script" className={cn('block w-full py-0.5 pl-3 text-left text-muted-foreground hover:bg-accent hover:text-foreground', id === NEW_SCRIPT && 'bg-selection text-primary')} onClick={() => setId(NEW_SCRIPT)}>
          + New script
        </button>
      </div>
      <div className="flex min-w-0 flex-1 flex-col">
        {!object || object.kind === 'script' ? (
          <ProjectScriptEditor key={object ? `${object.id}@${object.revision}` : NEW_SCRIPT} object={object} onSaved={setId} />
        ) : object.kind === 'parameter_table' ? (
          <TableEditor key={`${object.id}@${object.revision}`} object={object} onMade={setId} openCell={openCell} />
        ) : (
          <DataflowEditor key={`${object.id}@${object.revision}`} object={object} tables={tables} openStep={openStep} />
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

/** A cell asked for: the table's row (as saved, from 0) and the parameter; `n` counts asks, so the same again shows it again. */
interface OpenCell {
  row: number;
  column: string;
  n: number;
}

function TableEditor({ object, onMade, openCell }: { object: ObjectSummary; onMade: (id: string) => void; openCell: OpenCell | null }) {
  const { summary, selection, runRecord, setDraft } = useWorkspace();
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

  // A cell asked for, once the grid is read: scrolled into sight, focused and ringed for a moment.
  const [asked, setAsked] = useState<string | null>(null);
  const gridRead = grid !== null;
  useEffect(() => {
    if (!openCell || !grid) return;
    const ri = grid.rows.findIndex((r) => r.saved === openCell.row);
    if (ri < 0) return;
    const label = `Column ${ri + 1}, ${openCell.column}`;
    setAsked(label);
    const frame = requestAnimationFrame(() => {
      const input = document.querySelector<HTMLInputElement>(`[data-tool-panel="parameters"] input[aria-label="${CSS.escape(label)}"]`);
      input?.scrollIntoView({ block: 'center', inline: 'center' });
      input?.focus();
      input?.select();
    });
    const t = setTimeout(() => setAsked(null), 2500);
    return () => {
      cancelAnimationFrame(frame);
      clearTimeout(t);
    };
  }, [openCell?.n, gridRead]);

  // The edits as they stand, for the Dataflow tab to run on live (decision 0082); none once saved, discarded or closed.
  useEffect(() => {
    if (!grid || !saved || !checked || !gridEdited(grid, saved.rows.length, savedColumns)) return setDraft(object.id, null);
    const why = checked.problems[0]?.why ?? engineWhy;
    setDraft(object.id, { kind: 'table', name: object.name, table: why ? null : checked.table, why });
  }, [checked, engineWhy, saved]);
  useEffect(() => () => setDraft(object.id, null), [object.id]);

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
    return g && g.table !== here ? `${picked?.tables[g.table]?.[1] ?? 'another table'}, column ${g.row + 1}` : null;
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
  // Pasted as the grid shows it: a line of the block along a parameter, across the table's rows.
  const onPaste = (row: string, column: string) => (e: ClipboardEvent<HTMLInputElement>) => {
    const text = e.clipboardData.getData('text/plain');
    if (!/[\t\n]/.test(text.replace(/\r?\n$/, ''))) return;
    e.preventDefault();
    const { grid: g, cut } = pasteTurned(grid, row, column, parseClipboard(text));
    update(g);
    if (cut) setSaid({ text: `${cut} pasted cell${cut === 1 ? '' : 's'} past the last parameter left out`, tone: 'warning' });
  };
  // Enter goes down to the next parameter, Shift+Enter up, as in a sheet; Tab goes across.
  const onCellKey = (p: number, r: number) => (e: KeyboardEvent<HTMLInputElement>) => {
    if (e.key !== 'Enter' || e.ctrlKey || e.metaKey) return;
    e.preventDefault();
    const next = document.querySelector<HTMLInputElement>(`[data-tool-panel="parameters"] [data-cell="${p + (e.shiftKey ? -1 : 1)}:${r}"]`);
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
  const copyRows = () =>
    void navigator.clipboard.writeText(copyTextTurned(grid, rowsPicked.size ? rowsPicked : null)).then(() => {
      const n = rowsPicked.size || grid.rows.length;
      setSaid({ text: `${n} column${n === 1 ? '' : 's'} copied, a parameter a line with its heading, to paste into Excel`, tone: 'ok' });
    });
  const firstValue = grid.columns.findIndex((c) => c.kind === 'value');
  const tool = 'h-6 px-2 text-xs';

  return (
    <div className="flex min-h-0 flex-1 flex-col" onKeyDown={saveKey(() => void save())}>
      <div className="flex shrink-0 flex-wrap items-center gap-x-1.5 gap-y-1 border-b border-border px-2 py-1">
        <span className="font-semibold">{object.name}</span>
        {picked?.dataflow && <span className="text-muted-foreground">read by {picked.dataflow}{picked.tables.length > 1 ? `, table ${here + 1} of ${picked.tables.length}` : ''}</span>}
        {edited && <StatusBadge tone="edited">edited</StatusBadge>}
        <span className="ml-2 flex items-center gap-1">
          <Button variant="outline" size="sm" className={tool} onClick={() => update(insertRow(grid))} title="A blank column at the right, read last; move it left to where it's read">
            <Plus className="size-3.5" /> Column
          </Button>
          <Button variant="outline" size="sm" className={tool} onClick={() => update(addColumn(grid, 'filter'))} title="A filter: the attribute a column is chosen by">
            <Plus className="size-3.5" /> Filter
          </Button>
          <Button variant="outline" size="sm" className={tool} onClick={() => update(addColumn(grid, 'value'))} title="A parameter: an assumption the columns give">
            <Plus className="size-3.5" /> Parameter
          </Button>
          <Button variant="outline" size="sm" className={tool} disabled={!rowsPicked.size} onClick={() => (update(deleteRows(grid, rowsPicked)), setRowsPicked(new Set()))} title="Take the picked columns out (pick a column by its number)">
            <Trash2 className="size-3.5" /> Columns
          </Button>
          <Button variant="outline" size="sm" className={tool} onClick={copyRows} title="Copy the picked columns, or all of them, a parameter a line with its heading, to paste into Excel">
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
              <span className="text-primary">{solids.get(solid)}</span> fits {fits.size ? `column${fits.size === 1 ? '' : 's'} ${[...fits].map((r) => r + 1).join(', ')}` : 'no column'} of this table; its values in gold{edited ? ', as saved' : ''}.
            </div>
          )}
        </div>
      )}
      <div className="min-h-0 flex-1 overflow-auto">
        {/* Turned (Tim, 16 November): a filter or parameter a row, the table's rows across as numbered columns, read left to right. */}
        <table className="border-separate border-spacing-0" data-slot="parameters-grid">
          <thead className="sticky top-0 z-10 bg-card">
            <tr>
              <th className="sticky left-0 z-20 border-b border-r border-border bg-card px-2 py-1 text-left font-normal text-muted-foreground">Parameter, unit, numbers or words</th>
              {grid.rows.map((r, ri) => {
                const fit = r.saved !== null && fits.has(r.saved);
                return (
                  <th key={r.key} data-row={ri + 1} className={cn('min-w-24 border-b border-border px-0.5 py-0.5 font-normal', rowsPicked.has(r.key) && 'bg-accent/60')}>
                    <span className="flex items-center justify-center">
                      <IconButton label="Move left (read earlier)" className="size-5" onClick={() => update(moveRow(grid, r.key, -1))}>
                        <ArrowLeft />
                      </IconButton>
                      <button
                        type="button"
                        className={cn('min-w-6 tabular-nums text-muted-foreground hover:text-foreground', fit && 'font-semibold text-primary', rowsPicked.has(r.key) && 'text-foreground', r.saved === null && 'text-edited')}
                        title={`Column ${ri + 1}, read ${ri === 0 ? 'first' : `after column ${ri}`}${fit ? '; the picked solid fits it' : ''}. Click to pick, Ctrl+click to add`}
                        onClick={(e) => toggleRow(r.key, e.ctrlKey || e.metaKey)}
                      >
                        {ri + 1}
                      </button>
                      <IconButton label="Move right (read later)" className="size-5" onClick={() => update(moveRow(grid, r.key, 1))}>
                        <ArrowRight />
                      </IconButton>
                    </span>
                  </th>
                );
              })}
            </tr>
          </thead>
          <tbody>
            {grid.columns.map((c, pi) => {
              const filter = c.kind === 'filter';
              const lastFilter = filter && pi === firstValue - 1;
              const elsewhere = filter ? null : fromElsewhere(c.name);
              const line = cn('border-b border-border', lastFilter && 'border-b-2 border-b-muted-foreground/40', filter && 'bg-muted/30');
              return (
                <tr key={c.key} data-param={c.name} data-kind={c.kind}>
                  <td className={cn('sticky left-0 z-[1] border-r bg-card px-1 whitespace-nowrap', line)} title={c.note || undefined}>
                    <span className="flex items-center gap-0.5">
                      <span className="w-9 shrink-0 text-muted-foreground">{filter ? 'when' : ''}</span>
                      <CellInput
                        value={c.name}
                        label={filter ? 'Name of the filter' : 'Name of the parameter'}
                        edited={c.was === null || c.was.name !== c.name}
                        why={why(null, c.key)}
                        className="w-44 flex-none font-semibold"
                        onChange={(name) => update(editColumn(grid, c.key, { name }))}
                      />
                      {filter ? (
                        <span className="w-[5.75rem] shrink-0 px-1 text-muted-foreground">filter{c.was === null ? ', new' : ''}</span>
                      ) : (
                        <>
                          <CellInput value={c.unit} label={`Unit of ${c.name}`} placeholder="unit" edited={c.was === null || c.was.unit !== c.unit} className="w-14 flex-none text-muted-foreground" onChange={(unit) => update(editColumn(grid, c.key, { unit }))} />
                          {/* Gold is kept for what the picked solid takes, so numbers or words is a plain toggle here. */}
                          <button
                            type="button"
                            className={cn('h-6 w-9 shrink-0 rounded-sm border border-transparent text-muted-foreground hover:border-input hover:text-foreground', c.was !== null && c.was.text !== c.text && 'text-edited')}
                            aria-label={`${c.name} holds ${c.text ? 'words' : 'numbers'}`}
                            title={`${c.name} holds ${c.text ? 'words' : 'numbers'}; click for ${c.text ? 'numbers' : 'words'}`}
                            onClick={() => update(editColumn(grid, c.key, { text: !c.text }))}
                          >
                            {c.text ? 'abc' : '123'}
                          </button>
                        </>
                      )}
                      <IconButton label="Move up" className="size-5" onClick={() => update(moveColumn(grid, c.key, -1))}>
                        <ArrowUp />
                      </IconButton>
                      <IconButton label="Move down" className="size-5" onClick={() => update(moveColumn(grid, c.key, 1))}>
                        <ArrowDown />
                      </IconButton>
                      <IconButton label={filter ? 'Take this filter out' : 'Take this parameter out'} className="size-5" onClick={() => update(deleteColumn(grid, c.key))}>
                        <X />
                      </IconButton>
                    </span>
                    {elsewhere && (
                      <div className="truncate pl-10 text-primary" title={`The picked solid takes ${c.name} from ${elsewhere}`}>
                        from {elsewhere}
                      </div>
                    )}
                  </td>
                  {grid.rows.map((r, ri) => (
                    <td key={r.key} className={cn('px-0.5', line, rowsPicked.has(r.key) && 'bg-accent/60')}>
                      <CellInput
                        value={r.cells[c.key] ?? ''}
                        label={`Column ${ri + 1}, ${filter ? `when ${c.name}` : c.name}`}
                        data-cell={`${pi}:${ri}`}
                        data-asked={asked === `Column ${ri + 1}, ${c.name}` && !filter ? 'yes' : undefined}
                        className={cn(asked === `Column ${ri + 1}, ${c.name}` && !filter && 'ring-2 ring-primary')}
                        numeric={!filter && !c.text}
                        placeholder={filter ? 'any' : ''}
                        edited={cellEdited(r, c)}
                        why={why(r.key, c.key)}
                        picked={givenHere(c.name, r.saved)}
                        onChange={(text) => update(setCell(grid, r.key, c.key, text))}
                        onPaste={onPaste(r.key, c.key)}
                        onKeyDown={onCellKey(pi, ri)}
                      />
                    </td>
                  ))}
                </tr>
              );
            })}
          </tbody>
        </table>
        <p className="px-3 py-2 text-muted-foreground">
          Read left to right: each value comes from the first column that fits and holds it; a blank filter fits anything. A filter cell takes words, numbers, ranges, comparisons and nots as the bench rules do (U, 1-5, "U, M", &lt;0.3, !P). Paste cells from Excel into any cell: a line along a parameter, across the columns.
        </p>
      </div>
    </div>
  );
}

type FlowField = Dataflow['fields'][number];

function DataflowEditor({ object, tables, openStep }: { object: ObjectSummary; tables: ObjectSummary[]; openStep: { step: string; n: number } | null }) {
  const { runRecord, setDraft } = useWorkspace();
  const [saved, setSaved] = useState<Dataflow | null>(null);
  const [flow, setFlow] = useState<Dataflow | null>(null);
  const [engineWhy, setEngineWhy] = useState<string | null>(null);
  const [said, setSaid] = useState<{ text: string; tone: 'ok' | 'refused' } | null>(null);
  const [formulaAt, setFormulaAt] = useState<number | null>(null);
  const [tableAt, setTableAt] = useState<number | null>(null);
  // A formula asked for by name opens once the dataflow is read.
  const flowRead = flow !== null;
  useEffect(() => {
    if (!openStep || !flow) return;
    const at = flow.steps.findIndex((s) => s.step === openStep.step && s.formula);
    if (at >= 0) setFormulaAt(at);
  }, [openStep, flowRead]);

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

  // The dataflow as edited, its formulas with it, for the Dataflow tab to run on live (decision 0082). Always handed on:
  // a worked example that fails is flagged there and run anyway (0081), and what can't run comes back with why.
  useEffect(() => {
    if (!flow || !saved || JSON.stringify(flow) === JSON.stringify(saved)) return setDraft(object.id, null);
    setDraft(object.id, { kind: 'dataflow', name: object.name, dataflow: flow, why: null });
  }, [flow, saved]);
  useEffect(() => () => setDraft(object.id, null), [object.id]);

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
  // A new formula step after a stage, after any formula already there, opened.
  const addFormula = (anchor: string) => {
    let at = flow.steps.findIndex((s) => s.step === anchor && !s.formula);
    if (at < 0) return;
    while (flow.steps[at + 1]?.formula) at++;
    let name = 'formula';
    for (let n = 2; flow.steps.some((s) => s.step === name); n++) name = `formula_${n}`;
    set({ steps: [...flow.steps.slice(0, at + 1), { step: name, formula: { script: FORMULA_TEMPLATE } }, ...flow.steps.slice(at + 1)] });
    setTableAt(null);
    setFormulaAt(at + 1);
  };
  // A new table script step at the end, after final and any table script there, opened (decision 0099, workstream 7).
  const addTableStep = () => {
    let name = 'table_script';
    for (let n = 2; flow.steps.some((s) => s.step === name); n++) name = `table_script_${n}`;
    set({ steps: [...flow.steps, { step: name, table: { script: TABLE_STEP_TEMPLATE } }] });
    setFormulaAt(null);
    setTableAt(flow.steps.length);
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
              if (s.table) {
                // A table script step (decision 0099, workstream 7): opened on the right, run on the case's table last.
                return (
                  <div key={i} className={cn('mb-1 flex items-center gap-1 rounded-sm', tableAt === i && 'bg-selection')} data-step={s.step} data-table-step="yes">
                    <Choice value={s.on === false ? 'off' : 'on'} options={['on', 'off'] as const} label={`${s.step} on or off`} onChange={(v) => setStep({ on: v === 'on' ? undefined : false })} />
                    <button
                      type="button"
                      className={cn('flex min-w-0 flex-1 items-center gap-1 truncate text-left hover:underline', s.on === false && 'text-muted-foreground line-through', !same(saved.steps[i], s) && 'text-edited')}
                      onClick={() => (setFormulaAt(null), setTableAt(i))}
                      title="Open this table script"
                    >
                      <Table2 className="size-3.5 shrink-0 text-muted-foreground" />
                      {s.step.replaceAll('_', ' ')}
                    </button>
                  </div>
                );
              }
              if (s.formula) {
                // A formula step (decision 0077): opened in the formula editor on the right.
                return (
                  <div key={i} className={cn('mb-1 flex items-center gap-1 rounded-sm', formulaAt === i && 'bg-selection')} data-step={s.step} data-formula="yes">
                    <Choice value={s.on === false ? 'off' : 'on'} options={['on', 'off'] as const} label={`${s.step} on or off`} onChange={(v) => setStep({ on: v === 'on' ? undefined : false })} />
                    <button
                      type="button"
                      className={cn('flex min-w-0 flex-1 items-center gap-1 truncate text-left hover:underline', s.on === false && 'text-muted-foreground line-through', !same(saved.steps[i], s) && 'text-edited')}
                      onClick={() => (setTableAt(null), setFormulaAt(i))}
                      title="Open this formula"
                    >
                      <Braces className="size-3.5 shrink-0 text-muted-foreground" />
                      {s.step.replaceAll('_', ' ')}
                    </button>
                  </div>
                );
              }
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
            {FORMULA_STAGES.some((a) => flow.steps.some((s) => s.step === a && !s.formula)) && (
              <Select className="mt-1 w-full" value="" aria-label="Add a formula step" onChange={(e) => e.target.value && addFormula(e.target.value)}>
                <option value="">Add a formula after…</option>
                {FORMULA_STAGES.filter((a) => flow.steps.some((s) => s.step === a && !s.formula)).map((a) => (
                  <option key={a} value={a}>
                    {a.replaceAll('_', ' ')}
                  </option>
                ))}
              </Select>
            )}
            {flow.steps.some((s) => s.step === 'final' && !s.formula && !s.table) && (
              <button type="button" data-slot="add-table-step" className="mt-1 block w-full py-0.5 text-left text-muted-foreground hover:bg-accent hover:text-foreground" onClick={addTableStep} title="A table script run on the case's table after the final writes, every run: its columns are the run's own">
                + Add a table script after final
              </button>
            )}
          </section>
        </div>
        {tableAt !== null && flow.steps[tableAt]?.table ? (
          <section className="min-w-0 flex-1">
            <TableStepEditor
              key={tableAt}
              step={flow.steps[tableAt]}
              saved={saved.steps.find((s) => s.step === flow.steps[tableAt].step)}
              onStep={(s) => set({ steps: flow.steps.map((t, j) => (j === tableAt ? s : t)) })}
              onRemove={() => (set({ steps: flow.steps.filter((_, j) => j !== tableAt) }), setTableAt(null))}
              onClose={() => setTableAt(null)}
            />
          </section>
        ) : formulaAt !== null && flow.steps[formulaAt]?.formula ? (
          <section className="min-w-0 flex-1">
            <FormulaEditor
              flow={flow}
              flowId={object.id}
              index={formulaAt}
              saved={saved.steps.find((s) => s.step === flow.steps[formulaAt].step)}
              onStep={(s) => set({ steps: flow.steps.map((t, j) => (j === formulaAt ? s : t)) })}
              onRemove={() => (set({ steps: flow.steps.filter((_, j) => j !== formulaAt) }), setFormulaAt(null))}
              onClose={() => setFormulaAt(null)}
            />
          </section>
        ) : (
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
        )}
      </div>
    </div>
  );
}
