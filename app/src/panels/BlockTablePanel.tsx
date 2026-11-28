import { Download, EyeOff, FileUp, MousePointerSquareDashed, Scan, Trash2 } from 'lucide-react';
import { type ReactNode, useEffect, useMemo, useState } from 'react';
import { briefCheck, type CalculatedColumn, concealed, type Grid, type GridRow, type ObjectKind, type ObjectsCalculated, objectsCalculated, solidsTable, tableGrid } from '@/bridge';
import { columnHelper, DataTable } from '@/design-system/DataTable';
import { Choice, EditableCell, Field, FilterInput, GroupBy, NumberInput, ok, refused, type Said, SaidLine, ScriptEditor, Select, TextInput } from '@/design-system/kit';
import type { When } from '@/filters';
import { Button } from '@/design-system/ui/button';
import { cn } from 'cn';
import { useWorkspace } from '@/workspace/context';
import { attributeColumns, csvOf, densityColumns, filterable, shownRows, type SolidRow, totalsOf } from './blockTable';
import { fmt, kindLabel } from './format';
import { IN_TREE } from './layerTree';
import { UpdateRecords } from './UpdateRecords';
import { CalculatedEditor } from './CalculatedEditor';
import { TableGrid } from './TableGridView';
import { type Entry, gridCsv, groupsKept, nextSort, pastedLines, readEntry, type Sort, toggleOpen } from './tableGrid';
import { CopyForAi } from './CopyForAi';

/**
 * The Objects table (decision 0069 as the block table; Filtering plan, L5): objects as rows, solids by default, each
 * with every attribute and a solid with its own volume, to see, sort and filter. Now the layer tree has a row a layer
 * and none an object, this is where one is found by name. Taken from a layer, from what's picked, or from what's on
 * show. A click on a row picks it; Select shown picks every row the filter keeps, Zoom to and Hide shown act on them in
 * the view, and Delete shown deletes them in one run, one undo, as at a pause to take off what's under 50 bcm. The
 * totals add up what's shown: how many, their volume and, by a density column, tonnes.
 *
 * Grouped (Tables and scripts plan, decision 1), the rows shown are a table like any other: a subtotal a group, the
 * volume and tonnes summed and the density added up by volume, in the engine, as a reserves table's are. `picker` is the
 * Tables tab's Table list, at the head of the row of controls.
 *
 * An attribute's cells are typed in, or a column pasted down them (decision 4): each change is one
 * `object.set_attributes` run, one undo, the solid's mesh and volume as they were. + Attribute adds a column to type.
 */

type Take = 'layer' | 'picked' | 'shown';
const TAKES: readonly Take[] = ['layer', 'picked', 'shown'];
const TAKE_LABELS: Record<Take, string> = { layer: 'A layer', picked: 'Picked', shown: 'On show' };

/** Which kind of object the rows are: solids, as the block table had, or the others the tree has no row for. */
type Kind = 'solid' | 'string' | 'surface' | 'grid' | 'all';
const KINDS: readonly Kind[] = ['solid', 'string', 'surface', 'grid', 'all'];
const KIND_LABELS: Record<Kind, string> = { solid: 'Solids', string: 'Strings', surface: 'Surfaces', grid: 'Grids', all: 'All' };
const KIND_WORDS: Record<Kind, [string, string]> = { solid: ['solid', 'solids'], string: ['string', 'strings'], surface: ['surface', 'surfaces'], grid: ['grid', 'grids'], all: ['object', 'objects'] };

const helper = columnHelper<SolidRow>();

/** Columns the engine's objects table keeps for itself: an attribute of one of these names isn't a column of its own. */
const OWN = new Set(['name', 'layer', 'kind', 'volume', 'tonnes']);

/** An object script to start from (Table setup plan, decision 10): what it sets becomes each block's attribute. */
const STARTER_OBJECT_SCRIPT = `def formula(row, p):
    # What it sets becomes the block's attribute: words where the line gives words, else a number.
    row.process = "dragline" if row.horizon == "OB" else "loader"
`;

