/**
 * What a panel says a run, save or edit did, in one shape everywhere (Two sides plan, workstream 6): its words and the
 * tone they show in. `ok` green for what was done, `warning` orange for done with a catch, `refused` red for what wasn't
 * done and why, `quiet` muted for a word on the way. A run's notes are `RunResult`'s; this is the one line beside a
 * button.
 */
export type SaidTone = 'ok' | 'warning' | 'refused' | 'quiet';

export interface Said {
  text: string;
  tone: SaidTone;
}

export const ok = (text: string): Said => ({ text, tone: 'ok' });
export const warning = (text: string): Said => ({ text, tone: 'warning' });
export const refused = (text: string): Said => ({ text, tone: 'refused' });
export const quiet = (text: string): Said => ({ text, tone: 'quiet' });

/** What a command's reply said: its error refused, else the words given for what it did, else its first message. */
export function saidOf(reply: { error?: string | null; record?: { messages?: string[] } | null }, done?: string): Said {
  if (reply.error) return refused(reply.error);
  return ok(done ?? reply.record?.messages?.[0] ?? 'Done');
}

/** What a thrown error says, refused. */
export const saidOfError = (e: unknown): Said => refused(e instanceof Error ? e.message : String(e));

/** The class each tone draws in, from the theme's tokens. */
export const SAID_CLASS: Record<SaidTone, string> = { ok: 'text-ok', warning: 'text-warning', refused: 'text-destructive', quiet: 'text-muted-foreground' };
