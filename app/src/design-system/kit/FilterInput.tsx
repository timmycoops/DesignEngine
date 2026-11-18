import { Filter, X } from 'lucide-react';
import { useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react';
import { createPortal } from 'react-dom';
import { accept, type Attributes, isOn, type Named, readWhen, type Suggestion, type Suggestions, suggest, tick, valuesOf, type When, whenText } from '@/filters';
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
import { MENU_LIT, MENU_SURFACE } from './Select';

/** More values than this to an attribute, and its ticks list only the first so many: a range is typed for the rest. */
const MOST_TICKS = 60;

const fmtCount = new Intl.NumberFormat('en-AU');

/** How tall the suggestions may be; below the box where there's room, above it where there isn't. */
const LIST_HEIGHT = 240;

/**
 * The suggestions under the box, as the menus are drawn: on the page, over the docks, so a box at the foot of a dock
 * isn't cut off. Pressed without taking the focus from the box.
 */
function SuggestionList({ anchor, offered, lit, onLight, onTake }: { anchor: HTMLElement; offered: Suggestions; lit: number; onLight: (k: number) => void; onTake: (s: Suggestion) => void }) {
  const list = useRef<HTMLDivElement>(null);
  const [at, setAt] = useState<React.CSSProperties>({});
  useLayoutEffect(() => {
    const r = anchor.getBoundingClientRect();
    const below = window.innerHeight - r.bottom >= LIST_HEIGHT + 8 || r.top < LIST_HEIGHT + 8;
    setAt(below ? { left: r.left, top: r.bottom + 4, minWidth: r.width } : { left: r.left, bottom: window.innerHeight - r.top + 4, minWidth: r.width });
  }, [anchor, offered]);
  useEffect(() => {
    list.current?.querySelector<HTMLElement>('[aria-selected="true"]')?.scrollIntoView({ block: 'nearest' });
  }, [lit]);
  return createPortal(
    <div
      ref={list}
      role="listbox"
      data-slot="filter-suggestions"
      className={cn(MENU_SURFACE, 'fixed z-50 max-w-96 overflow-y-auto')}
      style={{ ...at, maxHeight: LIST_HEIGHT, pointerEvents: 'auto' }}
      onPointerDown={(e) => e.stopPropagation()}
      onMouseDown={(e) => e.preventDefault()}
    >
      {offered.items.map((s, k) => (
        <div
          key={s.insert}
          role="option"
          aria-selected={k === lit}
          data-value={s.insert}
          className={cn('flex cursor-default items-center gap-3 rounded-md px-1.5 py-1', k === lit && MENU_LIT)}
          onMouseEnter={() => onLight(k)}
          onClick={() => onTake(s)}
        >
          <span className="flex-1 truncate">{s.label}</span>
          {s.detail && <span className="text-muted-foreground tabular-nums">{s.detail}</span>}
        </div>
      ))}
      {offered.hint && <div className={cn('px-1.5 py-1 text-muted-foreground', offered.items.length > 0 && 'border-t border-border')} data-slot="filter-hint">{offered.hint}</div>}
    </div>,
    document.body,
  );
}

/**
 * The filter box (Filtering plan, workstream 2; decision 0088): one filter by attributes, set by ticking the values the
 * rows carry, each with how many hold it, or typed as text, `strip: 1-5; seam: U, M`, with ranges, comparisons (`<0.3`)
 * and nots (`!WASTE`) as the engine reads them. Ticks write words into the text and the text sets the ticks; a list
 * that's more than ticks, ticked, becomes the ticks of what it keeps. Text that doesn't read is ringed red, says why, and
 * leaves the filter as it was. `rows` are what's filtered: their attributes, with any measures to filter on (`volume`)
 * among them.
 *
 * As it's typed it suggests what fits where the caret is (`suggest`): attribute names, then a name's values with their
 * counts, and `names`, typed only (a layer's measures, a table's fields). ↑ ↓ light one, Tab or Enter puts it in, Esc
 * closes the list and a second press clears the box.
 */
export function FilterInput({ rows, value, onChange, label = 'Filter by attributes', names, className }: { rows: readonly Attributes[]; value: When; onChange: (when: When) => void; label?: string; names?: readonly Named[]; className?: string }) {
  const attributes = useMemo(() => valuesOf(rows), [rows]);
  const said = whenText(value);
  const [text, setText] = useState(said);
  const [why, setWhy] = useState<string | null>(null);
  // Suggestions as it's typed: open while the box has the focus, where the caret is, the one lit.
  const box = useRef<HTMLInputElement>(null);
  const [open, setOpen] = useState(false);
  const [caret, setCaret] = useState(0);
  const [lit, setLit] = useState(0);
  const offered = useMemo(() => suggest(text, caret, attributes, names), [text, caret, attributes, names]);
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
  const moved = (input: HTMLInputElement) => {
    setCaret(input.selectionStart ?? input.value.length);
    setLit(0);
  };
  const take = (s: Suggestion) => {
    const next = accept(text, offered, s);
    type(next.text);
    setCaret(next.caret);
    setLit(0);
    requestAnimationFrame(() => box.current?.setSelectionRange(next.caret, next.caret));
  };
  const showing = open && (offered.items.length > 0 || offered.hint !== undefined);
  const keyDown = (e: React.KeyboardEvent<HTMLInputElement>) => {
    const n = offered.items.length;
    if (showing && n > 0 && (e.key === 'ArrowDown' || e.key === 'ArrowUp')) {
      e.preventDefault();
      setLit((k) => (k + (e.key === 'ArrowDown' ? 1 : n - 1)) % n);
    } else if (showing && n > 0 && (e.key === 'Tab' || e.key === 'Enter')) {
      e.preventDefault();
      take(offered.items[Math.min(lit, n - 1)]);
    } else if (e.key === 'Escape') {
      // A press a thing, nearest first (decision 0080): the list closes, then the filter clears.
      if (showing) {
        e.preventDefault();
        setOpen(false);
      } else if (text !== '') {
        e.preventDefault();
        type('');
      }
    }
  };
  return (
    <span className={cn('inline-flex min-w-0 items-center gap-1', className)} data-slot="filter-input">
      <DropdownMenu>
        <DropdownMenuTrigger asChild>
          <IconButton label={`${label}: tick values`} className={cn(filtered && 'bg-primary/20 text-primary')} data-slot="filter-values" data-on={filtered || undefined}>
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
        ref={box}
        className={cn('h-7 min-w-0 flex-1 px-1.5 text-xs', filtered && !why && 'border-primary/70 bg-primary/5', why && 'border-destructive focus-visible:ring-destructive/40')}
        value={text}
        placeholder="strip: 1-5; seam: U, M; volume: <500"
        aria-label={label}
        aria-invalid={why ? true : undefined}
        role="combobox"
        aria-autocomplete="list"
        aria-expanded={showing}
        title={why ?? 'Attribute: values; another: values. Words, numbers, ranges (1-5), comparisons (<0.3) and !WASTE for one it may not hold. Suggestions as you type: ↑ ↓ to move, Tab or Enter to put one in, Esc to close'}
        onChange={(e) => {
          type(e.target.value);
          moved(e.target);
          setOpen(true);
        }}
        onFocus={(e) => {
          moved(e.currentTarget);
          setOpen(true);
        }}
        onBlur={() => setOpen(false)}
        onClick={(e) => {
          moved(e.currentTarget);
          setOpen(true);
        }}
        onKeyUp={(e) => (e.key === 'ArrowLeft' || e.key === 'ArrowRight' || e.key === 'Home' || e.key === 'End') && moved(e.currentTarget)}
        onKeyDown={keyDown}
      />
      {showing && box.current && <SuggestionList anchor={box.current} offered={offered} lit={lit} onLight={setLit} onTake={take} />}
      {filtered && (
        <IconButton label="Clear the filter" onClick={() => onChange({})}>
          <X />
        </IconButton>
      )}
    </span>
  );
}
