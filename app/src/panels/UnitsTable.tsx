import { Fragment, useEffect, useMemo, useState } from 'react';
import { type ColumnSteps, type FieldSet, fieldSets, setFieldSets } from '@/bridge';
import { Choice, Select } from '@/design-system/kit';
import { Button } from '@/design-system/ui/button';
import { cn } from 'cn';
import { unitWords, valueText } from './columnSteps';
import { fieldMap, fieldsOfSet, GROUP_WORDS, type GroupBy, journeyOf, lastWriter, OWN_SETS, type PieceRow, piecesOf, rowChanged, rowWords, unitGroups } from './caseUnits';

/**
 * The units table (the Build Plan's "Reserves case plan", decision 12, workstream 5): under the step view, the picked
 * block's units before and after the step, a row a piece of ground, in groups that add up before and after by each
 * field's own rule; a piece that leaves a group stays listed in it, in red. Field sets (This step by default, the table's
 * own, a planner's saved with the project), Find a field, Changed only, a change as "before → after" or "after, was";
 * the picked piece with every field, the last step to write it and its journey; and the field map, steps by fields.
 */

type Show = 'arrow' | 'was';

const moved = (a: number | null | undefined, b: number | null | undefined) =>
  (a == null) !== (b == null) || (a != null && b != null && Math.abs(a - b) > 1e-9 * Math.max(1, Math.abs(a)));