export function BlockTable({ picker }: { picker?: ReactNode }) {
  const { summary, selection, setSelection, runRecord, zoomTo, setObjectsVisible } = useWorkspace();
  const [kind, setKind] = useState<Kind>('solid');
  const [take, setTake] = useState<Take>('layer');
  const [layerId, setLayerId] = useState<string | null>(null);
  const [words, setWords] = useState('');
  const [when, setWhen] = useState<When>({});
  const [under, setUnder] = useState(0);
  const [density, setDensity] = useState<string | null>(null);
  const [rows, setRows] = useState<SolidRow[]>([]);
  const [confirming, setConfirming] = useState(false);
  const [group, setGroup] = useState<string[]>([]);
  const [open, setOpen] = useState<string[][]>([]);
  const [sort, setSort] = useState<Sort>(null);
  const [grid, setGrid] = useState<Grid | null>(null);
  /** Attributes added to type in, before any object has one. */
  const [extra, setExtra] = useState<string[]>([]);
  const [naming, setNaming] = useState<string | null>(null);
  /** Update records from a CSV, its strip open under the head (decision 5). */
  const [updating, setUpdating] = useState(false);
  const [said, setSaid] = useState<Said | null>(null);
  /** The Objects table's calculated columns (Table setup plan, decision 9): worked out on the rows, never stored. */
  const [calc, setCalc] = useState<{ ids: string[]; of: ObjectsCalculated } | null>(null);
  const [calcOpen, setCalcOpen] = useState<{ editing: CalculatedColumn | null } | null>(null);
  /** An object script (decision 10), being written, run on the solids shown. */
  const [scripting, setScripting] = useState<string | null>(null);

  const solids = useMemo(() => (summary?.objects ?? []).filter((o) => IN_TREE(o) && (kind === 'all' || o.kind === kind)), [summary, kind]);
  const [one, many] = KIND_WORDS[kind];
  const kinds = useMemo(() => new Map((summary?.objects ?? []).map((o) => [o.id, o.kind as ObjectKind])), [summary]);
  const layerName = useMemo(() => new Map((summary?.layers ?? []).map((l) => [l.id, l.name])), [summary]);
  // Layers holding solids, those with blocks first: a cut into blocks is what the table is mostly for.
  const solidLayers = useMemo(() => {
    const ids = [...new Set(solids.map((o) => o.layer))];
    const hasBlocks = (id: string) => solids.some((o) => o.layer === id && 'block' in o.attributes);
    return ids.sort((a, b) => Number(hasBlocks(b)) - Number(hasBlocks(a)));
  }, [solids]);
  const layer = layerId && solidLayers.includes(layerId) ? layerId : (solidLayers[0] ?? null);

  const taken = useMemo(() => {
    const visible = new Map((summary?.layers ?? []).map((l) => [l.id, l.visible]));
    if (take === 'layer') return solids.filter((o) => o.layer === layer);
    if (take === 'picked') return solids.filter((o) => selection.includes(o.id));
    return solids.filter((o) => visible.get(o.layer) && !concealed(o));
  }, [take, layer, solids, selection, summary]);

  // Volumes and every attribute, read in one call whenever what's taken, or any of it, changes.
  const key = taken.map((o) => `${o.id}@${o.revision}`).join(',');
  useEffect(() => {
    let live = true;
    if (taken.length === 0) {
      setRows([]);
      return;
    }
    // A solid's volume and attributes are read from the engine; anything else carries its attributes in the summary.
    const solidIds = taken.filter((o) => o.kind === 'solid').map((o) => o.id);
    void (solidIds.length > 0 ? solidsTable(solidIds) : Promise.resolve([])).then((list) => {
      if (!live) return;
      const read = new Map(solidIds.map((id, k) => [id, list[k]]));
      setRows(
        taken.map((o) => {
          const solid = read.get(o.id);
          return { id: o.id, name: o.name, layer: layerName.get(o.layer) ?? '', volume: solid?.volume ?? NaN, attributes: solid?.attributes ?? o.attributes };
        }),
      );
    });
    return () => {
      live = false;
    };
    // `key` stands for `taken`: the same solids at the same revisions read the same.
  }, [key, layerName]);

  // The calculated columns' values on the solids taken, again whenever they or the Objects table's own change.
  const ownRevision = summary?.objects.find((o) => o.objects_own)?.revision ?? 0;
  useEffect(() => {
    let live = true;
    const ids = taken.filter((o) => o.kind === 'solid').map((o) => o.id);
    if (ids.length === 0) {
      setCalc(null);
      return;
    }
    void objectsCalculated(ids)
      .then((of) => live && setCalc({ ids, of }))
      .catch(() => live && setCalc(null));
    return () => {
      live = false;
    };
  }, [key, ownRevision]);
  const calcOf = useMemo(() => {
    const at = new Map((calc?.ids ?? []).map((id, k) => [id, k]));
    return (id: string, column: number): number | null => {
      const k = at.get(id);
      return k === undefined ? null : (calc?.of.values[column]?.[k] ?? null);
    };
  }, [calc]);
  const calcColumns = useMemo(() => calc?.of.columns ?? [], [calc]);

  const columns = useMemo(() => {
    const have = attributeColumns(rows);
    return [...have, ...extra.filter((k) => !have.some((c) => c.key === k)).map((key) => ({ key, numeric: false }))];
  }, [rows, extra]);
  const numericOf = useMemo(() => new Map(columns.map((c) => [c.key, c.numeric && rows.some((r) => typeof r.attributes[c.key] === 'number')])), [columns, rows]);
  /**
   * Values typed or pasted, each read for its attribute: a column of numbers takes numbers; one of words, words; a new
   * one, a number where it reads as one. One run sets them all.
   */
  const setAttributes = async (key: string, cells: { id: string; text: string }[]) => {
    if (OWN.has(key) || cells.length === 0) return;
    const numeric = numericOf.get(key) ?? false;
    const fresh = !rows.some((r) => key in r.attributes);
    const read: { object: string; attribute: string; value: Entry }[] = [];
    for (const c of cells) {
      const asNumber = readEntry(c.text, true);
      const r = numeric ? asNumber : fresh && 'value' in asNumber ? asNumber : readEntry(c.text, false);
      if ('error' in r) return setSaid(refused(`${key}: ${r.error}`));
      read.push({ object: c.id, attribute: key, value: r.value });
    }
    const outcome = await runRecord('object.set_attributes', { cells: read });
    if (outcome.error) return setSaid(refused(outcome.error));
    setSaid(ok(`${outcome.record?.messages?.[0] ?? 'Set'}; one undo puts them back`));
  };
  const withVolume = kind === 'solid' || (kind === 'all' && rows.some((r) => Number.isFinite(r.volume)));
  const densities = useMemo(() => densityColumns(columns), [columns]);
  const rd = density && densities.includes(density) ? density : (densities[0] ?? null);
  const shown = useMemo(() => shownRows(rows, { words, under, when }), [rows, words, under, when]);
  const filterRows = useMemo(() => rows.map(filterable), [rows]);
  const totals = useMemo(() => totalsOf(shown, rd), [shown, rd]);
  const picked = useMemo(() => new Set(selection), [selection]);

  // Grouped: the rows shown as the engine's objects table, added up a group at a time.
  const groupOptions = useMemo(() => ['layer', ...(kind === 'all' ? ['kind'] : []), ...columns.filter((c) => !c.numeric && !OWN.has(c.key)).map((c) => c.key)], [columns, kind]);
  const groupBy = groupsKept(group, groupOptions);
  const shownKey = shown.map((r) => r.id).join(',');
  const gridColumns = useMemo(
    () => [...(withVolume ? ['volume'] : []), ...(withVolume && rd ? ['tonnes'] : []), ...columns.filter((c) => !OWN.has(c.key)).map((c) => c.key), ...(kind === 'solid' ? calcColumns.map((c) => c.name) : [])],
    [withVolume, rd, columns, kind, calcColumns],
  );
  useEffect(() => {
    if (groupBy.length === 0 || shown.length === 0) return setGrid(null);
    let live = true;
    // A moment after the last change, so typing in the filter doesn't ask the engine at every key.
    const timer = setTimeout(() => {
      void tableGrid({ of: 'objects', objects: shown.map((r) => r.id), density: rd }, { group: groupBy, open, columns: gridColumns, sort })
        .then((g) => live && setGrid(g))
        .catch((e) => live && setSaid(refused(String(e))));
    }, 150);
    return () => {
      live = false;
      clearTimeout(timer);
    };
    // `shownKey` stands for the rows shown.
  }, [shownKey, groupBy.join('\u0001'), open, sort, gridColumns.join('\u0001'), rd]);
  const idOf = (r: GridRow) => (r.row === null ? null : (shown[r.row]?.id ?? null));

  const tableColumns = useMemo(
    () => [
      helper.accessor('name', { header: 'Name', cell: (c) => <span title={c.getValue()}>{c.getValue()}</span> }),
      ...(kind === 'all' ? [helper.accessor((r) => kindLabel[kinds.get(r.id) ?? 'solid'], { id: 'kind', header: 'Kind' })] : []),
      ...(withVolume ? [helper.accessor('volume', { header: 'Volume m³', cell: (c) => <span className="block text-right tabular-nums">{Number.isFinite(c.getValue()) ? fmt(c.getValue()) : ''}</span> })] : []),
      ...columns.map((col) =>
        helper.accessor((r) => r.attributes[col.key] ?? (col.numeric ? Number.NEGATIVE_INFINITY : ''), {
          id: `a:${col.key}`,
          header: col.key,
          cell: (c) => {
            const v = c.row.original.attributes[col.key];
            const shown = v === undefined ? '' : typeof v === 'number' ? (Number.isInteger(v) ? String(v) : fmt(v, 3)) : String(v);
            if (OWN.has(col.key)) return typeof v === 'number' ? <span className="block text-right tabular-nums">{shown}</span> : shown;
            return (
              <EditableCell
                shown={shown}
                raw={v === undefined ? '' : String(v)}
                numeric={typeof v === 'number'}
                label={`${col.key} of ${c.row.original.name}`}
                onCommit={(text) => void setAttributes(col.key, [{ id: c.row.original.id, text }])}
                onPasteLines={(text) => {
                  // Down the rows as they're shown, sorted or not, from this one.
                  const order = c.table.getRowModel().rows.map((r) => r.original.id);
                  const from = order.indexOf(c.row.original.id);
                  const lines = pastedLines(text);
                  void setAttributes(
                    col.key,
                    order.slice(from, from + lines.length).map((id, i) => ({ id, text: lines[i] })),
                  );
                }}
              />
            );
          },
        }),
      ),
      // The Objects table's calculated columns (decision 9): worked out, read only, ƒ in the head to open the formula.
      ...(kind === 'solid'
        ? calcColumns.map((cc, k) =>
            helper.accessor((r) => calcOf(r.id, k) ?? Number.NEGATIVE_INFINITY, {
              id: `c:${cc.name}`,
              header: () => (
                <button type="button" className="italic hover:underline" title={`${cc.name}: calculated on each solid's row, never stored; open its formula`} onClick={() => setCalcOpen({ editing: cc })}>
                  ƒ {cc.name}
                </button>
              ),
              cell: (c) => {
                const v = calcOf(c.row.original.id, k);
                return <span className="block text-right italic tabular-nums">{v === null ? '' : fmt(v, 3)}</span>;
              },
            }),
          )
        : []),
      ...(take === 'layer' ? [] : [helper.accessor('layer', { header: 'Layer' })]),
    ],
    [columns, take, kind, kinds, withVolume, numericOf, rows, calcColumns, calcOf],
  );

  // The solid picked last, to run a calculated column's formula on, line by line.
  const pickedSolid = useMemo(() => {
    const id = selection[selection.length - 1];
    const r = id ? rows.find((x) => x.id === id) : undefined;
    return r ? { row: 0, label: r.name, solid: r.id } : null;
  }, [selection, rows]);

  const deleteShown = async () => {
    setConfirming(false);
    const ids = shown.map((r) => r.id);
    const outcome = await runRecord('object.delete_many', { objects: ids });
    if (outcome.error) return setSaid(refused(outcome.error));
    setSelection([]);
    setSaid(ok(`${fmt(ids.length)} ${ids.length === 1 ? one : many} deleted${withVolume ? `, ${fmt(totals.volume)} m³` : ''}; one undo brings them back`));
  };

  const saveCsv = () => {
    const link = document.createElement('a');
    link.href = URL.createObjectURL(new Blob([grid && groupBy.length > 0 ? gridCsv(grid) : csvOf(shown, columns)], { type: 'text/csv' }));
    link.download = `${take === 'layer' ? (layerName.get(layer ?? '') ?? many) : take === 'picked' ? `picked ${many}` : `${many} on show`}.csv`;
    link.click();
    URL.revokeObjectURL(link.href);
  };

  return (
    <div className="flex h-full min-h-0 flex-col text-xs" data-tool-panel="blocks">
      <div className="flex shrink-0 flex-wrap items-center gap-x-3 gap-y-1 border-b border-border px-2 py-1">
        {picker}
        <Field label="Of">
          <Choice value={kind} options={KINDS} labels={KIND_LABELS} onChange={setKind} label="Which kind of object" />
        </Field>
        <Field label="Take">
          <Choice value={take} options={TAKES} labels={TAKE_LABELS} onChange={setTake} label={`Which ${many}`} />
        </Field>
        {take === 'layer' && (
          <Select value={layer ?? ''} aria-label={`Layer of ${many}`} onChange={(e) => setLayerId(e.target.value)}>
            {solidLayers.length === 0 && <option value="">No {many}</option>}
            {solidLayers.map((id) => (
              <option key={id} value={id}>
                {layerName.get(id)}
              </option>
            ))}
          </Select>
        )}
        <TextInput className="w-36" value={words} placeholder="Words: 3 M coal…" label="Filter the rows by words" onChange={(value) => setWords(value)} />
        <FilterInput rows={filterRows} value={when} onChange={setWhen} label="Filter the rows by attributes" className="w-72" />
        {withVolume && (
          <Field label="Under" hint="Only rows under this volume; 0 for any">
            <NumberInput value={under} onChange={setUnder} label="Volume under" unit="m³" width="w-24" />
          </Field>
        )}
        {naming === null ? (
          <button type="button" className="whitespace-nowrap text-muted-foreground hover:text-foreground hover:underline" title="A new attribute to type in for these objects" onClick={() => setNaming('')}>
            + Attribute
          </button>
        ) : (
          <TextInput size="sm"
            autoFocus
            label="The new attribute's name"
            placeholder="name, Enter"
            className="w-28"
            value={naming}
            onChange={(value) => setNaming(value)}
            onBlur={() => setNaming(null)}
            onKeyDown={(e) => {
              if (e.key === 'Escape') setNaming(null);
              if (e.key !== 'Enter') return;
              const name = naming.trim();
              if (name && !OWN.has(name)) setExtra((x) => (x.includes(name) ? x : [...x, name]));
              setNaming(null);
            }}
          />
        )}
        {kind === 'solid' && (
          <>
            <button type="button" className="whitespace-nowrap text-muted-foreground hover:text-foreground hover:underline" title="A column worked out on each solid's row by a formula, never stored: tonnes as volume by RD, a waste class from ash" onClick={() => setCalcOpen({ editing: null })}>
              + Calculated
            </button>
            <button
              type="button"
              className="whitespace-nowrap text-muted-foreground hover:text-foreground hover:underline"
              title="An object script on the solids shown: def formula(row, p), what it sets becoming their attributes, one undo"
              onClick={() => setScripting((s) => (s === null ? STARTER_OBJECT_SCRIPT : null))}
            >
              Run a script on blocks…
            </button>
          </>
        )}
        <GroupBy
          value={groupBy}
          options={groupOptions}
          onChange={(next) => {
            setGroup(next);
            setOpen([]);
          }}
        />
        {densities.length > 0 && (
          <Field label="Density">
            <Select value={rd ?? ''} aria-label="Density column" onChange={(e) => setDensity(e.target.value)}>
              {densities.map((d) => (
                <option key={d} value={d}>
                  {d}
                </option>
              ))}
            </Select>
          </Field>
        )}
      </div>
      <div className="flex shrink-0 flex-wrap items-center gap-x-3 gap-y-1 border-b border-border px-2 py-1" role="status" aria-label="Totals of the rows shown">
        <span>
          <b className="tabular-nums">{fmt(totals.count)}</b> of {fmt(rows.length)} shown
          {withVolume && (
            <>
              {' '}· <b className="tabular-nums">{fmt(totals.volume)}</b> m³
            </>
          )}
          {totals.tonnes !== null && (
            <>
              {' '}· <b className="tabular-nums">{fmt(totals.tonnes)}</b> t by {rd}
              {totals.weighed < totals.count && <span className="text-muted-foreground"> ({fmt(totals.weighed)} rows with one)</span>}
            </>
          )}
        </span>
        <span className="ml-auto flex items-center gap-1">
          <Button variant="outline" size="sm" className="h-6 px-2 text-xs" disabled={shown.length === 0} title="Pick every row shown, lit in the view" onClick={() => setSelection(shown.map((r) => r.id))}>
            <MousePointerSquareDashed className="size-3.5" /> Select shown
          </Button>
          <Button variant="outline" size="sm" className="h-6 px-2 text-xs" disabled={shown.length === 0} title="Frame every row shown in the view" onClick={() => zoomTo(shown.map((r) => r.id))}>
            <Scan className="size-3.5" /> Zoom to
          </Button>
          <Button variant="outline" size="sm" className="h-6 px-2 text-xs" disabled={shown.length === 0} title="Hide every row shown in the view; Show all hidden on the view's right-click brings them back" onClick={() => setObjectsVisible(shown.map((r) => r.id), false)}>
            <EyeOff className="size-3.5" /> Hide shown
          </Button>
          {confirming ? (
            <>
              <Button variant="destructive" size="sm" className="h-6 px-2 text-xs" onClick={() => void deleteShown()}>
                Delete {fmt(shown.length)}
              {withVolume ? `, ${fmt(totals.volume)} m³` : ''}
              </Button>
              <Button variant="ghost" size="sm" className="h-6 px-2 text-xs" onClick={() => setConfirming(false)}>
                Keep
              </Button>
            </>
          ) : (
            <Button variant="outline" size="sm" className="h-6 px-2 text-xs" disabled={shown.length === 0} title="Delete every row shown, in one run: one undo brings them back" onClick={() => setConfirming(true)}>
              <Trash2 className="size-3.5" /> Delete shown
            </Button>
          )}
          <Button variant="outline" size="sm" className="h-6 px-2 text-xs" disabled={shown.length === 0} title="The rows shown as CSV, exact values" onClick={saveCsv}>
            <Download className="size-3.5" /> CSV
          </Button>
          <Button
            variant="outline"
            size="sm"
            className={cn('h-6 px-2 text-xs', updating && 'border-primary/70 text-primary')}
            disabled={shown.length === 0}
            title="Set attributes on the rows shown from a CSV, matched on key columns (block, or strip, block and seam), with what it matches and changes shown first"
            onClick={() => setUpdating((u) => !u)}
          >
            <FileUp className="size-3.5" /> Update records…
          </Button>
        </span>
        <SaidLine said={said} inline className="w-full" />
      </div>
      {updating && (
        <UpdateRecords
          target={{ of: 'objects', objects: shown.map((r) => r.id) }}
          keys={['name', 'pit', 'strip', 'block', 'seam', 'horizon', 'bench', 'material', 'case']}
          what={[one, many]}
          onClose={() => setUpdating(false)}
        />
      )}
      {calcOpen && (
        <CalculatedEditor
          key={calcOpen.editing?.name ?? '+'}
          table={null}
          numbers={['volume', ...(rd ? ['tonnes'] : []), ...columns.filter((c) => c.numeric && !OWN.has(c.key)).map((c) => c.key)]}
          editing={calcOpen.editing}
          picked={pickedSolid}
          onClose={() => setCalcOpen(null)}
        />
      )}
      {calc && calc.of.said.length > 0 && kind === 'solid' && (
        <div className="px-2 py-1 text-warning" data-slot="objects-calc-said">
          {calc.of.said.map((s) => (
            <div key={s.column}>
              {s.column}: no value on {fmt(s.stopped)} solid{s.stopped === 1 ? '' : 's'}
              {s.first ? `, the first ${s.first}` : ''}
            </div>
          ))}
        </div>
      )}
      {scripting !== null && <ObjectScriptStrip source={scripting} onChange={setScripting} ids={shown.filter((r) => kinds.get(r.id) === 'solid').map((r) => r.id)} onClose={() => setScripting(null)} />}
      <div className="min-h-0 flex-1">
        {groupBy.length > 0 ? (
          grid && (
            <TableGrid
              grid={grid}
              sort={sort}
              onSort={(c) => setSort((s) => nextSort(s, c))}
              onToggle={(p) => setOpen((o) => toggleOpen(o, p))}
              onRow={(r) => {
                const id = idOf(r);
                if (id) setSelection([id]);
              }}
              lit={(r) => {
                const id = idOf(r);
                return id !== null && picked.has(id);
              }}
              rowWords={[one, many]}
              editable={(r, c) => r.kind === 'leaf' && !OWN.has(c.name) && c.kind !== 'calculated'}
              onEditCalc={(c) => c.calculated && setCalcOpen({ editing: c.calculated })}
              onEdit={(c, cells) =>
                void setAttributes(
                  c.name,
                  cells.flatMap(({ row, text }) => {
                    const id = idOf(row);
                    return id ? [{ id, text }] : [];
                  }),
                )
              }
            />
          )
        ) : (
        <DataTable
          data={shown}
          columns={tableColumns}
          rowId={(r) => r.id}
          rowClassName={(r) => (picked.has(r.id) ? 'bg-selection' : undefined)}
          onRowClick={(r) => setSelection([r.id])}
          empty={take === 'picked' ? `Pick ${many} in the view to list them` : `No ${many} to list`}
          windowed={[undefined, ...(kind === 'all' ? ['6rem'] : []), ...(withVolume ? ['7rem'] : []), ...columns.map(() => '6rem'), ...(take === 'layer' ? [] : ['8rem'])]}
        />
        )}
      </div>
    </div>
  );
}

