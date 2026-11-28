import { FileUp, X } from 'lucide-react';
import { useEffect, useRef, useState } from 'react';
import { type RecordsReport, type RecordsTarget, recordsPreview } from '@/bridge';
import { Check, Choice, ok, refused, type Said, SaidLine } from '@/design-system/kit';
import { Button } from '@/design-system/ui/button';
import { cn } from 'cn';
import { useWorkspace } from '@/workspace/context';
import { fmt } from './format';
import { csvColumns, likelyKey } from './tableGrid';

type Mode = 'update' | 'replace';
const MODES: readonly Mode[] = ['update', 'replace'];
const MODE_LABELS: Record<Mode, string> = { update: 'Update', replace: 'Replace' };

/**
 * Update records from a CSV (Tables and scripts plan, decision 5), under the Tables tab's head: the file chosen, its
 * key columns ticked (those the table has by name first), Update or Replace, and the match report before anything is
 * written: records matched, in the file only, what's matched by none, every value that would change. The button writes
 * it, one `table.update_records` run, one undo; a file that can't be taken is refused with why, the button off.
 */
export function UpdateRecords({ target, keys, what, onClose }: { target: RecordsTarget; keys: readonly string[]; what: [string, string]; onClose: () => void }) {
  const { runRecord } = useWorkspace();
  const file = useRef<HTMLInputElement>(null);
  const [csv, setCsv] = useState<{ name: string; text: string } | null>(null);
  const [key, setKey] = useState<string[]>([]);
  const [mode, setMode] = useState<Mode>('update');
  const [report, setReport] = useState<RecordsReport | null>(null);
  const [said, setSaid] = useState<Said | null>(null);
  const [running, setRunning] = useState(false);
  const columns = csv ? csvColumns(csv.text) : [];

  const choose = async (f: File | undefined) => {
    if (!f) return;
    const text = await f.text();
    setCsv({ name: f.name, text });
    setKey(likelyKey(csvColumns(text), keys));
    setSaid(null);
  };
  const params = csv ? { target, csv: csv.text, key, mode } : null;
  const ask = params ? JSON.stringify({ t: target, n: csv?.name, l: csv?.text.length, key, mode }) : '';
  useEffect(() => {
    if (!params) return setReport(null);
    let live = true;
    void recordsPreview(params)
      .then((r) => live && setReport(r))
      .catch((e) => live && (setReport(null), setSaid(refused(String(e)))));
    return () => {
      live = false;
    };
    // `ask` stands for the target, the file, the key and the mode.
  }, [ask]);

  const write = async () => {
    if (!params) return;
    setRunning(true);
    const outcome = await runRecord('table.update_records', params);
    setRunning(false);
    if (outcome.error) return setSaid(refused(outcome.error));
    setSaid(ok(`${outcome.record?.messages?.[0] ?? 'Updated'}; one undo puts it back`));
    setCsv(null);
    setReport(null);
  };

  const [one, many] = what;
  const blocked = !report || report.refused.length > 0 || report.changed === 0;
  return (
    <div className="flex shrink-0 flex-col gap-1 border-b border-border bg-muted/20 px-2 py-1.5" data-slot="update-records">
      <div className="flex flex-wrap items-center gap-x-3 gap-y-1">
        <input ref={file} type="file" accept=".csv,text/csv" className="hidden" aria-label="The CSV to update from" onChange={(e) => void choose(e.target.files?.[0]).finally(() => (e.target.value = ''))} />
        <Button variant="outline" size="sm" className="h-6 px-2 text-xs" onClick={() => file.current?.click()} title="A CSV: a header row (a unit in brackets after a name, rd (t/m3)), then a record a row">
          <FileUp className="size-3.5" /> {csv ? csv.name : 'Choose a CSV…'}
        </Button>
        {csv && (
          <span className="flex flex-wrap items-center gap-x-2 gap-y-1" aria-label="Match on">
            <span className="text-muted-foreground">Match on</span>
            {columns.map((c) => (
              <label key={c} className={cn('flex items-center gap-1', key.includes(c) ? 'text-foreground' : 'text-muted-foreground')}>
                <Check checked={key.includes(c)} onChange={(on) => setKey((k) => (on ? [...k, c] : k.filter((x) => x !== c)))} label={`Match on ${c}`} />
                {c}
              </label>
            ))}
          </span>
        )}
        {csv && <Choice value={mode} options={MODES} labels={MODE_LABELS} onChange={setMode} label="Update: the file's values where a record matches, a blank cell leaving a value as it is. Replace: the file's columns replaced, so a blank cell or a record the file hasn't is cleared" />}
        <span className="ml-auto flex items-center gap-1">
          {csv && (
            <Button size="sm" className="h-6 px-2 text-xs" disabled={blocked || running} onClick={() => void write()} title="Write it: one run, one undo">
              {running ? 'Updating…' : report && report.changed > 0 ? `Update ${fmt(report.changed)} value${report.changed === 1 ? '' : 's'}` : 'Update'}
            </Button>
          )}
          <button type="button" className="rounded p-0.5 text-muted-foreground hover:bg-accent hover:text-foreground" aria-label="Close Update records" onClick={onClose}>
            <X className="size-3.5" />
          </button>
        </span>
      </div>
      {report && report.refused.length > 0 && (
        <div className="text-destructive" data-slot="records-refused">
          {report.refused.map((r) => (
            <div key={r}>{r}</div>
          ))}
        </div>
      )}
      {report && report.refused.length === 0 && (
        <div className="flex flex-col gap-0.5" data-slot="records-report">
          <span>
            <b className="tabular-nums">{fmt(report.matched)}</b> of {fmt(report.rows)} record{report.rows === 1 ? '' : 's'} matched <b className="tabular-nums">{fmt(report.targets)}</b> {report.targets === 1 ? one : many}
            {report.target_only > 0 && <span className="text-muted-foreground"> · {fmt(report.target_only)} {report.target_only === 1 ? one : many} matched by none{mode === 'replace' ? ', cleared' : ', left as they are'}</span>}
            {' · '}
            <b className="tabular-nums">{fmt(report.changed)}</b> value{report.changed === 1 ? '' : 's'} would change
          </span>
          {report.file_only_count > 0 && (
            <span className="text-warning" title={report.file_only.join('\n')}>
              {fmt(report.file_only_count)} record{report.file_only_count === 1 ? '' : 's'} in the file only: {report.file_only.slice(0, 6).join(', ')}
              {report.file_only_count > 6 ? ', …' : ''}
            </span>
          )}
          {report.changes.slice(0, 8).map((c, i) => (
            <span key={`${c.record}|${c.column}|${i}`} className="text-muted-foreground">
              {c.record} · {c.column}: <span className="line-through">{c.old ?? 'none'}</span> → <span className="text-edited">{c.new ?? 'none'}</span>
            </span>
          ))}
          {report.changed > 8 && <span className="text-muted-foreground">and {fmt(report.changed - 8)} more</span>}
        </div>
      )}
      <SaidLine said={said} inline />
    </div>
  );
}