export function UnitsTable({ column, at, block }: { column: ColumnSteps; at: number; block: string[] }) {
  const [by, setBy] = useState<GroupBy>('section');
  const [set, setSet] = useState<string>('This step');
  const [mine, setMine] = useState<FieldSet[]>([]);
  const [find, setFind] = useState('');
  const [changedOnly, setChangedOnly] = useState(false);
  const [show, setShow] = useState<Show>('arrow');
  const [picked, setPicked] = useState<string | null>(null);
  const [naming, setNaming] = useState<string | null>(null);
  const [said, setSaid] = useState<{ text: string; refused: boolean } | null>(null);
  const [map, setMap] = useState(false);
  useEffect(() => {
    void fieldSets().then(setMine).catch(() => setMine([]));
  }, []);

  const before = at > 0 ? column.steps[at - 1].units : column.steps[0].units;
  const after = column.steps[at].units;
  const rows = useMemo(() => piecesOf(before, after), [before, after]);
  const unitOf = (name: string) => column.fields.find((f) => f.name === name)?.unit ?? '';
  const setFields = fieldsOfSet(set, column.fields, column.steps[at], rows, mine);
  // Find a field across every set: what's typed picks from all the fields.
  const shown = find.trim() ? column.fields.map((f) => f.name).filter((n) => n.toLowerCase().includes(find.trim().toLowerCase())) : setFields;
  const groups = useMemo(() => unitGroups(rows, by, column.fields), [rows, by, column.fields]);
  const words = [...new Set(after.flatMap((u) => Object.keys(u.words ?? {})))];
  const pickedRow = rows.find((r) => r.key === picked) ?? null;

  const cell = (b: number | null | undefined, a: number | null | undefined, unit: string, side: 'both' | 'after' | 'before') => {
    const text = (v: number | null | undefined) => (v == null ? '—' : valueText(v, unit));
    if (side === 'after') return <span>{text(a)}</span>;
    if (side === 'before') return <span className="text-muted-foreground line-through">{text(b)}</span>;
    if (!moved(b, a)) return <span>{text(a)}</span>;
    return show === 'arrow' ? (
      <span className="text-edited">
        {text(b)} → {text(a)}
      </span>
    ) : (
      <span className="text-edited">
        {text(a)} <span className="text-muted-foreground">was {text(b)}</span>
      </span>
    );
  };
  const sideOf = (r: PieceRow) => (r.before && r.after ? 'both' : r.after ? 'after' : 'before');

  const saveSet = async (name: string) => {
    try {
      const kept = await setFieldSets([...mine.filter((s) => s.name !== name), { name, fields: shown }]);
      setMine(kept);
      setSet(name);
      setFind('');
      setSaid({ text: `${name} saved with the project: ${shown.length} fields.`, refused: false });
    } catch (e) {
      setSaid({ text: String(e), refused: true });
    }
    setNaming(null);
  };
  const deleteSet = async () => {
    try {
      setMine(await setFieldSets(mine.filter((s) => s.name !== set)));
      setSet('This step');
    } catch (e) {
      setSaid({ text: String(e), refused: true });
    }
  };

  return (
    <section className="space-y-1.5 border-t border-border p-2" data-slot="units-table">
      <div className="flex flex-wrap items-center gap-x-3 gap-y-1">
        <span className="font-semibold">
          Units of {block.join(' / ')} {at > 0 ? `before and after ${column.steps[at].step}` : 'as read'}
        </span>
        <label className="flex items-center gap-1">
          <span className="text-muted-foreground">Group by</span>
          <Select className="h-6 w-36" aria-label="Group the units by" value={by} onChange={(e) => setBy(e.target.value as GroupBy)}>
            {Object.entries(GROUP_WORDS).map(([k, w]) => (
              <option key={k} value={k}>
                {w}
              </option>
            ))}
            {words.map((w) => (
              <option key={w} value={`word:${w}`}>
                {w}
              </option>
            ))}
          </Select>
        </label>
        <label className="flex items-center gap-1">
          <span className="text-muted-foreground">Fields</span>
          <Select className="h-6 w-36" aria-label="The field set shown" value={set} onChange={(e) => (setSet(e.target.value), setFind(''))}>
            {OWN_SETS.map((s) => (
              <option key={s} value={s}>
                {s}
              </option>
            ))}
            {mine.map((s) => (
              <option key={s.name} value={s.name}>
                {s.name}
              </option>
            ))}
          </Select>
        </label>
        <input
          className="h-6 w-32 rounded-sm border border-input bg-transparent px-1 text-xs outline-none focus-visible:border-ring"
          placeholder="Find a field"
          aria-label="Find a field across every set"
          value={find}
          onChange={(e) => setFind(e.target.value)}
        />
        <label className="flex items-center gap-1">
          <input type="checkbox" checked={changedOnly} onChange={(e) => setChangedOnly(e.target.checked)} aria-label="Changed only" />
          <span>Changed only</span>
        </label>
        <Choice value={show} options={['arrow', 'was'] as const} labels={{ arrow: 'before → after', was: 'after, was' }} onChange={setShow} label="How a change shows" />
        {naming === null ? (
          <button type="button" className="text-muted-foreground hover:text-foreground hover:underline" onClick={() => setNaming('')} title="Keep the fields shown now as a set of your own, saved with the project">
            Save as a set
          </button>
        ) : (
          <span className="flex items-center gap-1">
            <input
              className="h-6 w-28 rounded-sm border border-input bg-transparent px-1 text-xs outline-none focus-visible:border-ring"
              placeholder="Its name"
              aria-label="The new field set's name"
              value={naming}
              autoFocus
              onChange={(e) => setNaming(e.target.value)}
              onKeyDown={(e) => (e.key === 'Enter' && naming.trim() ? void saveSet(naming.trim()) : e.key === 'Escape' ? setNaming(null) : undefined)}
            />
            <Button size="sm" className="h-6 px-2 text-xs" disabled={!naming.trim()} onClick={() => void saveSet(naming.trim())}>
              Save
            </Button>
          </span>
        )}
        {mine.some((s) => s.name === set) && (
          <button type="button" className="text-muted-foreground hover:text-destructive hover:underline" onClick={() => void deleteSet()}>
            Delete {set}
          </button>
        )}
        <button type="button" className="text-muted-foreground hover:text-foreground hover:underline" onClick={() => setMap((m) => !m)}>
          {map ? 'Hide the field map' : 'Field map'}
        </button>
      </div>
      {said && <div className={said.refused ? 'text-destructive' : 'text-ok'}>{said.text}</div>}
      <div className="flex gap-2">
        <div className="min-w-0 flex-1 overflow-x-auto">
          <table className="w-full border-collapse" data-slot="units-grid">
            <thead>
              <tr className="text-muted-foreground">
                <th className="px-1 text-left font-normal">Unit</th>
                <th className="px-1 text-left font-normal">Goes to</th>
                {shown.map((f) => (
                  <th key={f} className="px-1 text-right font-normal whitespace-nowrap">
                    {f} {unitOf(f) && <span className="text-[10px]">{unitOf(f)}</span>}
                  </th>
                ))}
              </tr>
            </thead>
            <tbody>
              {groups.map((g) => {
                const listed = g.rows.filter((r) => !changedOnly || r.left || rowChanged(r.row, shown));
                return (
                  <Fragment key={g.name}>
                    <tr className="border-t border-border bg-muted/40 font-semibold" data-slot="units-group" data-group={g.name}>
                      <td className="px-1">{g.name}</td>
                      <td className="px-1 font-normal text-muted-foreground">
                        {g.counts[0]} → {g.counts[1]} units
                      </td>
                      {shown.map((f) => (
                        <td key={f} className="px-1 text-right whitespace-nowrap tabular-nums" data-field={f} data-before={g.before[f] ?? ''} data-after={g.after[f] ?? ''}>
                          {cell(g.before[f], g.after[f], unitOf(f), 'both')}
                        </td>
                      ))}
                    </tr>
                    {listed.map(({ row, left }) => {
                      const u = row.after ?? row.before!;
                      return (
                        <tr
                          key={`${g.name}${row.key}${left}`}
                          className={cn('cursor-pointer border-t border-border/50 hover:bg-accent', left && 'text-destructive', picked === row.key && 'bg-selection')}
                          data-slot="units-row"
                          data-row={row.key}
                          data-left={left ? 'yes' : 'no'}
                          data-horizon={u.horizon}
                          data-volume-before={row.before?.values.volume ?? ''}
                          data-volume-after={row.after?.values.volume ?? ''}
                          data-from={row.before ? row.before.id : (row.other?.id ?? '')}
                          onClick={() => setPicked(row.key)}
                        >
                          <td className="px-1 pl-3 whitespace-nowrap">{rowWords(row)}</td>
                          <td className="px-1 whitespace-nowrap text-muted-foreground">{left ? `left for ${row.after ? (row.after.rom ?? row.after.section ?? '—') : 'nowhere'}` : (u.rom ?? u.section ?? '')}</td>
                          {shown.map((f) => (
                            <td key={f} className="px-1 text-right whitespace-nowrap tabular-nums">
                              {left ? <span className="text-destructive">{valueText(row.after?.values[f] ?? null, unitOf(f))}</span> : cell(row.before?.values[f], row.after?.values[f], unitOf(f), sideOf(row))}
                            </td>
                          ))}
                        </tr>
                      );
                    })}
                  </Fragment>
                );
              })}
            </tbody>
          </table>
        </div>
        {pickedRow && <PickedPiece column={column} at={at} row={pickedRow} onClose={() => setPicked(null)} />}
      </div>
      {map && <FieldMap column={column} at={at} fields={shown} />}
    </section>
  );
}

