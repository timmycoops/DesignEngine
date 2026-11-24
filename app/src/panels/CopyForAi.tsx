import { Bot, MessageSquareText, Wrench } from 'lucide-react';
import { useState } from 'react';
import { type BriefFor, scriptBrief } from '@/bridge';
import { Button } from '@/design-system/ui/button';

/**
 * Copy for AI, the same on every script editor (the AI script brief, decision 0104): the engine's brief of how scripts
 * are written, what this kind may set, every name it can read with its meaning, the parameter tables and scripts, this
 * script and the ask. Names only: the words a column holds (seam, horizon, material names) go in only when ticked, as on
 * a client's project they're the client's. Where the check found problems, Copy problems for AI hands them over with
 * their lines, for the AI to fix its own script.
 */
export function CopyForAi({ brief, problems = [], explain = true }: { brief: BriefFor; problems?: { message: string; line: number | null }[]; explain?: boolean }) {
  const [words, setWords] = useState(false);
  const [said, setSaid] = useState<{ text: string; error?: boolean } | null>(null);
  const copy = async (ask: 'write' | 'explain' | 'fix') => {
    try {
      const text = await scriptBrief({ ...brief, words, ask, problems: ask === 'fix' ? problems.map((p) => [p.line, p.message]) : [] });
      await navigator.clipboard.writeText(text);
      setSaid({
        text:
          ask === 'fix'
            ? `Copied the ${problems.length} problem${problems.length === 1 ? '' : 's'} for an AI to fix: paste its answer back here.`
            : ask === 'explain'
              ? 'Copied for an AI to explain: names only.'
              : `Copied for an AI: how it's written, every name it can read${words ? ' and the words they hold' : ''}, this script. Say what you want where it asks, and paste its answer back here.`,
      });
    } catch (e) {
      setSaid({ text: String(e), error: true });
    }
  };
  return (
    <span className="flex flex-wrap items-center gap-1" data-slot="copy-for-ai">
      <Button variant="outline" size="sm" className="h-6 px-2 text-xs" onClick={() => void copy('write')} title="Copy a brief for an AI to write or change this script: how scripts are written, what this kind may set, every name it can read with its meaning, the parameter tables and scripts. Names only, never a value.">
        <Bot className="size-3.5" /> Copy for AI
      </Button>
      <label className="flex items-center gap-1 text-muted-foreground" title="Also copy the words each column holds: seam, horizon and material names. On a client's project those are the client's: leave it off unless you mean to share them.">
        <input type="checkbox" checked={words} onChange={(e) => setWords(e.target.checked)} aria-label="Copy the words each column holds too" />
        with words
      </label>
      {explain && (
        <Button variant="outline" size="sm" className="h-6 px-2 text-xs" onClick={() => void copy('explain')} title="Copy this script for an AI to explain in plain words. Names only.">
          <MessageSquareText className="size-3.5" /> Explain
        </Button>
      )}
      {problems.length > 0 && (
        <Button variant="outline" size="sm" className="h-6 px-2 text-xs text-warning" onClick={() => void copy('fix')} title="Copy the problems the check found, each with its line, and the script, for the AI to fix">
          <Wrench className="size-3.5" /> Copy problems for AI
        </Button>
      )}
      {said && <span className={said.error ? 'text-destructive' : 'text-muted-foreground'}>{said.text}</span>}
    </span>
  );
}
