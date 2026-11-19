import { Check } from 'lucide-react';
import { useCallback, useEffect, useRef, useState } from 'react';
import { cn } from 'cn';
import { useWorkspace } from '../../workspace/context';
import { ToolSection } from './ToolPanel';

/**
 * Every tool asks for its inputs in turn (Surface tools plan, decision 7; decision 0092). Opened, a tool asks in the
 * prompt line under the view what to work on (what's picked already fills that step the first time), then what to work
 * with (a click on a string, a point), runs, and stays loaded, asking again. Esc steps back one prompt a press: from a
 * click to what to work on, and from there it stops asking, the tool's button starting it again. The panel lists its
 * steps, the one it's waiting on lit.
 */

/** One of a tool's steps, as its panel lists it: what it asks for, and what it has, once it has it. */
export interface AskStep {
  label: string;
  /** What fills it: "3 strings", "Pit limit". */
  value?: string;
}

/** What a tool's asking loop is handed: whether it's still the one asking, which step it's on, and the asks. */
export interface Asker {
  /** False once the tool is put down, changed, or asked to start again: the loop ends. */
  alive: () => boolean;
  /** The step being asked for, from 0, lit in the list. */
  step: (at: number) => void;
  /** What to work on: picks as ever until a right click or the button (true), or Esc (false). */
  gather: (prompt: string) => Promise<boolean>;
}

export interface Asking {
  /** The step waiting, from 0; null when it isn't asking. */
  at: number | null;
  /** Starts the loop afresh, ending any before it. */
  start: (loop: (ask: Asker) => Promise<void>) => void;
  /** Stops asking and lets go of any pick waiting. */
  stop: () => void;
  /** The tool's button: what's picked is all, at the first step; idle, start asking again. */
  next: (loop: (ask: Asker) => Promise<void>) => void;
}

/** A tool's asking, in turn. The loop it runs reads the panel's settings through a ref, since it outlives a render. */
export function useAsking(): Asking {
  const { pickObjects, finishObjects, cancelPicks } = useWorkspace();
  const [at, setAt] = useState<number | null>(null);
  const atRef = useRef<number | null>(null);
  const generation = useRef(0);
  const setStep = useCallback((step: number | null) => {
    atRef.current = step;
    setAt(step);
  }, []);
  const stop = useCallback(() => {
    generation.current += 1;
    setStep(null);
    cancelPicks();
  }, [cancelPicks, setStep]);
  const start = useCallback(
    (loop: (ask: Asker) => Promise<void>) => {
      generation.current += 1;
      const mine = generation.current;
      const alive = () => generation.current === mine;
      const ask: Asker = {
        alive,
        step: (step) => {
          if (alive()) setStep(step);
        },
        gather: (prompt) => pickObjects(prompt),
      };
      void loop(ask).finally(() => {
        if (alive()) setStep(null);
      });
    },
    [pickObjects, setStep],
  );
  const next = useCallback(
    (loop: (ask: Asker) => Promise<void>) => {
      if (atRef.current === 0) finishObjects();
      else if (atRef.current === null) start(loop);
    },
    [finishObjects, start],
  );
  useEffect(() => () => {
    generation.current += 1;
  }, []);
  return { at, start, stop, next };
}

/** A tool's steps, the one it's waiting on lit and those done ticked (decision 0092). */
export function AskSteps({ steps, at }: { steps: AskStep[]; at: number | null }) {
  return (
    <ToolSection title="Steps">
      <ol data-slot="ask-steps" className="flex flex-col gap-0.5 text-xs">
        {steps.map((s, k) => {
          const now = at === k;
          const done = at !== null && k < at;
          return (
            <li key={s.label} data-now={now || undefined} className={cn('flex items-center gap-1.5 rounded px-1.5 py-0.5', now ? 'bg-primary/15 font-medium text-foreground' : done ? 'text-foreground' : 'text-muted-foreground')}>
              <span className="w-3 shrink-0 tabular-nums">{done ? <Check className="size-3 text-ok" /> : k + 1}</span>
              <span>{s.label}</span>
              {s.value && <span className="min-w-0 truncate text-muted-foreground">: {s.value}</span>}
            </li>
          );
        })}
      </ol>
      {at === null && <p className="text-[11px] text-muted-foreground">Not asking: the button below asks again.</p>}
    </ToolSection>
  );
}
