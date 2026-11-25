import { useEffect, useRef, useState } from 'react';
import type { ColumnDid, ColumnField, ColumnUnit, StepTotals } from '@/bridge';
import { CellInput } from '@/design-system/kit';
import { cn } from 'cn';
import { unitWords, valueText } from './columnSteps';
import { blockMap, type CaseRow, caseRows, changeWords, mapMeasure, type SettingsGrid, shadeOf } from './reservesCase';

/**
 * The Reserves case tab's parts (Reserves case plan, decisions 8 and 11): the block map, a block's strip log before and
 * after the step, the step's settings by seam and ply, and the whole case at the step against the last run.
 */

const figure = (v: number, unit: string) => `${Math.round(v).toLocaleString('en-AU')} ${unit === 'm3' ? 'm³' : unit}`;

/**
 * The block map (decision 8): a row a strip, a square a block, shaded by what the step moved there (else what it wrote);
 * a block that stopped crossed; the block picked ringed gold. A click picks a block, whose strip log shows below.
 */
export function BlockMap({ blocks, picked, onPick }: { blocks: ColumnDid[]; picked: string[] | null; onPick: (path: string[]) => void }) {
  const rows = blockMap(blocks);
  const measure = mapMeasure(blocks);
  const key = (p: string[]) => p.join('\u0001');
  const pickedKey = picked ? key(picked) : null;
  const words = measure === 'moved' ? 'volume this step moved' : measure === 'written' ? 'volume this step wrote' : 'nothing moved by this step';
  return (
    <div className="space-y-1" data-slot="block-map">
      <div className="text-muted-foreground">Blocks · shaded by {words} · click to pick</div>
      <div className="max-h-40 space-y-0.5 overflow-auto">
        {rows.map((row) => (
          <div key={row.strip} className="flex items-center gap-1">
            <span className="w-16 shrink-0 truncate text-right text-muted-foreground tabular-nums" title={row.strip}>
              {row.strip}
            </span>
            <div className="flex flex-wrap gap-0.5">
              {row.blocks.map((b) => {
                const shade = shadeOf(b, blocks, measure);
                const name = b.path[b.path.length - 1] ?? '';
                const tip = b.stopped
                  ? `${b.path.join(' › ')}: stopped, ${b.stopped}`
                  : `${b.path.join(' › ')}: moved ${figure(b.moved, 'm3')}, wrote ${figure(b.written, 'm3')}; coal ${figure(b.coal_before, 'm3')} → ${figure(b.coal_after, 'm3')}`;
                return (
                  <button
                    key={name}
                    type="button"
                    aria-label={`Pick block ${b.path.join(' ')}`}
                    title={tip}
                    data-slot="map-block"
                    data-block={b.path.join(' ')}
                    onClick={() => onPick(b.path)}
                    className={cn(
                      'relative size-4 rounded-[2px] border border-border bg-muted',
                      pickedKey === key(b.path) && 'ring-2 ring-primary',
                      b.stopped && 'bg-destructive/30',
                    )}
                  >
                    {!b.stopped && shade > 0 && <span className="absolute inset-0 rounded-[1px] bg-primary" style={{ opacity: 0.15 + 0.85 * shade }} />}
                  </button>
                );
              })}
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}

/** A unit's look in a strip log: coal dark, waste muted, coal lost red, rock let into ROM orange. */
function unitLook(u: ColumnUnit): string {
  if (u.became?.startsWith('loss') || u.became?.startsWith('thin')) return 'bg-destructive/70';
  if (u.became?.startsWith('dilution')) return 'bg-warning/70';
  return u.material.toLowerCase() === 'coal' ? 'bg-coal' : 'bg-waste';
}

/**
 * A block's strip log (decision 8): its units top down before the step and after it, each as tall as it's thick; what the
 * step changed named beside After. A click follows a unit: its settings' cells are outlined in the grid.
 */
export function StripLog({ before, after, fields, followed, onFollow }: { before: ColumnUnit[]; after: ColumnUnit[]; fields: ColumnField[]; followed: number | null; onFollow: (u: ColumnUnit) => void }) {
  const thickness = fields.find((f) => f.kind === 'thickness')?.name ?? 'thickness';
  const volume = fields.find((f) => f.kind === 'volume')?.name ?? 'volume';
  const height = (units: ColumnUnit[]) => units.reduce((s, u) => s + Math.max(0, u.values[thickness] ?? 0), 0);
  const scale = 220 / Math.max(1e-9, height(before), height(after));
  const was = new Map(before.map((u) => [u.id, u]));
  const changed = (u: ColumnUnit) => {
    const b = was.get(u.id);
    return !b || b.became !== u.became || Math.abs((b.values[volume] ?? 0) - (u.values[volume] ?? 0)) > 1e-6 * Math.max(1, Math.abs(b.values[volume] ?? 0));
  };
  const log = (units: ColumnUnit[], side: 'before' | 'after') => (
    <div className="flex w-16 flex-col border border-border" data-slot={`strip-log-${side}`}>
      {units.map((u) => {
        const t = Math.max(0, u.values[thickness] ?? 0);
        return (
          <button
            key={u.id}
            type="button"
            className={cn('w-full border-b border-background/40', unitLook(u), followed === u.id && 'outline outline-2 -outline-offset-2 outline-primary')}
            style={{ height: Math.max(2, t * scale) }}
            title={`${unitWords(u)}${u.became ? ` (${u.became})` : ''}: ${valueText(t, 'm')} m, ${valueText(u.values[volume] ?? null, 'm3')} m³`}
            aria-label={`Follow ${unitWords(u)}`}
            onClick={() => onFollow(u)}
          />
        );
      })}
    </div>
  );
  const notes = after.filter(changed).slice(0, 8);
  return (
    <div className="flex gap-2" data-slot="strip-log">
      <div className="space-y-0.5">
        <div className="text-muted-foreground">Before</div>
        {log(before, 'before')}
      </div>
      <div className="space-y-0.5">
        <div className="text-muted-foreground">After</div>
        {log(after, 'after')}
      </div>
      <div className="min-w-0 flex-1 space-y-0.5 pt-4">
        {notes.length === 0 && <div className="text-muted-foreground">The step left this block as it was.</div>}
        {notes.map((u) => {
          const b = was.get(u.id);
          return (
            <div key={u.id} className={cn('truncate', u.became ? 'text-destructive' : 'text-edited')} title={unitWords(u)}>
              {unitWords(u)}
              {u.became ? ` ${u.became}` : ''} {b ? `${valueText(b.values[volume] ?? null, 'm3')} → ` : ''}
              {valueText(u.values[volume] ?? null, 'm3')} m³
            </div>
          );
        })}
      </div>
    </div>
  );
}

/** A setting's cell: what's typed shows as typed while it's typed, and goes to the edits at once. */
function SettingCell({ value, label, edited, outlined, readOnly, onSet }: { value: number | string | null; label: string; edited: boolean; outlined: boolean; readOnly: boolean; onSet: (text: string) => void }) {
  const shown = value === null ? '' : String(value);
  const [text, setText] = useState(shown);
  const typing = useRef(false);
  useEffect(() => {
    if (!typing.current) setText(shown);
  }, [shown]);
  if (readOnly) return <span className={cn('block px-1 text-right tabular-nums', outlined && 'rounded-sm ring-1 ring-primary')}>{shown || '—'}</span>;
  return (
    <CellInput
      value={text}
      label={label}
      numeric
      edited={edited}
      placeholder="—"
      className={cn(outlined && 'ring-1 ring-primary')}
      onFocus={() => (typing.current = true)}
      onBlur={() => ((typing.current = false), setText(shown))}
      onChange={(t) => (setText(t), onSet(t))}
    />
  );
}

/**
 * The step's settings by seam and ply (decision 11): its tables' rows in the order they're read, what each fits, a
 * column a setting; a blank passes to the rows below; the cell that gave the unit followed its value ringed gold. Type to
 * set, clear to pass on: the edits are the Parameters tab's, and the case runs on them as they're typed.
 */
export function SettingsBySeam({
  grid,
  supplied,
  edited,
  onSet,
}: {
  grid: SettingsGrid;
  supplied: Map<string, { table: number; row: number }>;
  edited: Set<string>;
  onSet: (table: number, row: number, column: string, text: string) => void;
}) {
  if (grid.columns.length === 0) return <div className="text-muted-foreground">This step reads no settings.</div>;
  if (grid.rows.length === 0) return <div className="text-muted-foreground">No table this case reads holds {grid.columns.join(', ')}.</div>;
  return (
    <div className="overflow-x-auto" data-slot="settings-by-seam">
      <table className="w-full border-collapse">
        <thead>
          <tr className="text-muted-foreground">
            <th className="px-1 text-left font-normal">Row</th>
            <th className="px-1 text-left font-normal">Fits</th>
            {grid.columns.map((c) => (
              <th key={c} className="px-1 text-right font-normal">
                {c}
              </th>
            ))}
          </tr>
        </thead>
        <tbody>
          {grid.rows.map((r) => (
            <tr key={`${r.table}:${r.row}`} className="border-t border-border" data-slot="settings-row" data-table={r.tableName} data-row={r.row}>
              <td className="px-1 whitespace-nowrap">
                {r.tableName} <span className="text-muted-foreground">row {r.row + 1}</span>
              </td>
              <td className="px-1 whitespace-nowrap text-muted-foreground">{r.fits}</td>
              {grid.columns.map((c) => {
                const s = supplied.get(c);
                return (
                  <td key={c} className="w-20 px-0.5">
                    <SettingCell
                      value={r.values[c]}
                      label={`${r.tableName} row ${r.row + 1}, ${c}`}
                      edited={edited.has(`${r.table}:${r.row}:${c.toLowerCase()}`)}
                      outlined={!!s && s.table === r.table && s.row === r.row}
                      readOnly={r.tableId === null}
                      onSet={(t) => onSet(r.table, r.row, c, t)}
                    />
                  </td>
                );
              })}
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

/** A case figure in its unit: m³ and BCM to the whole one, tonnes to the whole tonne. */
const caseFigure = (v: number | null, unit: CaseRow['unit']) => (v === null ? '—' : `${Math.round(v).toLocaleString('en-AU')} ${unit === 'm3' ? 'm³' : unit}`);

/**
 * The whole case at the step (decision 8): what the step moved and the headline figures written so far, as the last run
 * left them and, with edits, now and the change. Every block of the case, run as you type (decision 9).
 */
export function WholeCase({ saved, now }: { saved: StepTotals; now: StepTotals | null }) {
  const rows = caseRows(saved, now);
  if (rows.length === 0) return <div className="text-muted-foreground">This step moves nothing it says, and writes nothing of the headline figures yet.</div>;
  return (
    <table className="w-full border-collapse" data-slot="whole-case">
      <thead>
        <tr className="text-muted-foreground">
          <th className="px-1 text-left font-normal" />
          <th className="px-1 text-right font-normal">{now ? 'Last run' : 'This case'}</th>
          {now && <th className="px-1 text-right font-normal">Now</th>}
          {now && <th className="px-1 text-right font-normal">Change</th>}
        </tr>
      </thead>
      <tbody>
        {rows.map((r) => {
          const change = now ? changeWords(r) : '';
          return (
            <tr key={r.key} className="border-t border-border" data-slot="case-row" data-row={r.label}>
              <td className="px-1">{r.label}</td>
              <td className="px-1 text-right tabular-nums">{caseFigure(r.last, r.unit)}</td>
              {now && <td className={cn('px-1 text-right tabular-nums', change !== '—' && 'text-edited')}>{caseFigure(r.now, r.unit)}</td>}
              {now && <td className={cn('px-1 text-right tabular-nums', change !== '—' && 'text-edited')}>{change}</td>}
            </tr>
          );
        })}
      </tbody>
    </table>
  );
}
