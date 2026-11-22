import { ChevronDown, ChevronRight, Columns3, Download, FileUp, Focus, MousePointerSquareDashed, Palette } from 'lucide-react';
import { Fragment, type ReactNode, useEffect, useMemo, useRef, useState } from 'react';
import { ReservesBalance } from './ReservesBalanceView';
import { type Breakdown, type ChangedSince, type Grid, type GridColumn, type GridRow, reservesBreakdown, reservesChanged, reservesFilterRows, reservesNodeOf, reservesRerunParams, reservesSolids, reservesTree, type ReservesNode, tableGrid } from '@/bridge';
import { type When, whenText } from '@/filters';
import { Choice, Field, FilterInput, GroupBy, IconButton, ObjectMenu } from '@/design-system/kit';
import { Button } from '@/design-system/ui/button';
import { Checkbox } from '@/design-system/ui/checkbox';
import { cn } from 'cn';
import { useWorkspace } from '@/workspace/context';
import { asPutBack, breakdownCsv, cellText, fieldRows, type Isolation, isolation, nodeForLeaf, samePath, startsWith, unitLabel } from './reservesTable';
import { fmt } from './format';
import { TableGrid } from './TableGridView';
import { UpdateRecords } from './UpdateRecords';
import { changedWords, type Entry, gridCsv, groupsKept, nextSort, readEntry, type Sort, toggleOpen as toggleGroup } from './tableGrid';

/**
 * The reserves table (decision 0075, workstream 5a of 0070), the fields as rows as Tim asked: a node picked in the tree on
 * the left, its fields down the side by folder (Imported, Insitu, Loss, Dilution, ROM, Scheduled, …) and across its
 * total then its children, a child opening to its own. Read down a column for one node from Imported to Scheduled, across
 * a row for a field broken down. Every value is the engine's add-up of the table's leaves by the field's own rule.
 */

type Fields = 'key' | 'all';
const FIELDS: readonly Fields[] = ['key', 'all'];
const FIELD_LABELS: Record<Fields, string> = { key: 'Main fields', all: 'Every field' };
type Follow = 'on' | 'off';
const FOLLOWS: readonly Follow[] = ['on', 'off'];
const FOLLOW_LABELS: Record<Follow, string> = { on: 'Follow the pick', off: 'Stay' };

// Across a node: its children (each opening to its own), or every leaf under it top of the stack to the bottom, no
// subtotals between (Tim, 16 November 2026).
type Across = 'children' | 'leaves';
const ACROSS: readonly Across[] = ['children', 'leaves'];
const ACROSS_LABELS: Record<Across, string> = { children: 'Children', leaves: 'Leaves' };

// The table as a grid of its leaves grouped (Tables and scripts plan, decision 1), the node as fields down the side and
// children across, or as its balance of coal and waste (decision 0086).
type View = 'grid' | 'fields' | 'balance';
const VIEWS: readonly View[] = ['grid', 'fields', 'balance'];
const VIEW_LABELS: Record<View, string> = { grid: 'Grid', fields: 'Breakdown', balance: 'Balance' };
const VIEW_KEY = 'zd.reservesTable.view.v1';

function readView(): View {
  try {
    const v = localStorage.getItem(VIEW_KEY);
    return v === 'grid' || v === 'balance' ? v : 'fields';
  } catch {
    return 'fields';
  }
}

function writeView(view: View) {
  try {
    localStorage.setItem(VIEW_KEY, view);
  } catch {
    // Storage unavailable: the Breakdown again next time.
  }
}

const FOLDED_KEY = 'zd.reservesTable.folded.v1';
const ACROSS_KEY = 'zd.reservesTable.across.v1';

function readAcross(): Across {
  try {
    return localStorage.getItem(ACROSS_KEY) === 'leaves' ? 'leaves' : 'children';
  } catch {
    return 'children';
  }
}

function writeAcross(across: Across) {
  try {
    localStorage.setItem(ACROSS_KEY, across);
  } catch {
    // Storage unavailable: children again next time.
  }
}

function readFolded(): Set<string> {
  try {
    return new Set(JSON.parse(localStorage.getItem(FOLDED_KEY) ?? '[]') as string[]);
  } catch {
    return new Set();
  }
}

function writeFolded(folded: Set<string>) {
  try {
    localStorage.setItem(FOLDED_KEY, JSON.stringify([...folded]));
  } catch {
    // Storage unavailable: the folders open again next time.
  }
}

const keyOf = (path: string[]) => path.join('\u0001');

/**
 * One reserves table in the Tables tab (Tables and scripts plan, decision 1): `picker` is the tab's Table list, shown at
 * the head of the side column; `onDeleted` says the table went from its menu.
 */
