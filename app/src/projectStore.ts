/**
 * What the app holds of the project, kept up from the shell's replies (the App at scale plan, workstream 1). The shell
 * sends only the objects added, changed or gone since what it last sent; this takes each reply in turn and gives the
 * whole project back, as the panels and viewport have always had it. Objects that didn't change keep their rows, so a
 * memo keyed on one stays good.
 */
import type { LogEntry, ObjectSummary, ProjectChanges, ProjectSummary } from './bridge';

export class ProjectStore {
  /** What the shell said the app holds once it took the last reply; null before the first. */
  private epoch: number | null = null;
  private rows = new Map<string, ObjectSummary>();
  private order: string[] = [];

  /** The project after `changes`, or null if they don't follow on from what's held: ask for the whole project. */
  take(changes: ProjectChanges): ProjectSummary | null {
    if (changes.base === null) {
      this.rows = new Map();
      this.order = [];
    } else if (changes.base !== this.epoch) {
      return null;
    }
    if (changes.removed.length > 0) {
      const gone = new Set(changes.removed);
      for (const id of gone) this.rows.delete(id);
      this.order = this.order.filter((id) => !gone.has(id));
    }
    for (const row of changes.objects) {
      if (!this.rows.has(row.id)) this.order.push(row.id);
      this.rows.set(row.id, row);
    }
    if (changes.order) this.order = changes.order;
    this.epoch = changes.epoch;
    const { base: _base, epoch: _epoch, removed: _removed, order: _order, objects: _objects, ...rest } = changes;
    return { ...rest, objects: this.order.map((id) => this.rows.get(id)!) };
  }
}

/** One session's run log as the shell sends it: from entry `from` on. */
export interface LogPart {
  session: number;
  from: number;
  entries: LogEntry[];
}

/** The run log as far as the app has it, added to from the shell's replies: the log only grows within a session. */
export class LogStore {
  session: number | null = null;
  entries: LogEntry[] = [];

  /** The whole log after `part`. */
  take(part: LogPart): LogEntry[] {
    if (part.session !== this.session) {
      this.session = part.session;
      this.entries = part.entries;
    } else if (part.entries.length > 0 || part.from !== this.entries.length) {
      // Two fetches can cross: each says where it starts, and what's before that is already held.
      this.entries = [...this.entries.slice(0, part.from), ...part.entries];
    }
    return this.entries;
  }
}
