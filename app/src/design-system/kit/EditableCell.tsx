import { type ClipboardEvent, type KeyboardEvent, useEffect, useRef, useState } from 'react';
import { cn } from 'cn';

/**
 * A table cell a person types in (Tables and scripts plan, decisions 2 and 4): what it holds, and on a double-click (or
 * Enter while it's focused) a box to type a new value. Enter or Tab keeps it, Esc puts it back. Several lines pasted
 * (a column from a spreadsheet) go down the rows from here, through `onPasteLines`. `title` says why a cell can't be
 * typed in, where it can't; such a cell is drawn as any other.
 */
export function EditableCell({
  shown,
  raw,
  onCommit,
  onPasteLines,
  numeric = false,
  className,
  label,
}: {
  /** What the cell shows. */
  shown: string;
  /** What the box starts with: the value as it is, exact. */
  raw: string;
  onCommit: (text: string) => void;
  onPasteLines?: (lines: string) => void;
  numeric?: boolean;
  className?: string;
  /** What the box is, for a screen reader and the bench: `rd of 3_B1_U`. */
  label: string;
}) {
  const [editing, setEditing] = useState(false);
  const [text, setText] = useState(raw);
  const box = useRef<HTMLInputElement>(null);
  useEffect(() => {
    if (editing) {
      box.current?.focus();
      box.current?.select();
    }
  }, [editing]);
  const start = () => {
    setText(raw);
    setEditing(true);
  };
  const keep = () => {
    setEditing(false);
    if (text !== raw) onCommit(text);
  };
  const onKey = (e: KeyboardEvent<HTMLInputElement>) => {
    if (e.key === 'Enter' || e.key === 'Tab') {
      e.preventDefault();
      keep();
    } else if (e.key === 'Escape') {
      e.preventDefault();
      e.stopPropagation();
      setEditing(false);
    }
  };
  const onPaste = (e: ClipboardEvent<HTMLInputElement>) => {
    const pasted = e.clipboardData.getData('text/plain');
    if (!onPasteLines || !/[\r\n]/.test(pasted.replace(/[\r\n]+$/, ''))) return;
    e.preventDefault();
    setEditing(false);
    onPasteLines(pasted);
  };
  if (editing) {
    return (
      <input
        ref={box}
        aria-label={label}
        data-slot="cell-input"
        className={cn('h-5 w-full min-w-16 rounded-sm border border-ring bg-input/30 px-1 text-xs outline-none', numeric && 'text-right tabular-nums')}
        value={text}
        onChange={(e) => setText(e.target.value)}
        onKeyDown={onKey}
        onPaste={onPaste}
        onBlur={keep}
        onClick={(e) => e.stopPropagation()}
      />
    );
  }
  return (
    <span
      role="button"
      tabIndex={0}
      aria-label={`${label}: double-click to type`}
      data-slot="cell-editable"
      title="Double-click to type; paste a column to fill the rows below"
      className={cn('block min-h-4 cursor-text rounded-sm hover:outline hover:outline-1 hover:outline-ring/40', numeric && 'text-right tabular-nums', className)}
      onDoubleClick={(e) => {
        e.stopPropagation();
        start();
      }}
      onKeyDown={(e) => {
        if (e.key === 'Enter') {
          e.preventDefault();
          start();
        }
      }}
    >
      {shown}
    </span>
  );
}
