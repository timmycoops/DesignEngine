import { useEffect, useRef, useState } from 'react';
import type { InputRule } from '@/bridge';
import { workflowInputRows } from '@/bridge';
import { FilterInput } from '@/design-system/kit';
import { type Attributes, type When, whenFits, whenText } from '@/filters';
import { useWorkspace } from '@/workspace/context';
import { plural } from './workflowParts';
import { whereOf } from './workflow';

/** How long after the last change a filter is saved: a word typed isn't a revision a key. */
const SAVE_AFTER_MS = 600;

/**
 * A step input's filter edited under it (decision 0088, workstream 5): the filter box over what the input finds before
 * its filter, as the case picked sees the project, with its values to tick and how many of them the filter keeps. Saved
 * as the step's own a moment after the last change, and when the box closes; a rule for one object says when the filter
 * keeps other than one.
 */
export function InputFilter({ workflow, step, position, input, rule, caseName, save }: { workflow: string; step: number; position: number; input: string; rule: InputRule; caseName: string; save: (when: When) => void }) {
  const { summary } = useWorkspace();
  const [rows, setRows] = useState<Attributes[] | null>(null);
  const [why, setWhy] = useState<string | null>(null);
  const saved = whenText(whereOf(rule));
  const [draft, setDraft] = useState<When>(() => whereOf(rule));
  // What's waiting to be saved, and the save that's due.
  const pending = useRef<When | null>(null);
  const timer = useRef<number | null>(null);
  const saveRef = useRef(save);
  saveRef.current = save;
  const flush = () => {
    if (timer.current !== null) window.clearTimeout(timer.current);
    timer.current = null;
    if (pending.current) saveRef.current(pending.current);
    pending.current = null;
  };

  // The saved filter changed from outside (an undo, another case's save): the box follows.
  useEffect(() => {
    if (!pending.current) setDraft(whereOf(rule));
    // Only the saved filter's text moves the draft.
  }, [saved]);
  useEffect(() => flush, []);

  // What the input finds before its filter, read again as the project changes.
  useEffect(() => {
    let live = true;
    void workflowInputRows(workflow, step, input, caseName)
      .then((found) => {
        if (!live) return;
        setRows(found);
        setWhy(null);
      })
      .catch((e) => {
        if (!live) return;
        setRows([]);
        setWhy(String(e));
      });
    return () => {
      live = false;
    };
  }, [workflow, step, input, caseName, summary]);

  const change = (when: When) => {
    setDraft(when);
    pending.current = when;
    if (timer.current !== null) window.clearTimeout(timer.current);
    timer.current = window.setTimeout(flush, SAVE_AFTER_MS);
  };

  const kept = rows ? rows.filter((r) => whenFits(draft, r)).length : null;
  const short = rows !== null && kept !== null && !rule.many && kept !== 1;
  return (
    <div className="flex flex-col gap-0.5 py-0.5" data-slot="input-filter" data-input={input}>
      <FilterInput rows={rows ?? []} value={draft} onChange={change} label={`Filter what step ${position}'s ${input} takes`} />
      <span className={short || why ? 'text-destructive' : undefined} data-slot="input-filter-finds">
        {why
          ? `Finds nothing to filter: ${why}`
          : rows === null
            ? 'Reading what it finds…'
            : `Keeps ${kept} of the ${plural(rows.length, 'object')} it finds${short ? `, and takes one` : ''}${caseName !== 'Base' ? ` (as ${caseName} sees them)` : ''}`}
      </span>
    </div>
  );
}
