import { useEffect, useState, useSyncExternalStore } from 'react';
import { cancelRun, type RunProgress, runProgress } from '../../bridge';
import { type Going, progressWords, running, runWords } from '../../running';

/** The run going, app-wide, or null (decision 0060). */
export function useRunning(): Going | null {
  return useSyncExternalStore(running.subscribe, running.get);
}

/** A run shorter than this never shows on the status bar, so quick runs don't flicker. */
const SHOW_AFTER_MS = 300;

/**
 * The run going, on the status bar: what it's doing, how far it has got and how long it's been, with Cancel. Shown once a
 * run has gone a moment; Cancel stops it at the next solid, strip or file, with nothing applied.
 */
export function RunGoing() {
  const going = useRunning();
  const [progress, setProgress] = useState<RunProgress | null>(null);
  const [shown, setShown] = useState(false);

  useEffect(() => {
    setProgress(null);
    setShown(false);
    if (!going) return;
    let live = true;
    const ask = () =>
      void runProgress()
        .then((p) => live && setProgress(p))
        .catch(() => {});
    const show = setTimeout(() => live && setShown(true), SHOW_AFTER_MS);
    const every = setInterval(ask, 250);
    ask();
    return () => {
      live = false;
      clearTimeout(show);
      clearInterval(every);
    };
  }, [going]);

  if (!going || !shown) return null;
  const seconds = progress?.seconds ?? (performance.now() - going.since) / 1000;
  const words = runWords(going.command);
  return (
    <span className="flex shrink-0 items-center gap-2 text-foreground" role="status" aria-live="polite">
      <span className="size-2 animate-pulse rounded-full bg-primary" aria-hidden />
      <span className="first-letter:uppercase">{words}</span>
      <span className="text-muted-foreground tabular-nums">
        {progressWords({ done: progress?.done ?? 0, total: progress?.total ?? 0, of: progress?.of ?? '', seconds, cancelled: progress?.cancelled })}
      </span>
      <button
        type="button"
        className="h-5 rounded border border-input px-2 text-[11px] hover:bg-muted disabled:opacity-50"
        disabled={progress?.cancelled}
        title="Stop it at the next solid, strip or file: nothing it made is kept"
        onClick={() => void cancelRun()}
      >
        Cancel
      </button>
    </span>
  );
}
