import { X } from 'lucide-react';
import { Select } from './fields';

/**
 * Group by (Tables and scripts plan, decision 1): the word columns a table is grouped by, outermost first, each with a
 * cross to take it off, and a list to add another. `vertical` stacks them, for a column at a panel's side.
 */
export function GroupBy({ value, options, onChange, vertical = false }: { value: readonly string[]; options: readonly string[]; onChange: (next: string[]) => void; vertical?: boolean }) {
  const left = options.filter((o) => !value.includes(o));
  return (
    <div className={vertical ? 'flex flex-col items-stretch gap-1' : 'flex flex-wrap items-center gap-1'} data-slot="group-by" aria-label="Group by">
      <span className="shrink-0 text-muted-foreground">Group by</span>
      {value.map((g, i) => (
        <span key={g} className="inline-flex h-6 items-center gap-1 rounded-md border border-border bg-muted/40 pr-0.5 pl-1.5" data-group={g}>
          {i > 0 && !vertical && <span className="text-muted-foreground">›</span>}
          {g}
          <button type="button" className="rounded p-0.5 text-muted-foreground hover:bg-accent hover:text-foreground" aria-label={`Stop grouping by ${g}`} onClick={() => onChange(value.filter((v) => v !== g))}>
            <X className="size-3" />
          </button>
        </span>
      ))}
      {left.length > 0 && (
        <Select value="" aria-label="Add a column to group by" onChange={(e) => e.target.value && onChange([...value, e.target.value])}>
          <option value="">{value.length === 0 ? 'Nothing: every row' : 'Then by…'}</option>
          {left.map((o) => (
            <option key={o} value={o}>
              {o}
            </option>
          ))}
        </Select>
      )}
    </div>
  );
}