export function ReservesTable({ tableId, picker, onDeleted }: { tableId: string; picker: ReactNode; onDeleted: () => void }) {
  const { summary, selection, setSelection, openNotebook, followInNotebook, colourBlocksByField, fieldColoured, zoomTo, setObjectsVisible, setLayersVisible, setReservesFilter, runRecord } = useWorkspace();
  const last = selection[selection.length - 1] ?? null;
  const table = useMemo(() => (summary?.objects ?? []).find((o) => o.kind === 'table' && o.id === tableId) ?? null, [summary, tableId]);

  const [tree, setTree] = useState<ReservesNode | null>(null);
  const [expanded, setExpanded] = useState<Set<string>>(new Set([keyOf([])]));
  const [node, setNode] = useState<string[]>([]);
  const [open, setOpen] = useState<string[][]>([]);
  const [leaf, setLeaf] = useState<string[] | null>(null);
  const [breakdown, setBreakdown] = useState<Breakdown | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [fields, setFields] = useState<Fields>('key');
  const [follow, setFollow] = useState<Follow>('on');
  const [folded, setFolded] = useState<Set<string>>(readFolded);
  const [across, setAcrossState] = useState<Across>(readAcross);
  const setAcross = (a: Across) => {
    setAcrossState(a);
    writeAcross(a);
  };
  const [said, setSaid] = useState<string | null>(null);
  const [view, setViewState] = useState<View>(readView);
  const setView = (v: View) => {
    setViewState(v);
    writeView(v);
  };
  /** The grid: grouped by (the table's levels but its last until picked), groups opened, the sort, and what came back. */
  const [group, setGroup] = useState<string[] | null>(null);
  const [gridOpen, setGridOpen] = useState<string[][]>([]);
  const [gridSort, setGridSort] = useState<Sort>(null);
  const [grid, setGrid] = useState<Grid | null>(null);
  /** A new entered column being named (decision 2): its name and whether it holds words. */
  const [adding, setAdding] = useState<{ name: string; words: boolean } | null>(null);
  /** What the table was made from that has changed since its run (decision 4), and Run again going. */
  const [changed, setChanged] = useState<ChangedSince | null>(null);
  const [rerunning, setRerunning] = useState(false);
  /** Update records from a CSV into the table's entered columns, its strip open under the head (decision 5). */
  const [updating, setUpdating] = useState(false);
  /**
   * What's on screen alone, and what to put back: the table's layers (Show), or a node's solids (Isolate its solids) with
   * what it hid or showed. One at a time: a press of either puts the last back first.
   */
  const [alone, setAlone] = useState<{ key: string; layers: { id: string; visible: boolean }[]; changed: Isolation | null } | null>(null);
  /** The leaves the table adds up (decision 0088, workstream 4): levels to tick, fields typed. */
  const [when, setWhen] = useState<When>({});
  const [filterRows, setFilterRows] = useState<Record<string, boolean | number | string>[]>([]);
  const whenKey = whenText(when);
  /** The leaves filter carried to the view, and only the coal solids on the leaves kept (decision 0090). */
  const [inView, setInView] = useState(false);
  const [coalOnly, setCoalOnly] = useState(false);
  /** Every solid on the table's leaves: their layers are the table's. */
  const [tableSolids, setTableSolids] = useState<string[]>([]);

  const tableKey = table ? `${table.id}@${table.revision}` : '';
  useEffect(() => {
    let live = true;
    setFilterRows([]);
    if (table) void reservesFilterRows(table.id).then((rows) => live && setFilterRows(rows));
    return () => {
      live = false;
    };
  }, [tableKey]);
  useEffect(() => {
    let live = true;
    setTableSolids([]);
    if (table) void reservesSolids(table.id, []).then((ids) => live && setTableSolids(ids));
    return () => {
      live = false;
    };
  }, [tableKey]);
  // A table whose filter is in the view picks it up again: its leaves filter and its ticks.
  const viewFilter = summary?.display.reserves;
  useEffect(() => {
    const mine = !!table && viewFilter?.table === table.id;
    setInView(mine);
    setCoalOnly(mine && !!viewFilter?.coal_only);
    if (mine && viewFilter?.when) setWhen(viewFilter.when);
  }, [table?.id]);
  // The filter in the view follows the box as it's typed, a moment after the last change.
  useEffect(() => {
    if (!table) return;
    const mine = viewFilter?.table === table.id;
    if (!inView && !mine) return;
    const timer = setTimeout(() => void setReservesFilter(inView ? { table: table.id, when, coal_only: coalOnly } : null), 300);
    return () => clearTimeout(timer);
  }, [inView, coalOnly, whenKey, table?.id]);
  // Another table starts at All; the same one filtered keeps the node shown and what's open.
  const treeOf = useRef('');
  useEffect(() => {
    let live = true;
    const fresh = treeOf.current !== tableKey;
    treeOf.current = tableKey;
    if (fresh) {
      setTree(null);
      setNode([]);
      setOpen([]);
      setLeaf(null);
      setGridOpen([]);
    }
    if (!table) return;
    void reservesTree(table.id, when).then((t) => {
      if (!live) return;
      setTree(t);
      // The first level open, and its first node shown, so there's something to read.
      if (fresh) setExpanded(new Set([keyOf([]), ...t.children.slice(0, 1).map((c) => keyOf(c.path))]));
    });
    return () => {
      live = false;
    };
    // `tableKey` stands for the table and its revision, `whenKey` for the filter.
  }, [tableKey, whenKey]);

  const openKey = open.map(keyOf).join('|');
  useEffect(() => {
    let live = true;
    if (!table) return setBreakdown(null);
    void reservesBreakdown(table.id, node, open, across === 'leaves', when)
      .then((b) => {
        if (!live) return;
        setBreakdown(b);
        setError(null);
      })
      .catch((e) => live && setError(String(e)));
    return () => {
      live = false;
    };
  }, [tableKey, keyOf(node), openKey, across, whenKey]);

  // Across leaves, a line where the leaf's parent changes (one seam's benches from the next).
  const groupStarts = useMemo(() => {
    const starts = new Set<number>();
    if (!breakdown) return starts;
    breakdown.columns.forEach((c, i) => {
      if (c.depth !== 1) return;
      const before = breakdown.columns[i - 1];
      if (across === 'children' || !before || before.depth !== 1 || !samePath(before.path.slice(0, -1), c.path.slice(0, -1))) starts.add(i);
    });
    return starts;
  }, [breakdown, across]);

  // A solid picked in the view or the Objects table: its block, its seam opened, its leaf marked.
  useEffect(() => {
    if (!table || !breakdown || follow !== 'on' || !last || last === table.id) return;
    let live = true;
    void reservesNodeOf(table.id, last).then((found) => {
      if (!live || !found) return;
      const at = nodeForLeaf(breakdown.levels, found);
      setNode(at.node);
      setOpen(at.open);
      setLeaf(found);
      setExpanded((e) => new Set([...e, ...at.node.map((_, i) => keyOf(at.node.slice(0, i + 1))), keyOf([])]));
    });
    return () => {
      live = false;
    };
    // Only a new pick moves the table; the breakdown it lands on doesn't.
  }, [last, follow, tableKey, breakdown !== null]);

  const rows = useMemo(() => (breakdown ? fieldRows(breakdown, fields === 'all', folded) : []), [breakdown, fields, folded]);

  // The grid (Tables and scripts plan, decision 1): the fields the Breakdown shows, as columns, over every leaf the
  // filter keeps, grouped and added up in the engine by each field's own rule.
  // The entered columns follow the run's fields (decision 2): the grid's last answer lists them.
  const enteredKey = (grid?.every ?? []).filter((c) => c.kind === 'entered').map((c) => c.name).join('\u0001');
  const gridColumns = useMemo(() => [...rows.flatMap((r) => (r.kind === 'field' ? [r.key] : [])), ...(enteredKey ? enteredKey.split('\u0001') : [])], [rows, enteredKey]);
  const levelsNow = breakdown?.levels ?? [];
  const groupBy = group ?? levelsNow.slice(0, -1);
  const groupKey = groupBy.join('\u0001');
  const gridOpenKey = gridOpen.map(keyOf).join('|');
  const gridColumnsKey = gridColumns.join('\u0001');
  const gridSortKey = gridSort ? `${gridSort.column}|${gridSort.descending}` : '';
  useEffect(() => {
    if (view !== 'grid' || !table || !breakdown) return;
    let live = true;
    void tableGrid({ of: 'table', table: table.id }, { when, group: groupBy, open: gridOpen, columns: gridColumns, sort: gridSort })
      .then((g) => {
        if (!live) return;
        setGrid(g);
        setError(null);
      })
      .catch((e) => live && setError(String(e)));
    return () => {
      live = false;
    };
    // The keys stand for the lists they're made from.
  }, [view, tableKey, whenKey, groupKey, gridOpenKey, gridSortKey, gridColumnsKey, breakdown !== null]);
  // Grouped by the table's levels from the top, a group's words are a node of the tree: picking it shows that node.
  const groupIsLevels = groupBy.every((g, i) => levelsNow[i] === g);
  const pickRow = (r: GridRow) => {
    setSaid(null);
    if (r.kind === 'leaf' && r.words) {
      setNode(r.words);
      setOpen([]);
      setLeaf(r.words);
    } else if (groupIsLevels) {
      setNode(r.path);
      setOpen([]);
      setLeaf(null);
    }
  };
  // What's changed since the run, asked again whenever the project changes.
  useEffect(() => {
    let live = true;
    if (!table) return setChanged(null);
    void reservesChanged(table.id)
      .then((c) => live && setChanged(c))
      .catch(() => live && setChanged(null));
    return () => {
      live = false;
    };
  }, [tableKey, summary]);
  const runAgain = async () => {
    if (!table) return;
    setRerunning(true);
    try {
      const reply = await runRecord('reserves.run', await reservesRerunParams(table.id, []));
      setSaid(reply.error ?? 'Every block run again into the table: one undo puts it back');
    } catch (e) {
      setSaid(String(e));
    } finally {
      setRerunning(false);
    }
  };
  /** Values typed or pasted in an entered column, each read for it; one run enters them all. */
  const enter = async (c: GridColumn, cells: { row: GridRow; text: string }[]) => {
    if (!table) return;
    const read: { row: number; value: Entry }[] = [];
    for (const { row, text } of cells) {
      if (row.row === null) continue;
      const r = readEntry(text, !c.text);
      if ('error' in r) return setError(`${c.name}: ${r.error}`);
      read.push({ row: row.row, value: r.value });
    }
    const reply = await runRecord('table.enter', { table: table.id, column: c.name, cells: read });
    setError(reply.error ?? null);
  };
  const addColumn = async () => {
    if (!table || !adding) return;
    const name = adding.name.trim();
    setAdding(null);
    if (!name) return;
    const reply = await runRecord('table.enter', { table: table.id, column: name, words: adding.words, cells: [] });
    setError(reply.error ?? null);
  };
  const removeColumn = async (c: GridColumn) => {
    if (!table) return;
    const reply = await runRecord('table.remove_column', { table: table.id, column: c.name });
    setError(reply.error ?? null);
    if (!reply.error) setSaid(`${c.name} taken out: one undo puts it back`);
  };
  const changedLine = changed ? changedWords(changed) : '';
  const rowLit = (r: GridRow) => (r.kind === 'leaf' ? !!r.words && samePath(r.words, node) : groupIsLevels && r.kind === 'group' && samePath(r.path, node));
  // The fields of numbers by their folders, offered as the filter's typed (`ROM\Coal\ash`): never ticked, each leaf's its own.
  const fieldColumns = (breakdown?.fields ?? []).filter((f) => !f.words).map((f) => `${f.column}\t${f.unit}`).join('\n');
  const fieldNames = useMemo(
    () =>
      fieldColumns === ''
        ? []
        : fieldColumns.split('\n').map((line) => {
            const [key, unit] = line.split('\t');
            return { key, detail: unit || 'field' };
          }),
    [fieldColumns],
  );

  // The Dataflow tab follows the block shown here, or the seam or bench of it marked, with no press of Open its column.
  const levels = breakdown?.levels;
  const followKey = table ? `${table.id}|${keyOf(node)}|${leaf ? keyOf(leaf) : ''}|${levels?.join('|') ?? ''}` : '';
  useEffect(() => {
    if (!table || !levels) return;
    const seam = levels.findIndex((l) => l.toLowerCase() === 'seam');
    if (seam > 0 && node.length >= seam) followInNotebook(table.id, leaf && startsWith(leaf, node) ? leaf : node);
  }, [followKey]);

  const show = (path: string[]) => {
    setNode(path);
    setOpen([]);
    setLeaf(null);
    setSaid(null);
  };
  const toggleOpen = (path: string[]) => setOpen((o) => (o.some((p) => samePath(p, path)) ? o.filter((p) => !startsWith(p, path)) : [...o, path]));
  const toggleFolded = (key: string) =>
    setFolded((f) => {
      const next = new Set(f);
      if (next.has(key)) next.delete(key);
      else next.add(key);
      writeFolded(next);
      return next;
    });
  const tableLayers = useMemo(() => {
    const ids = new Set(tableSolids);
    const on = new Set((summary?.objects ?? []).filter((o) => ids.has(o.id)).map((o) => o.layer));
    return (summary?.layers ?? []).filter((l) => on.has(l.id));
  }, [tableSolids, summary]);
  const selectSolids = async () => {
    if (!table) return;
    const ids = await reservesSolids(table.id, node, when);
    setSelection(ids);
    setSaid(`${fmt(ids.length)} solid${ids.length === 1 ? '' : 's'} picked`);
  };
  const isolatedKey = table ? `node:${table.id}|${keyOf(node)}|${whenKey}` : '';
  const layersKey = table ? `layers:${table.id}` : '';
  /** Whatever is alone on screen put back: the layers as they were, and only what was hidden or shown changed back. */
  const putBack = async () => {
    if (!alone) return;
    const { layers, changed } = alone;
    setAlone(null);
    if (changed?.hide.length) await setObjectsVisible(changed.hide, true);
    if (changed?.unhide.length) await setObjectsVisible(changed.unhide, false);
    await setLayersVisible(
      layers.filter((l) => l.visible).map((l) => l.id),
      layers.filter((l) => !l.visible).map((l) => l.id),
    );
    setSaid('Shown as it was');
  };
  /** The layers and objects as they'll be once what's alone is put back: what the next Show or Isolate starts from. */
  const startFrom = () => asPutBack(summary?.objects ?? [], summary?.layers ?? [], alone);
  /** The node's solids (those on leaves the filter keeps) alone on screen, zoomed to; again, everything as it was. */
  const isolateSolids = async () => {
    if (!table || !summary) return;
    const was = alone?.key;
    const start = startFrom();
    await putBack();
    if (was === isolatedKey) return;
    const ids = await reservesSolids(table.id, node, when);
    if (ids.length === 0) return setSaid('No solids to isolate');
    const changed = isolation(ids, start.objects, start.layers);
    setAlone({ key: isolatedKey, layers: start.layers.map((l) => ({ id: l.id, visible: l.visible })), changed });
    if (changed.unhide.length) await setObjectsVisible(changed.unhide, true);
    if (changed.hide.length) await setObjectsVisible(changed.hide, false);
    await setLayersVisible(changed.showLayers, changed.hideLayers);
    zoomTo(ids);
    setSaid(`${fmt(ids.length)} solid${ids.length === 1 ? '' : 's'} alone on show`);
  };
  /** The table's solids' layers on and every other layer off; again, the layers as they were. */
  const showTableLayers = async () => {
    if (!table || tableLayers.length === 0) return;
    const was = alone?.key;
    const start = startFrom();
    await putBack();
    if (was === layersKey) return;
    const on = new Set(tableLayers.map((l) => l.id));
    setAlone({ key: layersKey, layers: start.layers.map((l) => ({ id: l.id, visible: l.visible })), changed: null });
    await setLayersVisible([...on], start.layers.filter((l) => l.visible && !on.has(l.id)).map((l) => l.id));
    setSaid(`${tableLayers.map((l) => l.name).join(', ')} alone on show`);
  };
  /** The leaves filter in the view: on, the table's layers are turned on too, so what it keeps is on show. */
  const filterTheView = async (on: boolean) => {
    setInView(on);
    if (!on) return setCoalOnly(false);
    const off = tableLayers.filter((l) => !l.visible).map((l) => l.id);
    if (off.length) await setLayersVisible(off, []);
  };
  const saveCsv = () => {
    if (!breakdown || !table) return;
    const link = document.createElement('a');
    const byGrid = view === 'grid' && grid !== null;
    link.href = URL.createObjectURL(new Blob([byGrid ? gridCsv(grid) : breakdownCsv(breakdown)], { type: 'text/csv' }));
    link.download = `${table.name}${byGrid ? (groupBy.length ? ` by ${groupBy.join(' ')}` : '') : node.length ? ` ${node.join(' ')}` : ''}${whenKey ? ` where ${whenKey.replace(/[\\/:*?"<>|]/g, ' ')}` : ''}.csv`;
    link.click();
    URL.revokeObjectURL(link.href);
  };

  if (!table) return null;

  // A column is a block: the levels above the seam. A node at a block or below it opens its block's column.
  const seamAt = breakdown ? breakdown.levels.findIndex((l) => l.toLowerCase() === 'seam') : -1;
  const canOpenColumn = seamAt > 0 && node.length >= seamAt;
  const columnNode = leaf && startsWith(leaf, node) ? leaf : node;
  const crumbs: { label: string; path: string[] }[] =[{ label: 'All', path: [] }, ...node.map((w, i) => ({ label: w || '(none)', path: node.slice(0, i + 1) }))];
  return (
    <div className="flex h-full min-h-0 text-xs" data-tool-panel="reserves-table">
      <div className="flex w-52 shrink-0 flex-col border-r border-border">
        <div className="flex items-center gap-1 border-b border-border px-2 py-1">
          {picker}
          <ObjectMenu object={table} what="reserves table" onDeleted={onDeleted} />
        </div>
        {tableLayers.length > 0 && (
          <div className="flex items-center gap-1 border-b border-border px-2 py-1" data-slot="reserves-layers">
            <span className="min-w-0 flex-1 truncate text-muted-foreground" title={`The table's solids are on ${tableLayers.map((l) => l.name).join(', ')}`}>
              Solids on <span className="text-foreground">{tableLayers.map((l) => l.name).join(', ')}</span>
            </span>
            <Button
              variant="outline"
              size="sm"
              className={cn('h-6 shrink-0 px-2 text-xs', alone?.key === layersKey && 'border-primary/70 text-primary')}
              title={alone?.key === layersKey ? 'Put the layers back as they were' : "Turn the table's solids' layer on and every other layer off; again, the layers as they were"}
              onClick={() => void showTableLayers()}
            >
              {alone?.key === layersKey ? 'Back' : 'Show'}
            </Button>
          </div>
        )}
        {view === 'grid' ? (
          <div className="min-h-0 flex-1 overflow-auto p-2">
            <GroupBy
              vertical
              value={groupBy}
              options={(grid?.groups ?? levelsNow).filter((g) => g !== 'case')}
              onChange={(next) => {
                setGroup(groupsKept(next, grid?.groups ?? levelsNow));
                setGridOpen([]);
              }}
            />
            <div className="mt-3 flex flex-col gap-1" data-slot="add-column">
              {adding === null ? (
                <button
                  type="button"
                  className="self-start text-muted-foreground hover:text-foreground hover:underline"
                  title="A column of your own beside the run's: typed or pasted on each leaf, kept by the leaf when the table runs again. The run's own numbers are never typed over"
                  onClick={() => setAdding({ name: '', words: false })}
                >
                  + Column
                </button>
              ) : (
                <>
                  <input
                    autoFocus
                    aria-label="The new column's name"
                    placeholder="name, Enter"
                    className="h-6 rounded-md border border-input bg-input/30 px-1.5 text-xs outline-none"
                    value={adding.name}
                    onChange={(e) => setAdding({ ...adding, name: e.target.value })}
                    onKeyDown={(e) => {
                      if (e.key === 'Escape') setAdding(null);
                      if (e.key === 'Enter') void addColumn();
                    }}
                  />
                  <Choice value={adding.words ? 'words' : 'numbers'} options={['numbers', 'words'] as const} labels={{ numbers: 'Numbers', words: 'Words' }} onChange={(v) => setAdding({ ...adding, words: v === 'words' })} label="What the new column holds" />
                </>
              )}
            </div>
          </div>
        ) : (
          <div className="min-h-0 flex-1 overflow-auto py-1" role="tree" aria-label="Levels">
            {tree && <TreeRow n={tree} depth={0} node={node} leaf={leaf} expanded={expanded} setExpanded={setExpanded} show={show} />}
          </div>
        )}
      </div>
      <div className="flex min-w-0 flex-1 flex-col">
        <div className="flex shrink-0 items-center gap-2 border-b border-border px-2 py-1" data-slot="reserves-filter">
          <span className="shrink-0 text-muted-foreground">Leaves</span>
          <FilterInput rows={filterRows} value={when} onChange={setWhen} label="Filter the leaves" names={fieldNames} className="max-w-md flex-1" />
          {tree && (
            <span className={cn('shrink-0 tabular-nums', Object.keys(when).length > 0 ? 'text-primary' : 'text-muted-foreground')} data-slot="reserves-filter-kept">
              {Object.keys(when).length > 0 ? `${fmt(tree.leaves)} of ${fmt(filterRows.length)} leaves` : `${fmt(tree.leaves)} leaves`}
            </span>
          )}
          <label
            className={cn('flex shrink-0 items-center gap-1.5', inView ? 'text-primary' : 'text-muted-foreground')}
            title="Draw only the solids on the leaves the filter keeps, on the table's layer, as a layer's filter does. Saved with the project; untick to draw them all again"
          >
            <Checkbox checked={inView} onCheckedChange={(c) => void filterTheView(c === true)} aria-label="Filter the view too" data-slot="reserves-in-view" />
            Filter the view too
          </label>
          <label
            className={cn('flex shrink-0 items-center gap-1.5', !inView ? 'text-muted-foreground/50' : coalOnly ? 'text-primary' : 'text-muted-foreground')}
            title="Only the coal solids on the leaves kept, by the material the seam cut gave them: a bench's waste isn't drawn. Coal the working sections reject still shows, as its solid is coal"
          >
            <Checkbox checked={coalOnly} disabled={!inView} onCheckedChange={(c) => setCoalOnly(c === true)} aria-label="Coal solids only" data-slot="reserves-coal-only" />
            Coal solids only
          </label>
        </div>
        <div className="flex shrink-0 flex-wrap items-center gap-x-3 gap-y-1 border-b border-border px-2 py-1">
          <span className="flex items-center gap-1" aria-label="Node shown">
            {crumbs.map((c, i) => (
              <Fragment key={keyOf(c.path)}>
                {i > 0 && <span className="text-muted-foreground">›</span>}
                <button type="button" className={cn('rounded px-1 hover:bg-accent', i === crumbs.length - 1 && 'font-semibold text-primary')} onClick={() => show(c.path)}>
                  {c.label}
                </button>
              </Fragment>
            ))}
          </span>
          <Choice value={view} options={VIEWS} labels={VIEW_LABELS} onChange={setView} label="Every leaf as a grid, grouped; the node's fields broken down; or its balance of coal and waste from Imported to Scheduled" />
          <Field label="Fields">
            <Choice value={fields} options={FIELDS} labels={FIELD_LABELS} onChange={setFields} label="Which fields" />
          </Field>
          {view !== 'grid' && (
            <Field label="Across">
              <Choice value={across} options={ACROSS} labels={ACROSS_LABELS} onChange={setAcross} label="Across the node: its children, or every leaf top of the stack to the bottom" />
            </Field>
          )}
          <Choice value={follow} options={FOLLOWS} labels={FOLLOW_LABELS} onChange={setFollow} label="Follow a solid picked in the view" />
          <span className="ml-auto flex items-center gap-1">
            {said && <span className="text-ok">{said}</span>}
            <Button
              variant="outline"
              size="sm"
              className="h-6 px-2 text-xs"
              disabled={!canOpenColumn}
              title={canOpenColumn ? "This block's column in the Dataflow tab: its steps in a strip, what each read and wrote, and its units before and after each. An open Dataflow tab follows the block shown here without a press" :'Pick a block, or a seam or bench of one, to open its column in the Dataflow tab'}
              onClick={() => table && openNotebook(table.id, columnNode)}
            >
              <Columns3 className="size-3.5" /> Open its column
            </Button>
            <Button variant="outline" size="sm" className="h-6 px-2 text-xs" title="Pick the solids this node was made from, on the leaves the filter keeps, in the view, the tree and Blocks" onClick={() => void selectSolids()}>
              <MousePointerSquareDashed className="size-3.5" /> Select its solids
            </Button>
            <Button
              variant="outline"
              size="sm"
              className={cn('h-6 px-2 text-xs', alone?.key === isolatedKey && 'border-primary/70 text-primary')}
              title={
                alone?.key === isolatedKey
                  ? 'Put back the layers and objects that were showing before Isolate'
                  : 'Show only the solids this node was made from, on the leaves the filter keeps, zoomed to: every other layer off, and the rest on their layers hidden. Pressed again, everything comes back as it was'
              }
              onClick={() => void isolateSolids()}
            >
              <Focus className="size-3.5" /> {alone?.key === isolatedKey ? 'Show what was shown' : 'Isolate its solids'}
            </Button>
            <Button variant="outline" size="sm" className="h-6 px-2 text-xs" disabled={!breakdown} title={view === 'grid' ? 'The grid as shown, its groups and the leaves opened, as CSV, exact values' : "This node's breakdown as CSV, exact values"} onClick={saveCsv}>
              <Download className="size-3.5" /> CSV
            </Button>
            <Button
              variant="outline"
              size="sm"
              className={cn('h-6 px-2 text-xs', updating && 'border-primary/70 text-primary')}
              title="Enter values on the table's leaves from a CSV, matched on its levels (block and seam, say), into columns of your own: what it matches and changes shown first. The run's own numbers are never written over"
              onClick={() => setUpdating((u) => !u)}
            >
              <FileUp className="size-3.5" /> Update records…
            </Button>
          </span>
        </div>
        {error && <div className="px-2 py-1 text-destructive">{error}</div>}
        {updating && table && (
          <UpdateRecords target={{ of: 'table', table: table.id }} keys={[...levelsNow, 'case']} what={['leaf', 'leaves']} onClose={() => setUpdating(false)} />
        )}
        {changedLine && (
          <div className="flex items-center gap-2 px-2 py-1 text-warning" data-slot="table-changed">
            <span>{changedLine}: its numbers may not be theirs until it runs again.</span>
            <Button variant="outline" size="sm" className="h-6 px-2 text-xs" disabled={rerunning} title="Run the reserves dataflow again for every block into this table, on what's saved: the table's next revision, one undo" onClick={() => void runAgain()}>
              {rerunning ? 'Running…' : 'Run again'}
            </Button>
          </div>
        )}
        {view !== 'grid' && breakdown && breakdown.hidden > 0 && (
          <div className="px-2 py-1 text-warning">
            The first {fmt(breakdown.columns.length - 1)} of {fmt(breakdown.columns.length - 1 + breakdown.hidden)} leaves shown: pick a strip or block for the rest. The total holds them all.
          </div>
        )}
        {view === 'grid' && grid && (
          <div className="min-h-0 flex-1" data-slot="reserves-grid">
            <TableGrid
              grid={grid}
              sort={gridSort}
              onSort={(c) => setGridSort((s) => nextSort(s, c))}
              onToggle={(p) => setGridOpen((o) => toggleGroup(o, p))}
              onRow={pickRow}
              lit={rowLit}
              rowWords={['leaf', 'leaves']}
              editable={(r, c) => r.kind === 'leaf' && c.kind === 'entered'}
              onEdit={(c, cells) => void enter(c, cells)}
              onRemove={(c) => void removeColumn(c)}
            />
          </div>
        )}
        <div className={cn('min-h-0 flex-1 overflow-auto', view === 'grid' && 'hidden')}>
          {breakdown && view === 'balance' && <ReservesBalance fields={breakdown.fields} node={crumbs.map((c) => c.label).join(' › ')} />}
          {breakdown && view === 'fields' && (
            <table className="border-separate border-spacing-0 tabular-nums" data-slot="reserves-breakdown">
              <thead className="sticky top-0 z-10 bg-card">
                <tr>
                  <th className="sticky left-0 z-20 min-w-56 border-b border-border bg-card px-2 py-1 text-left font-medium text-muted-foreground">field</th>
                  {breakdown.columns.map((c, i) => {
                    const marked = leaf !== null && startsWith(leaf, c.path) && c.depth > 0;
                    return (
                      <th
                        key={keyOf(c.path)}
                        data-col={c.path.join(' > ')}
                        className={cn(
                          'min-w-24 border-b border-border px-2 py-1 text-right font-medium whitespace-nowrap',
                          c.depth === 0 && 'bg-muted/50 font-semibold',
                          groupStarts.has(i) && 'border-l border-border',
                          c.depth > 1 && 'font-normal text-muted-foreground',
                          marked && 'text-primary',
                        )}
                        title={`${c.path.join(' › ') || 'All'}: ${fmt(c.leaves)} lea${c.leaves === 1 ? 'f' : 'ves'}${c.depth > 0 ? '. Double-click to show it on its own' : ''}`}
                        onDoubleClick={() => c.depth > 0 && show(c.path)}
                      >
                        {c.depth > 0 && c.opens ? (
                          <button type="button" className="inline-flex items-center gap-0.5 hover:underline" onClick={() => toggleOpen(c.path)} aria-label={`${c.open ? 'Close' : 'Open'} ${c.label}`}>
                            {c.open ? <ChevronDown className="size-3" /> : <ChevronRight className="size-3" />}
                            {c.label}
                          </button>
                        ) : (
                          c.label
                        )}
                      </th>
                    );
                  })}
                </tr>
              </thead>
              <tbody>
                {rows.map((r) =>
                  r.kind === 'folder' ? (
                    <tr key={`f:${r.key}`} data-folder={r.key}>
                      <td className={cn('sticky left-0 border-b border-border bg-card py-0.5 font-semibold', r.depth === 0 ? 'px-2 pt-1.5' : 'pl-5 text-muted-foreground')} colSpan={1}>
                        <button type="button" className="inline-flex items-center gap-0.5" onClick={() => toggleFolded(r.key)} aria-label={`${folded.has(r.key) ? 'Open' : 'Fold'} ${r.key}`}>
                          {folded.has(r.key) ? <ChevronRight className="size-3" /> : <ChevronDown className="size-3" />}
                          {r.label}
                        </button>
                      </td>
                      {breakdown.columns.map((c, i) => (
                        <td key={keyOf(c.path)} className={cn('border-b border-border', c.depth === 0 && 'bg-muted/50', groupStarts.has(i) && 'border-l')} />
                      ))}
                    </tr>
                  ) : (
                    <tr key={r.key} data-field={r.key} className="group hover:bg-accent/40">
                      <td className={cn('sticky left-0 bg-card py-px pr-2', r.depth === 1 ? 'pl-5' : 'pl-8')}>
                        {r.label} <span className="text-muted-foreground">{unitLabel(r.unit)}</span>
                        {table && (
                          <IconButton
                            label={`Colour the blocks in the view by ${r.key}, each its own leaf's value (decision 0075, point 7)`}
                            data-slot="colour-by"
                            className={cn(
                              'ml-1 size-5 align-middle opacity-0 group-hover:opacity-100 focus:opacity-100',
                              fieldColoured?.table === table.id && fieldColoured.column === r.key && 'text-primary opacity-100',
                            )}
                            onClick={() => colourBlocksByField({ table: table.id, column: r.key, byBlock: false })}
                          >
                            <Palette />
                          </IconButton>
                        )}
                      </td>
                      {breakdown.columns.map((c, i) => (
                        <td
                          key={keyOf(c.path)}
                          className={cn(
                            'px-2 py-px text-right whitespace-nowrap',
                            c.depth === 0 && 'bg-muted/50 font-semibold',
                            c.depth === 1 && 'font-medium',
                            groupStarts.has(i) && 'border-l border-border',
                            c.depth > 1 && 'text-muted-foreground',
                            leaf !== null && c.depth > 0 && samePath(leaf.slice(0, c.path.length), c.path) && 'text-primary',
                          )}
                        >
                          {cellText(r.field, i)}
                        </td>
                      ))}
                    </tr>
                  ),
                )}
              </tbody>
            </table>
          )}
        </div>
      </div>
    </div>
  );
}

function TreeRow({
  n,
  depth,
  node,
  leaf,
  expanded,
  setExpanded,
  show,
}: {
  n: ReservesNode;
  depth: number;
  node: string[];
  leaf: string[] | null;
  expanded: Set<string>;
  setExpanded: (f: (e: Set<string>) => Set<string>) => void;
  show: (path: string[]) => void;
}) {
  const k = keyOf(n.path);
  const isOpen = expanded.has(k);
  const shown = samePath(n.path, node);
  const holdsLeaf = leaf !== null && n.path.length > 0 && startsWith(leaf, n.path);
  const toggle = () =>
    setExpanded((e) => {
      const next = new Set(e);
      if (next.has(k)) next.delete(k);
      else next.add(k);
      return next;
    });
  return (
    <>
      <div
        role="treeitem"
        aria-selected={shown}
        aria-expanded={n.children.length > 0 ? isOpen : undefined}
        data-slot="reserves-node"
        data-path={n.path.join(' > ')}
        className={cn('flex cursor-pointer items-center gap-0.5 py-px pr-2 whitespace-nowrap hover:bg-accent', shown && 'bg-selection text-primary', !shown && holdsLeaf && 'text-primary')}
        style={{ paddingLeft: 4 + depth * 12 }}
        onClick={() => show(n.path)}
        title={`${fmt(n.leaves)} lea${n.leaves === 1 ? 'f' : 'ves'}`}
      >
        {n.children.length > 0 ? (
          <button
            type="button"
            className="text-muted-foreground"
            aria-label={`${isOpen ? 'Close' : 'Open'} ${n.label}`}
            onClick={(e) => {
              e.stopPropagation();
              toggle();
            }}
          >
            {isOpen ? <ChevronDown className="size-3" /> : <ChevronRight className="size-3" />}
          </button>
        ) : (
          <span className="w-3" />
        )}
        {n.label}
      </div>
      {isOpen && n.children.map((c) => <TreeRow key={keyOf(c.path)} n={c} depth={depth + 1} node={node} leaf={leaf} expanded={expanded} setExpanded={setExpanded} show={show} />)}
    </>
  );
}
