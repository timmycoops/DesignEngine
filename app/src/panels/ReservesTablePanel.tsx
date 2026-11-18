import { ChevronDown, ChevronRight, Columns3, Download, MousePointerSquareDashed, Palette } from 'lucide-react';
import { Fragment, useEffect, useMemo, useState } from 'react';
import { ReservesBalance } from './ReservesBalanceView';
import { type Breakdown, reservesBreakdown, reservesNodeOf, reservesSolids, reservesTree, type ReservesNode } from '@/bridge';
import { Choice, Field, IconButton, ObjectMenu, Select } from '@/design-system/kit';
import { Button } from '@/design-system/ui/button';
import { cn } from 'cn';
import { useWorkspace } from '@/workspace/context';
import { breakdownCsv, cellText, fieldRows, nodeForLeaf, samePath, startsWith, unitLabel } from './reservesTable';
import { fmt } from './format';

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

// The node as fields down the side and children across, or as its balance of coal and waste (decision 0086).
type View = 'fields' | 'balance';
const VIEWS: readonly View[] = ['fields', 'balance'];
const VIEW_LABELS: Record<View, string> = { fields: 'Breakdown', balance: 'Balance' };

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

export function ReservesTable() {
  const { summary, selection, setSelection, openNotebook, followInNotebook, colourBlocksByField, fieldColoured } = useWorkspace();
  const tables = useMemo(() => (summary?.objects ?? []).filter((o) => o.kind === 'table'), [summary]);
  const [tableId, setTableId] = useState<string | null>(null);
  // A table picked anywhere is the one shown.
  const last = selection[selection.length - 1] ?? null;
  useEffect(() => {
    if (last && tables.some((t) => t.id === last)) setTableId(last);
  }, [last, tables]);
  const table = tables.find((t) => t.id === tableId) ?? tables[tables.length - 1] ?? null;

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
  const [view, setView] = useState<View>('fields');

  const tableKey = table ? `${table.id}@${table.revision}` : '';
  useEffect(() => {
    let live = true;
    setTree(null);
    setNode([]);
    setOpen([]);
    setLeaf(null);
    if (!table) return;
    void reservesTree(table.id).then((t) => {
      if (!live) return;
      setTree(t);
      // The first level open, and its first node shown, so there's something to read.
      setExpanded(new Set([keyOf([]), ...t.children.slice(0, 1).map((c) => keyOf(c.path))]));
    });
    return () => {
      live = false;
    };
    // `tableKey` stands for the table and its revision.
  }, [tableKey]);

  const openKey = open.map(keyOf).join('|');
  useEffect(() => {
    let live = true;
    if (!table) return setBreakdown(null);
    void reservesBreakdown(table.id, node, open, across === 'leaves')
      .then((b) => {
        if (!live) return;
        setBreakdown(b);
        setError(null);
      })
      .catch((e) => live && setError(String(e)));
    return () => {
      live = false;
    };
  }, [tableKey, keyOf(node), openKey, across]);

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
  const selectSolids = async () => {
    if (!table) return;
    const ids = await reservesSolids(table.id, node);
    setSelection(ids);
    setSaid(`${fmt(ids.length)} solid${ids.length === 1 ? '' : 's'} picked`);
  };
  const saveCsv = () => {
    if (!breakdown || !table) return;
    const link = document.createElement('a');
    link.href = URL.createObjectURL(new Blob([breakdownCsv(breakdown)], { type: 'text/csv' }));
    link.download = `${table.name}${node.length ? ` ${node.join(' ')}` : ''}.csv`;
    link.click();
    URL.revokeObjectURL(link.href);
  };

  if (tables.length === 0) {
    return <div className="p-3 text-xs text-muted-foreground">No reserves table yet: run a reserves dataflow (Reserves, or the workflow's reserves step) and its table shows here.</div>;
  }

  // A column is a block: the levels above the seam. A node at a block or below it opens its block's column.
  const seamAt = breakdown ? breakdown.levels.findIndex((l) => l.toLowerCase() === 'seam') : -1;
  const canOpenColumn = seamAt > 0 && node.length >= seamAt;
  const columnNode = leaf && startsWith(leaf, node) ? leaf : node;
  const crumbs: { label: string; path: string[] }[] =[{ label: 'All', path: [] }, ...node.map((w, i) => ({ label: w || '(none)', path: node.slice(0, i + 1) }))];
  return (
    <div className="flex h-full min-h-0 text-xs" data-tool-panel="reserves-table">
      <div className="flex w-52 shrink-0 flex-col border-r border-border">
        <div className="flex items-center gap-1 border-b border-border px-2 py-1">
          <Select
            value={table?.id ?? ''}
            aria-label="Reserves table"
            onChange={(e) => setTableId(e.target.value)}
          >
            {tables.map((t) => (
              <option key={t.id} value={t.id}>
                {t.name}
              </option>
            ))}
          </Select>
          <ObjectMenu object={table} what="reserves table" onDeleted={() => setTableId(null)} />
        </div>
        <div className="min-h-0 flex-1 overflow-auto py-1" role="tree" aria-label="Levels">
          {tree && <TreeRow n={tree} depth={0} node={node} leaf={leaf} expanded={expanded} setExpanded={setExpanded} show={show} />}
        </div>
      </div>
      <div className="flex min-w-0 flex-1 flex-col">
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
          <Choice value={view} options={VIEWS} labels={VIEW_LABELS} onChange={setView} label="The node's fields, or its balance of coal and waste from Imported to Scheduled" />
          <Field label="Fields">
            <Choice value={fields} options={FIELDS} labels={FIELD_LABELS} onChange={setFields} label="Which fields" />
          </Field>
          <Field label="Across">
            <Choice value={across} options={ACROSS} labels={ACROSS_LABELS} onChange={setAcross} label="Across the node: its children, or every leaf top of the stack to the bottom" />
          </Field>
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
            <Button variant="outline" size="sm" className="h-6 px-2 text-xs" title="Pick the solids this node was made from, in the view, the tree and Blocks" onClick={() => void selectSolids()}>
              <MousePointerSquareDashed className="size-3.5" /> Select its solids
            </Button>
            <Button variant="outline" size="sm" className="h-6 px-2 text-xs" disabled={!breakdown} title="This node's breakdown as CSV, exact values" onClick={saveCsv}>
              <Download className="size-3.5" /> CSV
            </Button>
          </span>
        </div>
        {error && <div className="px-2 py-1 text-destructive">{error}</div>}
        {breakdown && breakdown.hidden > 0 && (
          <div className="px-2 py-1 text-warning">
            The first {fmt(breakdown.columns.length - 1)} of {fmt(breakdown.columns.length - 1 + breakdown.hidden)} leaves shown: pick a strip or block for the rest. The total holds them all.
          </div>
        )}
        <div className="min-h-0 flex-1 overflow-auto">
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
