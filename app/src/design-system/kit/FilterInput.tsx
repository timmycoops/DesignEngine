import { Filter, X } from 'lucide-react';
import { useEffect, useMemo, useState } from 'react';
import { type Attributes, isOn, readWhen, tick, valuesOf, type When, whenText } from '@/filters';
import {
  DropdownMenu,
  DropdownMenuCheckboxItem,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuSub,
  DropdownMenuSubContent,
  DropdownMenuSubTrigger,
  DropdownMenuTrigger,
} from '@/design-system/ui/dropdown-menu';
import { Input } from '@/design-system/ui/input';
import { cn } from 'cn';
import { IconButton } from './badges';

/** More values than this to an attribute, and its ticks list only the first so many: a range is typed for the rest. */
const MOST_TICKS = 60;

const fmtCount = new Intl.NumberFormat('en-AU');

/**
 * The filter box (Filtering plan, workstream 2; decision 0088): one filter by attributes, set by ticking the values the
 * rows carry, each with how many hold it, or typed as text, `strip: 1-5; seam: U, M`, with ranges, comparisons (`<0.3`)
 * and nots (`!WASTE`) as the engine reads them. Ticks write words into the text and the text sets the ticks; a list
 * that's more than ticks, ticked, becomes the ticks of what it keeps. Text that doesn't read is ringed red, says why, and
 * leaves the filter as it was. `rows` are what's filtered: their attributes, with any measures to filter on (`volume`)
 * among them.
 */
export function FilterInput({ rows, value, onChange, label = 'Filter by attributes', className }: { rows: readonly Attributes[]; value: When; onChange: (when: When) => void; label?: string; className?: string }) {
  const attributes = useMemo(() => valuesOf(rows), [rows]);
  const said = whenText(value);
  const [text, setText] = useState(said);
  const [why, setWhy] = useState<string | null>(null);
  // The filter changed by a tick or from outside: the text follows, unless what's typed already reads as it.
  useEffect(() => {
    const read = readWhen(text);
    if (!('when' in read) || whenText(read.when) !== said) {
      setText(said);
      setWhy(null);
    }
    // Only the filter moves the text; typing moves the filter.
  }, [said]);
  const type = (next: string) => {
    setText(next);
    const read = readWhen(next);
    if ('why' in read) return setWhy(read.why);
    setWhy(null);
    onChange(read.when);
  };
  const filtered = Object.keys(value).length > 0;
  return (
    <span className={cn('inline-flex min-w-0 items-center gap-1', className)} data-slot="filter-input">
      <DropdownMenu>
        <DropdownMenuTrigger asChild>
          <IconButton label={`${label}: tick values`} className={cn(filtered && 'text-primary')} data-slot="filter-values">
            <Filter />
          </IconButton>
        </DropdownMenuTrigger>
        <DropdownMenuContent align="start" className="max-h-96 min-w-52 overflow-auto" data-slot="filter-menu" onCloseAutoFocus={(e) => e.preventDefault()}>
          {attributes.length === 0 && <DropdownMenuLabel className="font-normal text-muted-foreground">No attributes to filter by</DropdownMenuLabel>}
          {attributes.map((a) => {
            const on = a.values.filter((v) => isOn(value, a.key, v.value)).length;
            return (
              <DropdownMenuSub key={a.key}>
                <DropdownMenuSubTrigger data-attribute={a.key}>
                  <span className={cn('flex-1 truncate', value[a.key] !== undefined && 'text-primary')}>{a.key}</span>
                  <span className="ml-3 text-xs text-muted-foreground tabular-nums">{on > 0 ? `${on} of ${a.values.length}` : a.values.length}</span>
                </DropdownMenuSubTrigger>
                <DropdownMenuSubContent className="max-h-96 min-w-44 overflow-auto" data-slot="filter-values-of" data-attribute={a.key}>
                  {a.numeric && a.min !== null && a.max !== null && (
                    <DropdownMenuLabel className="font-normal text-muted-foreground">
                      {a.min === a.max ? `All ${a.min}` : `${a.min} to ${a.max}`}: tick, or type a range ({a.key}: {a.min}-{a.max}) or &lt;
                    </DropdownMenuLabel>
                  )}
                  {a.values.slice(0, MOST_TICKS).map((v) => (
                    <DropdownMenuCheckboxItem
                      key={v.value}
                      checked={isOn(value, a.key, v.value)}
                      data-value={v.value}
                      onSelect={(e) => e.preventDefault()}
                      onCheckedChange={(checked) => onChange(tick(value, a.key, v.value, checked === true, a.values.map((x) => x.value)))}
                    >
                      <span className="flex-1 truncate">{v.value === '' ? '(blank)' : v.value}</span>
                      <span className="ml-3 text-xs text-muted-foreground tabular-nums">{fmtCount.format(v.count)}</span>
                    </DropdownMenuCheckboxItem>
                  ))}
                  {a.values.length > MOST_TICKS && (
                    <DropdownMenuLabel className="font-normal text-muted-foreground">
                      {fmtCount.format(a.values.length - MOST_TICKS)} more: type them, or a range
                    </DropdownMenuLabel>
                  )}
                </DropdownMenuSubContent>
              </DropdownMenuSub>
            );
          })}
          {filtered && (
            <>
              <DropdownMenuSeparator />
              <DropdownMenuItem onSelect={() => onChange({})}>Clear the filter</DropdownMenuItem>
            </>
          )}
        </DropdownMenuContent>
      </DropdownMenu>
      <Input
        className={cn('h-7 min-w-0 flex-1 px-1.5 text-xs', why && 'border-destructive focus-visible:ring-destructive/40')}
        value={text}
        placeholder="strip: 1-5; seam: U, M; volume: <500"
        aria-label={label}
        aria-invalid={why ? true : undefined}
        title={why ?? 'Attribute: values; another: values. Words, numbers, ranges (1-5), comparisons (<0.3) and !WASTE for one it may not hold'}
        onChange={(e) => type(e.target.value)}
      />
      {filtered && (
        <IconButton label="Clear the filter" onClick={() => onChange({})}>
          <X />
        </IconButton>
      )}
    </span>
  );
}
