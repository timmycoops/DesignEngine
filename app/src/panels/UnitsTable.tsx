import { Fragment, useEffect, useMemo, useRef, useState } from 'react';
import { type ColumnSteps, type FieldSet, fieldSets, setFieldSets } from '@/bridge';
import { Check, Choice, Disclosure, ok, PanelButton, refused, type Said, SaidLine, Select, TextButton, TextInput } from '@/design-system/kit';
import { cn } from 'cn';
import { unitWords, valueText } from './columnSteps';
import { fieldMap, fieldsOfSet, GROUP_WORDS, type GroupBy, journeyOf, OWN_SETS, type PieceField, type PieceGroup, type PieceRow, pieceSheet, piecesOf, rowChanged, rowWords, unitGroups } from './caseUnits';

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

export function UnitsTable({ column, at, block, fill = false }: { column: ColumnSteps; at: number; block: string[]; fill?: boolean }) {
  const [by, setBy] = useState<GroupBy>('section');
  const [set, setSet] = useState<string>('This step');
  const [mine, setMine] = useState<FieldSet[]>([]);
  const [find, setFind] = useState('');
  const [changedOnly, setChangedOnly] = useState(false);
  const [show, setShow] = useState<Show>('arrow');
  const [picked, setPicked] = useState<string | null>(null);
  const [naming, setNaming] = useState<string | null>(null);
  const [said, setSaid] = useState<Said | null>(null);
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
      setSaid(ok(`${name} saved with the project: ${shown.length} fields.`));
    } catch (e) {
      setSaid(refused(String(e)));
    }
    setNaming(null);
  };
  const deleteSet = async () => {
    try {
      setMine(await setFieldSets(mine.filter((s) => s.name !== set)));
      setSet('This step');
    } catch (e) {
      setSaid(refused(String(e)));
    }
  };

  return (
    <section className={cn('space-y-1.5 p-2', !fill && 'border-t border-border')} data-slot="units-table">
      {/* In its own column (Case tab layout plan, decision 6) the toolbar stays over the rows as they scroll. */}
      <div className={cn('flex flex-wrap items-center gap-x-3 gap-y-1', fill && 'sticky -top-2 z-10 -mx-2 -mt-2 bg-background px-2 py-1.5')}>
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
        <TextInput size="sm"
          className="w-32"
          placeholder="Find a field"
          label="Find a field across every set"
          value={find}
          onChange={(value) => setFind(value)}
        />
        <label className="flex items-center gap-1">
          <Check checked={changedOnly} onChange={(on) => setChangedOnly(on)} label="Changed only" />
          <span>Changed only</span>
        </label>
        <Choice value={show} options={['arrow', 'was'] as const} labels={{ arrow: 'before → after', was: 'after, was' }} onChange={setShow} label="How a change shows" />
        {naming === null ? (
          <TextButton onClick={() => setNaming('')} title="Keep the fields shown now as a set of your own, saved with the project">
            Save as a set
          </TextButton>
        ) : (
          <span className="flex items-center gap-1">
            <TextInput size="sm"
              className="w-28"
              placeholder="Its name"
              label="The new field set's name"
              value={naming}
              autoFocus
              onChange={(value) => setNaming(value)}
              onKeyDown={(e) => (e.key === 'Enter' && naming.trim() ? void saveSet(naming.trim()) : e.key === 'Escape' ? setNaming(null) : undefined)}
            />
            <PanelButton variant="default" row disabled={!naming.trim()} onClick={() => void saveSet(naming.trim())}>
              Save
            </PanelButton>
          </span>
        )}
        {mine.some((s) => s.name === set) && (
          <TextButton tone="danger" onClick={() => void deleteSet()}>
            Delete {set}
          </TextButton>
        )}
        <TextButton onClick={() => setMap((m) => !m)}>
          {map ? 'Hide the field map' : 'Field map'}
        </TextButton>
      </div>
      <SaidLine said={said} />
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
              {/* The picked unit opens under its first row (Case setup and unit plan, decision 8). */}
              {(() => {
                let placed = false;
                return groups.map((g) => {
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
                      const sheetHere = !placed && pickedRow !== null && row.key === pickedRow.key;
                      if (sheetHere) placed = true;
                      return (
                        <Fragment key={`${g.name}${row.key}${left}`}>
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
                        {sheetHere && (
                          <tr data-slot="picked-piece-row">
                            <td colSpan={2 + shown.length} className="px-1 pt-1 pb-2">
                              <PickedPiece column={column} at={at} block={block} row={row} onClose={() => setPicked(null)} />
                            </td>
                          </tr>
                        )}
                        </Fragment>
                      );
                    })}
                  </Fragment>
                );
                });
              })()}
            </tbody>
          </table>
        </div>
      </div>
      {map && <FieldMap column={column} at={at} fields={shown} />}
    </section>
  );
}

/**
 * The picked piece (decision 12), opened under its row across the units table (Case setup and unit plan, decisions 5
 * to 8): a title naming the unit, its block and the step; Before and After that step, no Before for a unit the step
 * made; the fields the step changed first and lit, the rest after, each under its path's heading by its short name;
 * those empty or at nought folded; Last written "this step", dimmed, or the step's number and name; and its journey.
 * Esc or × closes it.
 */
