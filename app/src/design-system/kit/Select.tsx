import { Check } from 'lucide-react';
import { type ComponentProps, type KeyboardEvent, type MouseEvent, useEffect, useLayoutEffect, useRef, useState } from 'react';
import { createPortal } from 'react-dom';
import { cn } from 'cn';

/**
 * How every list that drops from a box looks (the menus, a select's choices, the filter's suggestions): lighter than the
 * panels, edged and shadowed, so it reads as over them and not part of them.
 */
export const MENU_SURFACE = 'rounded-lg bg-popover p-1 text-xs text-popover-foreground shadow-lg shadow-black/50 ring-1 ring-foreground/20';

/** The row under the mouse or the arrow keys. */
export const MENU_LIT = 'bg-accent text-accent-foreground';

const boxClass = 'h-7 rounded-md border border-input bg-input/30 text-xs outline-none focus-visible:border-ring focus-visible:ring-2 focus-visible:ring-ring/50';

/** How tall the list may be; below the box where there's room, above it where there isn't. */
const LIST_HEIGHT = 320;

interface Item {
  value: string;
  label: string;
  disabled: boolean;
  /** The heading of the group it's in, on its first item. */
  group?: string;
}

/** The choices a select holds, as its `<option>`s and `<optgroup>`s give them. */
function itemsOf(select: HTMLSelectElement): Item[] {
  const items: Item[] = [];
  for (const child of Array.from(select.children)) {
    if (child instanceof HTMLOptGroupElement) {
      Array.from(child.children).forEach((o, k) => {
        if (o instanceof HTMLOptionElement) items.push({ value: o.value, label: o.label || o.text, disabled: o.disabled || child.disabled, group: k === 0 ? child.label : undefined });
      });
    } else if (child instanceof HTMLOptionElement) {
      items.push({ value: child.value, label: child.label || child.text, disabled: child.disabled });
    }
  }
  return items;
}

/** The select set to a value as a person picking it would: its change goes to its `onChange`. */
function choose(select: HTMLSelectElement, value: string) {
  if (select.value === value) return;
  Object.getOwnPropertyDescriptor(HTMLSelectElement.prototype, 'value')?.set?.call(select, value);
  select.dispatchEvent(new Event('change', { bubbles: true }));
}

/**
 * A select, in the kit's look (decision 0089): a `<select>` with its `<option>`s as ever, so it's set, read and changed
 * as one, but its choices open in the app's own list, not the system's white one: dark, the choice it holds ticked in
 * gold, the one under the mouse or the arrow keys lit, groups headed. A click or Enter, Space or Alt+↓ opens it; ↑ ↓ and
 * a letter move, Enter or Tab picks, Esc closes (decision 0080: the press is the list's).
 */
export function Select({ className, onMouseDown, onKeyDown, onBlur, ...props }: ComponentProps<'select'>) {
  const box = useRef<HTMLSelectElement>(null);
  const [items, setItems] = useState<Item[] | null>(null);
  const [lit, setLit] = useState(0);
  const open = items !== null;
  const usable = (k: number) => items !== null && k >= 0 && k < items.length && !items[k].disabled;

  const show = () => {
    const select = box.current;
    if (!select || select.disabled) return;
    const all = itemsOf(select);
    setItems(all);
    setLit(Math.max(0, all.findIndex((i) => i.value === select.value)));
  };
  const close = () => setItems(null);
  const pick = (k: number) => {
    if (!box.current || !usable(k)) return;
    choose(box.current, items![k].value);
    close();
  };
  const step = (by: number) => {
    if (!items) return;
    for (let k = lit + by, n = 0; n < items.length; k += by, n++) {
      const at = (k + items.length) % items.length;
      if (!items[at].disabled) return setLit(at);
    }
  };

  const mouseDown = (e: MouseEvent<HTMLSelectElement>) => {
    onMouseDown?.(e);
    if (e.defaultPrevented || e.button !== 0) return;
    // The system's list never opens: the app's does.
    e.preventDefault();
    e.currentTarget.focus();
    if (open) close();
    else show();
  };
  const keyDown = (e: KeyboardEvent<HTMLSelectElement>) => {
    onKeyDown?.(e);
    if (e.defaultPrevented) return;
    if (!open) {
      if (e.key === 'Enter' || e.key === ' ' || e.key === 'F4' || (e.altKey && (e.key === 'ArrowDown' || e.key === 'ArrowUp'))) {
        e.preventDefault();
        show();
      }
      return;
    }
    if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
      e.preventDefault();
      step(e.key === 'ArrowDown' ? 1 : -1);
    } else if (e.key === 'Home' || e.key === 'End') {
      e.preventDefault();
      setLit(e.key === 'Home' ? 0 : items!.length - 1);
    } else if (e.key === 'Enter' || e.key === ' ' || e.key === 'Tab') {
      if (e.key !== 'Tab') e.preventDefault();
      pick(lit);
    } else if (e.key === 'Escape') {
      e.preventDefault();
      close();
    } else if (e.key.length === 1 && /\S/.test(e.key)) {
      // A letter goes to the next choice starting with it.
      e.preventDefault();
      const n = items!.length;
      for (let k = 1; k <= n; k++) {
        const at = (lit + k) % n;
        if (!items![at].disabled && items![at].label.toLowerCase().startsWith(e.key.toLowerCase())) return setLit(at);
      }
    }
  };

  return (
    <>
      <select
        ref={box}
        className={cn(boxClass, 'min-w-0 px-1', open && 'border-ring', className)}
        aria-expanded={open}
        onMouseDown={mouseDown}
        onKeyDown={keyDown}
        onBlur={(e) => {
          close();
          onBlur?.(e);
        }}
        {...props}
      />
      {open && box.current && <SelectList anchor={box.current} items={items} value={box.current.value} lit={lit} onLight={setLit} onPick={pick} onClose={close} />}
    </>
  );
}

