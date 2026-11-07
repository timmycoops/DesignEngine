import { type ComponentProps, type ReactNode, useEffect, useId, useState } from 'react';
import { cn } from 'cn';
import type { ObjectSummary, StepTarget } from '@/bridge';
import { type Range, readNumber, showNumber } from './number';

/**
 * The inputs every tool panel is made of (UI plan, the kit): a number with its unit, a choice of a few, a select, an RL,
 * grid or surface, and a field row to label each. One look and one behaviour wherever they're used.
 */

const boxClass = 'h-7 rounded-md border border-input bg-input/30 text-xs outline-none focus-visible:border-ring focus-visible:ring-2 focus-visible:ring-ring/50';

/**
 * A number with its unit in the box. It takes what parses and is in range as it's typed; anything else it holds as typed,
 * outlined, with why in its tooltip, until it's put right or the box loses focus, which puts the last good value back.
 */
export function NumberInput({ value, onChange, label, unit, range, width = 'w-20' }: { value: number; onChange: (value: number) => void; label: string; unit?: string; range?: Range; width?: string }) {
  const [text, setText] = useState(showNumber(value));
  const [why, setWhy] = useState<string | null>(null);
  useEffect(() => {
    // A value set from outside shows; one being typed that reads the same stays as typed.
    setText((typed) => {
      const read = readNumber(typed, range);
      return 'value' in read && read.value === value ? typed : showNumber(value);
    });
  }, [value]);
  return (
    <span className={cn('relative inline-flex shrink-0 items-center', width)}>
      <input
        className={cn(boxClass, 'w-full px-1.5 tabular-nums', unit && 'pr-7', why && 'border-warning ring-1 ring-warning/40')}
        value={text}
        inputMode="decimal"
        aria-label={label}
        aria-invalid={why !== null}
        title={why ?? label}
        onChange={(e) => {
          setText(e.target.value);
          const read = readNumber(e.target.value, range);
          if ('value' in read) {
            setWhy(null);
            onChange(read.value);
          } else {
            setWhy(read.why);
          }
        }}
        onBlur={() => {
          setText(showNumber(value));
          setWhy(null);
        }}
      />
      {unit && <span className="pointer-events-none absolute right-1.5 text-muted-foreground">{unit}</span>}
    </span>
  );
}

/** A choice of a few, as a row of buttons, the one chosen lit. */
export function Choice<T extends string>({ value, options, labels, onChange, label }: { value: T; options: readonly T[]; labels?: Partial<Record<T, string>>; onChange: (value: T) => void; label: string }) {
  return (
    <span className="inline-flex h-7 shrink-0 overflow-hidden rounded-md border border-input text-xs" role="group" aria-label={label} title={label}>
      {options.map((option) => (
        <button
          key={option}
          type="button"
          aria-pressed={option === value}
          className={cn('px-2 transition-colors', option === value ? 'bg-selection text-primary' : 'text-muted-foreground hover:text-foreground')}
          onClick={() => onChange(option)}
        >
          {labels?.[option] ?? option}
        </button>
      ))}
    </span>
  );
}

/** A select, in the kit's look. */
export function Select({ className, ...props }: ComponentProps<'select'>) {
  return <select className={cn(boxClass, 'min-w-0 px-1', className)} {...props} />;
}

/** The grids and surfaces in a project, each kind under its own heading, or a word that there are none yet. */
export function TargetOptions({ surfaces }: { surfaces: ObjectSummary[] }) {
  const grids = surfaces.filter((s) => s.kind === 'grid');
  const meshes = surfaces.filter((s) => s.kind === 'surface');
  const options = (of: ObjectSummary[]) =>
    of.map((s) => (
      <option key={s.id} value={s.id}>
        {s.name}
      </option>
    ));
  if (surfaces.length === 0) {
    return (
      <option value="none" disabled>
        No grids or surfaces in this project yet
      </option>
    );
  }
  return (
    <>
      {grids.length > 0 && <optgroup label="Grids">{options(grids)}</optgroup>}
      {meshes.length > 0 && <optgroup label="Surfaces">{options(meshes)}</optgroup>}
    </>
  );
}

/**
 * Where a wall goes to or a line lies on (UI plan, item 3): an RL, then the grids and surfaces in groups. `none` adds a
 * first choice that leaves it to something else, as a berm lies on what its line lies on unless it says.
 */
export function TargetInput({ value, surfaces, onChange, label, none }: { value: StepTarget | null; surfaces: ObjectSummary[]; onChange: (target: StepTarget | null) => void; label: string; none?: string }) {
  const [set, current] = [onChange, value];
  return (
    <span className="flex min-w-0 items-center gap-1">
      <Select
        value={current === null ? '' : current.kind === 'rl' ? 'rl' : current.id}
        aria-label={label}
        title={label}
        onChange={(e) => set(e.target.value === '' ? null : e.target.value === 'rl' ? { kind: 'rl', rl: current?.kind === 'rl' ? current.rl : 0 } : { kind: 'surface', id: e.target.value })}
      >
        {none !== undefined && <option value="">{none}</option>}
        <option value="rl">RL</option>
        <TargetOptions surfaces={surfaces} />
      </Select>
      {current?.kind === 'rl' && <NumberInput value={current.rl} label={`${label}: RL`} unit="m" onChange={(rl) => set({ kind: 'rl', rl })} />}
    </span>
  );
}

/** A label beside what it labels, the labels of a panel lined up. */
export function Field({ label, children, hint }: { label: string; children: ReactNode; hint?: ReactNode }) {
  const id = useId();
  return (
    <div className="flex flex-col gap-0.5">
      <div className="flex min-h-7 items-center gap-2" role="group" aria-labelledby={id}>
        <span id={id} className="w-20 shrink-0 text-muted-foreground">
          {label}
        </span>
        <span className="flex min-w-0 flex-1 flex-wrap items-center gap-1.5">{children}</span>
      </div>
      {hint && <div className="pl-[5.5rem] text-muted-foreground">{hint}</div>}
    </div>
  );
}
