import { useEffect, useMemo, useState } from 'react';
import { type ColumnSteps, type GridRow, reservesStepGrid, type StepGrid } from '@/bridge';
import { GroupBy } from '@/design-system/kit';
import { Checkbox } from '@/design-system/ui/checkbox';
import { fmt } from './format';
import { beforeAfter } from './dataflowView';
import { TableGrid } from './TableGridView';
import { groupsKept, nextSort, type Sort, toggleOpen } from './tableGrid';

/**
 * Every block at a step (Tables and scripts plan, decision 10): the picked step's units for every block of the table's
 * case, a unit a row, in the Tables tab's grid, so they group, filter and add up by each field's own rule. Run on demand
 * from what's saved and never kept. The fields are After's for the block shown (those the step reads and changes),
 * All fields the rest. A unit picked opens its block's column, as the Reserves table's Open its column does.
 */
export function DataflowEveryBlock({ table, column, at, onOpen }: { table: string; column: ColumnSteps; at: number; onOpen: (words: string[]) => void }) {
  const [group, setGroup] = useState<string[] | null>(null);
  const [open, setOpen] = useState<string[][]>([]);
  const [sort, setSort] = useState<Sort>(null);
  const [all, setAll] = useState(false);
  const [result, setResult] = useState<StepGrid | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [running, setRunning] = useState(false);

  const fields = useMemo(() => beforeAfter(column, at, all).fields.map((f) => f.field.name), [column, at, all]);
  const columns = useMemo(() => ['became', 'section', ...fields], [fields]);
  const upper = result?.upper ?? [];
  const groupBy = group ?? upper;
  const groups = result?.grid.groups ?? [];
  const ask = JSON.stringify({ groupBy, open, sort, columns });
  useEffect(() => {
    let live = true;
    setRunning(true);
    void reservesStepGrid(table, at, { group: groupBy, open, columns, sort })
      .then((r) => {
        if (!live) return;
        setResult(r);
        setError(null);
      })
      .catch((e) => live && setError(String(e)))
      .finally(() => live && setRunning(false));
    return () => {
      live = false;
    };
    // `ask` stands for the group, what's open, the sort and the columns.
  }, [table, at, ask]);

  const pick = (r: GridRow) => {
    if (r.kind === 'leaf' && r.words && result) onOpen(r.words.slice(0, result.upper.length));
  };
  const step = column.steps[at];
  return (
    <div className="flex min-h-0 flex-col" data-slot="every-block">
      <div className="flex shrink-0 flex-wrap items-center gap-x-3 gap-y-1 border-b border-border px-2 py-1">
        <span className="text-muted-foreground" data-slot="every-block-said">
          {result
            ? `${step?.step === 'as input' ? 'As input' : `After ${result.step}`}: ${fmt(result.grid.leaves)} units of ${fmt(result.columns)} blocks, run on what's saved`
            : running
              ? 'Running every block…'
              : ''}
        </span>
        <GroupBy
          value={groupBy}
          options={groups.filter((g) => g !== 'name')}
          onChange={(next) => {
            setGroup(groupsKept(next, groups));
            setOpen([]);
          }}
        />
        <label className="flex items-center gap-1.5 text-muted-foreground">
          <Checkbox checked={all} onCheckedChange={(c) => setAll(c === true)} aria-label="Every field" />
          All fields
        </label>
      </div>
      {error && <div className="px-2 py-1 text-destructive">{error}</div>}
      {result && result.stopped.length > 0 && (
        <div className="px-2 py-1 text-warning" data-slot="every-block-stopped" title={result.stopped.join('\n')}>
          {fmt(result.stopped.length)} block{result.stopped.length === 1 ? '' : 's'} stopped before this step, not in the totals: {result.stopped[0]}
          {result.stopped.length > 1 ? ', …' : ''}
        </div>
      )}
      <div className="min-h-0 flex-1">
        {result && (
          <TableGrid
            grid={result.grid}
            sort={sort}
            onSort={(c) => setSort((s) => nextSort(s, c))}
            onToggle={(p) => setOpen((o) => toggleOpen(o, p))}
            onRow={pick}
            lit={(r) => r.kind === 'leaf' && !!r.words && r.words.slice(0, result.upper.length).join('\u0001') === column.column.join('\u0001')}
            rowWords={['unit', 'units']}
          />
        )}
      </div>
    </div>
  );
}