/** The list a select opens, on the page over the docks, below its box or above it. */
function SelectList({ anchor, items, value, lit, onLight, onPick, onClose }: { anchor: HTMLElement; items: Item[]; value: string; lit: number; onLight: (k: number) => void; onPick: (k: number) => void; onClose: () => void }) {
  const list = useRef<HTMLDivElement>(null);
  const [at, setAt] = useState<React.CSSProperties>({ visibility: 'hidden' });
  useLayoutEffect(() => {
    const r = anchor.getBoundingClientRect();
    const below = (anchor.ownerDocument.defaultView ?? window).innerHeight - r.bottom >= Math.min(LIST_HEIGHT, list.current?.scrollHeight ?? LIST_HEIGHT) + 8 || r.top < (anchor.ownerDocument.defaultView ?? window).innerHeight - r.bottom;
    setAt(below ? { left: r.left, top: r.bottom + 4, minWidth: r.width } : { left: r.left, bottom: (anchor.ownerDocument.defaultView ?? window).innerHeight - r.top + 4, minWidth: r.width });
  }, [anchor, items]);
  useEffect(() => {
    list.current?.querySelector<HTMLElement>('[data-lit="true"]')?.scrollIntoView({ block: 'nearest' });
  }, [lit, at]);
  // A dock scrolled or the window resized moves the box from under it: the list closes.
  useEffect(() => {
    const scrolled = (e: Event) => !list.current?.contains(e.target as Node) && onClose();
    // The window the list opens in: the main one, or the Tables side's popped out (decision 0109).
    const win = anchor.ownerDocument.defaultView ?? window;
    win.addEventListener('scroll', scrolled, true);
    win.addEventListener('resize', onClose);
    return () => {
      win.removeEventListener('scroll', scrolled, true);
      win.removeEventListener('resize', onClose);
    };
  }, [onClose]);
  return createPortal(
    <div
      ref={list}
      role="listbox"
      data-slot="select-menu"
      className={cn(MENU_SURFACE, 'fixed z-50 max-w-96 overflow-y-auto')}
      // Over a modal dialog too: it takes the clicks, and a click on it isn't one outside the dialog.
      style={{ ...at, maxHeight: LIST_HEIGHT, pointerEvents: 'auto' }}
      onPointerDown={(e) => e.stopPropagation()}
      onMouseDown={(e) => e.preventDefault()}
    >
      {items.map((item, k) => (
        <div key={`${k}:${item.value}`}>
          {item.group !== undefined && <div className={cn('px-1.5 pt-1.5 pb-0.5 text-[10px] font-medium tracking-wide text-muted-foreground uppercase', k > 0 && 'mt-1 border-t border-border')}>{item.group}</div>}
          <div
            role="option"
            aria-selected={item.value === value}
            aria-disabled={item.disabled || undefined}
            data-value={item.value}
            data-lit={k === lit}
            className={cn(
              'flex cursor-default items-center gap-2 rounded-md py-1 pr-2 pl-1.5',
              item.disabled ? 'text-muted-foreground/60' : k === lit && MENU_LIT,
              item.value === value && !item.disabled && 'text-primary',
            )}
            onMouseEnter={() => !item.disabled && onLight(k)}
            onClick={() => onPick(k)}
          >
            <Check className={cn('size-3.5 shrink-0', item.value === value ? 'text-primary' : 'invisible')} />
            <span className="truncate">{item.label}</span>
          </div>
        </div>
      ))}
    </div>,
    anchor.ownerDocument.body,
  );
}
