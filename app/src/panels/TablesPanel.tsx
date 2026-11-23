import { useEffect, useMemo, useState } from 'react';
import { Select } from '@/design-system/kit';
import { useWorkspace } from '@/workspace/context';
import { BlockTable } from './BlockTablePanel';
import { ReservesTable } from './ReservesTablePanel';

/**
 * The Tables tab (Tables and scripts plan, decision 1): every table in one place, picked from the Table list at its head.
 * Objects first, the project's objects as rows (the blocks table: each solid's own volume and attributes, a live view);
 * then each reserves table, a row a leaf, with its Grid, Breakdown and Balance. One table concept, two kinds of row.
 *
 * The toolbar's Objects table opens it on Objects and its Reserves table on the latest reserves table; a table picked
 * anywhere, in Properties or a list, is shown.
 */

const OBJECTS = 'objects';

export function TablesPanel() {
  const { summary, selection, toolAsk, toolMode } = useWorkspace();
  const tables = useMemo(() => (summary?.objects ?? []).filter((o) => o.kind === 'table' && !o.objects_own), [summary]);
  const latest = tables[tables.length - 1]?.id ?? null;
  const [pick, setPick] = useState<string | null>(null);
  const shown = pick === OBJECTS || tables.some((t) => t.id === pick) ? pick : (latest ?? OBJECTS);

  // A table picked anywhere is the one shown.
  const last = selection[selection.length - 1] ?? null;
  useEffect(() => {
    if (last && tables.some((t) => t.id === last)) setPick(last);
  }, [last, tables]);
  // The toolbar: Objects table to Objects, Reserves table to a reserves table (the one shown, else the latest).
  useEffect(() => {
    if (toolAsk?.panel !== 'restable') return;
    if (toolAsk.mode === OBJECTS) setPick(OBJECTS);
    else if (shown === OBJECTS && latest) setPick(latest);
  }, [toolAsk]);
  // The ribbon lights the tool for what's shown.
  useEffect(() => {
    toolMode('restable', shown === OBJECTS ? OBJECTS : 'reserves');
    return () => toolMode('restable', null);
  }, [toolMode, shown]);

  const picker = (
    <Select value={shown ?? OBJECTS} aria-label="Table" data-slot="table-pick" onChange={(e) => setPick(e.target.value)}>
      <option value={OBJECTS}>Objects</option>
      {tables.length > 0 && (
        <optgroup label="Reserves tables">
          {tables.map((t) => (
            <option key={t.id} value={t.id}>
              {t.name}
            </option>
          ))}
        </optgroup>
      )}
    </Select>
  );
  if (shown === OBJECTS || !shown) return <BlockTable picker={picker} />;
  return <ReservesTable tableId={shown} picker={picker} onDeleted={() => setPick(null)} />;
}