/** The picked piece (decision 12): every field before and after, the last step to write it, and its journey. */
function PickedPiece({ column, at, row, onClose }: { column: ColumnSteps; at: number; row: PieceRow; onClose: () => void }) {
  const u = row.after ?? row.before!;
  const journey = journeyOf(column.steps, at, u.id, column.fields.find((f) => f.kind === 'volume')?.name);
  return (
    <aside className="w-72 shrink-0 space-y-1.5 border-l border-border pl-2" data-slot="picked-piece">
      <div className="flex items-baseline gap-2">
        <span className="font-semibold">{unitWords(u)}</span>
        <button type="button" className="ml-auto text-muted-foreground hover:text-foreground" onClick={onClose} aria-label="Close the picked piece">
          ×
        </button>
      </div>
      <table className="w-full border-collapse">
        <thead>
          <tr className="text-muted-foreground">
            <th className="text-left font-normal">Field</th>
            <th className="text-right font-normal">Before</th>
            <th className="text-right font-normal">After</th>
            <th className="pl-1 text-left font-normal">Last written</th>
          </tr>
        </thead>
        <tbody>
          {column.fields.map((f) => (
            <tr key={f.name} className="border-t border-border/50">
              <td>{f.name}</td>
              <td className="text-right tabular-nums">{valueText(row.before?.values[f.name] ?? null, f.unit)}</td>
              <td className={cn('text-right tabular-nums', moved(row.before?.values[f.name], row.after?.values[f.name]) && 'text-edited')}>{valueText(row.after?.values[f.name] ?? null, f.unit)}</td>
              <td className="pl-1 text-muted-foreground">{lastWriter(column.steps, at, u.id, f.name) ?? 'as read'}</td>
            </tr>
          ))}
        </tbody>
      </table>
      <div className="text-muted-foreground">Its journey</div>
      <ol className="space-y-0.5" data-slot="journey">
        {journey.map((j) => (
          <li key={j.step}>
            <span className="text-muted-foreground tabular-nums">{j.step}</span> {j.name}: {j.what}
          </li>
        ))}
      </ol>
    </aside>
  );
}

/** The field map (decision 12): steps by fields, which reads (R) and which writes (W) each on this block. */
function FieldMap({ column, at, fields }: { column: ColumnSteps; at: number; fields: string[] }) {
  const rows = fieldMap(column.steps, column.fields);
  return (
    <div className="overflow-x-auto" data-slot="field-map">
      <table className="border-collapse">
        <thead>
          <tr className="text-muted-foreground">
            <th className="px-1 text-left font-normal">Step</th>
            {fields.map((f) => (
              <th key={f} className="px-1 font-normal">
                {f}
              </th>
            ))}
          </tr>
        </thead>
        <tbody>
          {rows.map((r) => (
            <tr key={r.step} className={cn('border-t border-border/50', r.step === at && 'bg-selection')}>
              <td className="px-1 whitespace-nowrap">
                <span className="text-muted-foreground tabular-nums">{r.step}</span> {r.name}
              </td>
              {fields.map((f) => (
                <td key={f} className="px-1 text-center">
                  <span className="text-primary">{r.reads.has(f) ? 'R' : ''}</span>
                  <span className="text-edited">{r.writes.has(f) ? 'W' : ''}</span>
                </td>
              ))}
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