/**
 * An object script (Table setup plan, decision 10) on the solids shown: `def formula(row, p):` on each one's row of the
 * Objects table, what it sets becoming its attributes, words or a number; never its name, layer, volume or geometry. One
 * run, one undo; a reserves table made before then says its blocks have changed.
 */
function ObjectScriptStrip({ source, onChange, ids, onClose }: { source: string; onChange: (s: string) => void; ids: string[]; onClose: () => void }) {
  const { runRecord } = useWorkspace();
  const [said, setSaid] = useState<Said | null>(null);
  const [running, setRunning] = useState(false);
  // Checked as it's typed against the solids shown, as a run would (decision 0104): each problem on its line.
  const [problems, setProblems] = useState<{ message: string; line: number | null }[]>([]);
  useEffect(() => {
    let live = true;
    const t = setTimeout(() => {
      void briefCheck({ kind: 'object', source })
        .then((p) => live && setProblems(p))
        .catch((e) => live && setProblems([{ message: String(e), line: null }]));
    }, 300);
    return () => {
      live = false;
      clearTimeout(t);
    };
  }, [source]);
  const run = async () => {
    setRunning(true);
    const outcome = await runRecord('object.script', { objects: ids, source });
    setRunning(false);
    setSaid(outcome.error ? refused(outcome.error) : ok(`${outcome.record?.messages?.[0] ?? 'Done'}; one undo puts them back`));
  };
  return (
    <div className="shrink-0 space-y-1 border-b border-border px-2 py-1" data-slot="object-script">
      <ScriptEditor value={source} onChange={onChange} problems={problems} label="Object script" className="max-h-48" />
      <div className="flex items-center gap-2">
        <Button size="sm" className="h-6 px-2 text-xs" disabled={running || ids.length === 0} onClick={() => void run()} title="Run it on every solid shown, one run: each it changes revised once">
          {running ? 'Running…' : `Run on the ${fmt(ids.length)} solid${ids.length === 1 ? '' : 's'} shown`}
        </Button>
        <Button variant="ghost" size="sm" className="h-6 px-2 text-xs" onClick={onClose}>
          Close
        </Button>
        <CopyForAi brief={{ kind: 'object', source }} problems={problems} />
        <SaidLine said={said} inline />
      </div>
    </div>
  );
}