function PickedPiece({ column, at, block, row, onClose }: { column: ColumnSteps; at: number; block: string[]; row: PieceRow; onClose: () => void }) {
  const u = row.after ?? row.before!;
  const journey = journeyOf(column.steps, at, u.id, column.fields.find((f) => f.kind === 'volume')?.name);
  const sheet = pieceSheet(column.steps, at, row, column.fields);
  const [quietOpen, setQuietOpen] = useState(false);
  const box = useRef<HTMLDivElement>(null);
  useEffect(() => box.current?.focus({ preventScroll: true }), [row.key]);
  const stepWords = at > 0 ? `step ${at}` : 'as read';
  const quietCount = sheet.quiet.reduce((n, g) => n + g.fields.length, 0);
  const cols = sheet.made ? 'grid-cols-[minmax(9rem,1fr)_7.5rem_minmax(5rem,9rem)]' : 'grid-cols-[minmax(9rem,1fr)_7.5rem_7.5rem_minmax(5rem,9rem)]';
  const fieldRow = (f: PieceField) => (
    <div key={f.name} className={cn('grid items-baseline gap-x-2 border-t border-border/40', cols)} data-slot="piece-field" data-field={f.name} data-writer={f.writer}>
      <span className="truncate whitespace-nowrap" title={f.name} data-slot="piece-field-name">
        {f.short}
        {f.unit && <span className="pl-1 text-[10px] text-muted-foreground">{f.unit}</span>}
      </span>
      {!sheet.made && <span className="text-right text-muted-foreground tabular-nums">{valueText(f.before, f.unit)}</span>}
      <span className={cn('text-right tabular-nums', f.changed && 'font-semibold text-edited')}>{valueText(f.after, f.unit)}</span>
      <span className={cn('truncate pl-1 whitespace-nowrap', f.thisStep ? 'text-muted-foreground/60' : 'text-muted-foreground')}>{f.writer}</span>
    </div>
  );
  const groupList = (groups: PieceGroup[]) =>
    groups.map((g) => (
      <div key={g.path ?? ''} className="break-inside-avoid" data-slot="piece-group" data-path={g.path ?? ''}>
        {g.path && (
          <div className="pt-1.5 font-semibold text-muted-foreground" data-slot="piece-group-path">
            {g.path}
          </div>
        )}
        {g.fields.map(fieldRow)}
      </div>
    ));
  return (
    <div
      ref={box}
      tabIndex={-1}
      className="sticky left-0 max-w-[72rem] space-y-1.5 rounded border border-border bg-background p-2 outline-none"
      data-slot="picked-piece"
      data-made={sheet.made ? 'yes' : 'no'}
      onKeyDown={(e) => e.key === 'Escape' && (e.preventDefault(), e.stopPropagation(), onClose())}
    >
      <div className="flex items-baseline gap-2">
        <span className="text-[13px] font-semibold" data-slot="picked-piece-title">
          {unitWords(u)} · {block.join(' / ')} · {at > 0 ? `step ${at} ${column.steps[at].step}` : 'as read'}
        </span>
        {sheet.made && <span className="text-muted-foreground">Made by this step: nothing before it.</span>}
        {!row.after && <span className="text-muted-foreground">Taken away or joined into another by this step.</span>}
        <button type="button" className="ml-auto px-1 text-muted-foreground hover:text-foreground" onClick={onClose} aria-label="Close the picked unit (Esc)" title="Close (Esc)">
          ×
        </button>
      </div>
      <div className="flex flex-wrap gap-x-6 gap-y-2">
        <div className="min-w-0 flex-[3_1_28rem]">
          <div className={cn('grid gap-x-2 text-muted-foreground', cols)}>
            <span>Field</span>
            {!sheet.made && (
              <span className="text-right whitespace-nowrap" data-slot="piece-before-head">
                Before {stepWords}
              </span>
            )}
            <span className="text-right whitespace-nowrap">After {stepWords}</span>
            <span className="pl-1">Last written</span>
          </div>
          {sheet.changed.length > 0 && (
            <>
              <div className="pt-1 font-semibold text-edited" data-slot="piece-changed">
                Changed by this step
              </div>
              <div className="columns-[30rem] gap-x-6">{groupList(sheet.changed)}</div>
              {sheet.rest.length > 0 && <div className="pt-1.5 font-semibold text-muted-foreground">The rest</div>}
            </>
          )}
          <div className="columns-[30rem] gap-x-6">{groupList(sheet.rest)}</div>
          {quietCount > 0 && (
            <Disclosure open={quietOpen} onToggle={() => setQuietOpen((o) => !o)} className="mt-1" slot="piece-quiet" data-count={quietCount}>
              {quietCount} more, empty or nought
            </Disclosure>
          )}
          {quietOpen && <div className="columns-[30rem] gap-x-6">{groupList(sheet.quiet)}</div>}
        </div>
        <div className="min-w-0 flex-[1_1_14rem]">
          <div className="text-muted-foreground">Its journey</div>
          <ol className="space-y-0.5" data-slot="journey">
            {journey.map((j) => (
              <li key={j.step}>
                <span className="text-muted-foreground tabular-nums">{j.step}</span> {j.name}: {j.what}
              </li>
            ))}
          </ol>
        </div>
      </div>
    </div>
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
