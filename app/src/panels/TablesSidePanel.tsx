import { ChevronsLeft, ChevronsRight, PictureInPicture2, SquareArrowOutUpRight } from 'lucide-react';
import { useEffect, useMemo, useRef, useState, useSyncExternalStore } from 'react';
import { ObjectMenu, PanelHead } from '@/design-system/kit';
import { cn } from 'cn';
import { type CaseHeadline, type ObjectSummary } from '@/bridge';
import { useWorkspace } from '@/workspace/context';
import { itemNow, type ListRow, listOf, OBJECTS, onItem, openItem, popBack, popOut, poppedNow, SECTIONS, type Section } from '@/workspace/tablesSide';
import { PortalRoot } from '@/design-system/portalRoot';
import { BlockTable } from './BlockTablePanel';
import { useCases } from './CasesBar';
import { Parameters } from './ParametersPanel';
import { ReservesCasePanel } from './ReservesCasePanel';
import { ReservesTable } from './ReservesTablePanel';

/** A new script being written, as Parameters names it. */
const NEW_SCRIPT = 'new-script';

/** Whether the list is folded to its numbers, remembered on this machine. */
const FOLD_KEY = 'zd.tables.list.folded.v1';
/** The panel's width under which its list folds by itself. */
const NARROW_PX = 900;
const foldedAtStart = () => {
  try {
    return localStorage.getItem(FOLD_KEY) === 'yes';
  } catch {
    return false;
  }
};

/**
 * The Tables side (decision 0108, workstream 4), one panel filling the window: a list down the left in the order the work
 * flows (1 Inputs, 2 Cases, 3 Dataflows, 4 Parameters, 5 Results, 6 Schedule), and the item picked in the middle. The case
 * is run from the top row. The case and the parameters stay in the page once opened, as they hold where you were and any
 * edit not yet saved.
 */
export function TablesSidePanel() {
  const { summary } = useWorkspace();
  const cases = useCases();
  const open = useSyncExternalStore(onItem, itemNow);
  const rows = useMemo(() => listOf(summary?.objects ?? [], cases.heads), [summary, cases.heads]);
  // An item with nothing picked yet shows its section's first: Base for a case or a result.
  const firstOf = (section: Section) => {
    const base = section === 'case' || section === 'results' ? cases.base?.table : undefined;
    return base ?? rows.find((r) => r.section === section)?.id ?? null;
  };
  const known = (section: Section, id: string | null) => id === NEW_SCRIPT || rows.some((r) => r.section === section && r.id === id);
  const id = open.id && known(open.section, open.id) ? open.id : firstOf(open.section);
  const section = open.section;

  // The case and the parameters are kept in the page once seen (their place in a step, their edits), the rest made as asked.
  const [seen, setSeen] = useState<Set<'case' | 'params'>>(new Set());
  const keeps = section === 'case' ? 'case' : section === 'params' || section === 'dataflows' ? 'params' : null;
  useEffect(() => {
    if (keeps && !seen.has(keeps)) setSeen(new Set([...seen, keeps]));
  }, [keeps]);
  const caseShown = section === 'case' || section === 'results' ? id : null;
  // The list folds to its six numbers, for the room a case's three columns want on a narrow window.
  const [foldedByHand, setFolded] = useState(foldedAtStart);
  // Narrower than this, as in a small window of its own, the list folds by itself: the item wants the room.
  const box = useRef<HTMLDivElement>(null);
  const [narrow, setNarrow] = useState(false);
  useEffect(() => {
    const el = box.current;
    if (!el) return;
    const seen = new ResizeObserver(() => setNarrow(el.clientWidth > 0 && el.clientWidth < NARROW_PX));
    seen.observe(el);
    return () => seen.disconnect();
  }, []);
  const folded = foldedByHand || narrow;
  // In a window of its own (decision 0109): its menus and lists open on its page, not the main window's.
  const popped = useSyncExternalStore(onItem, poppedNow);
  const [page, setPage] = useState<HTMLElement | null>(null);
  useEffect(() => {
    const frame = requestAnimationFrame(() => setPage(popped ? (box.current?.ownerDocument.body ?? null) : null));
    return () => cancelAnimationFrame(frame);
  }, [popped]);
  const windowButton = (
    <button
      type="button"
      className="rounded p-0.5 text-muted-foreground hover:bg-accent hover:text-foreground"
      onClick={() => (popped ? popBack() : popOut())}
      title={popped ? 'Back into the main window' : 'Pop out into a window of its own, for a second monitor'}
      aria-label={popped ? 'Back into the main window' : 'Pop out'}
      data-slot="tables-pop"
      data-popped={popped ? 'yes' : 'no'}
    >
      {popped ? <PictureInPicture2 className="size-3.5" /> : <SquareArrowOutUpRight className="size-3.5" />}
    </button>
  );
  const fold = (to: boolean) => {
    setFolded(to);
    try {
      localStorage.setItem(FOLD_KEY, to ? 'yes' : 'no');
    } catch {
      // Storage unavailable: the fold just isn't remembered.
    }
  };
  const heads = new Map(cases.heads.map((h) => [h.table, h]));
  const byId = new Map((summary?.objects ?? []).map((o) => [o.id, o]));

  return (
    <PortalRoot.Provider value={page}>
      <div ref={box} className="flex h-full min-h-0 text-xs" data-tool-panel="tables-side" data-section={section} data-item={id ?? ''} data-narrow={narrow ? 'yes' : 'no'}>
        {folded && (
          <nav className="flex w-8 shrink-0 flex-col items-center gap-1 border-r border-border bg-card py-1" aria-label="Tables, down the chain, folded" data-slot="tables-list" data-folded="yes">
            {!narrow && (
              <button type="button" className="rounded p-1 text-muted-foreground hover:bg-accent hover:text-foreground" onClick={() => fold(false)} title="Open the list" aria-label="Open the list" data-slot="tables-list-fold">
                <ChevronsRight className="size-3.5" />
              </button>
            )}
            {windowButton}
            {SECTIONS.map((s, n) => (
              <button key={s.id} type="button" className={cn('w-6 rounded py-0.5 text-center tabular-nums hover:bg-accent', section === s.id && 'bg-selection font-semibold text-primary')} onClick={() => openItem(s.id)} title={`${n + 1} · ${s.title}: ${s.tip}`} data-slot="tables-section-folded" data-section={s.id}>
                {n + 1}
              </button>
            ))}
          </nav>
        )}
        <nav className={cn('flex w-56 shrink-0 flex-col overflow-auto border-r border-border bg-card', folded && 'hidden')} aria-label="Tables, down the chain" data-slot="tables-list">
          <div className="flex items-center justify-between border-b border-border px-2 py-1">
            <span className="font-semibold">Down the chain</span>
            <span className="ml-auto mr-1">{windowButton}</span>
            <button type="button" className="rounded p-0.5 text-muted-foreground hover:bg-accent hover:text-foreground" onClick={() => fold(true)} title="Fold the list to its numbers, for the room" aria-label="Fold the list" data-slot="tables-list-fold">
              <ChevronsLeft className="size-3.5" />
            </button>
          </div>
          {SECTIONS.map((s, n) => (
            <div key={s.id} className="pb-1" data-slot="tables-section" data-section={s.id}>
              <button
                type="button"
                className={cn('flex w-full items-baseline justify-between px-2 pt-2 pb-0.5 text-left text-[11px] font-semibold text-muted-foreground uppercase hover:text-foreground', section === s.id && 'text-foreground')}
                title={s.tip}
                onClick={() => openItem(s.id)}
                data-slot="tables-section-head"
                data-section={s.id}
              >
                <span>
                  {n + 1} · {s.title}
                </span>
                {s.id === 'inputs' && <span className="font-normal normal-case">from Design</span>}
              </button>
              {rows
                .filter((r) => r.section === s.id)
                .map((r) => (
                  <ListItem
                    key={`${r.section}:${r.id}`}
                    row={r}
                    lit={section === r.section && id === r.id}
                    head={r.section === 'case' && r.id ? heads.get(r.id) : undefined}
                    object={(r.section === 'dataflows' || r.section === 'params') && r.id ? byId.get(r.id) : undefined}
                  />
                ))}
              {s.id === 'case' && cases.heads.length === 0 && <div className="px-3 text-muted-foreground">none yet: + in a case's header, or New case</div>}
              {s.id === 'dataflows' && (
                <button type="button" className={cn('block w-full truncate py-0.5 pl-3 text-left text-muted-foreground hover:bg-accent hover:text-foreground', section === 'dataflows' && id === NEW_SCRIPT && 'bg-selection text-primary')} onClick={() => openItem('dataflows', NEW_SCRIPT)} data-slot="new-script">
                  + New script
                </button>
              )}
            </div>
          ))}
        </nav>
        <main className="flex min-w-0 flex-1 flex-col" data-slot="tables-item-view">
          {section === 'inputs' && (id === OBJECTS || !id ? <BlockTable /> : <ReservesTable key={id} tableId={id} picker={null} onDeleted={() => openItem('inputs', OBJECTS)} />)}
          {section === 'results' && id && <ReservesTable key={id} tableId={id} picker={null} onDeleted={() => openItem('results', null)} />}
          {section === 'results' && !id && <Empty words="No results yet: a case's run makes its reserves table." />}
          {section === 'schedule' && <Schedule />}
          {seen.has('case') && (
            <div className={cn('min-h-0 flex-1', section !== 'case' && 'hidden')}>
              {cases.heads.length > 0 ? <ReservesCasePanel caseTable={section === 'case' ? id : caseShown} /> : <Empty words="No case yet. A case is a reserves table: make one from a block layer with New case (the + in a case's header) or from Python." />}
            </div>
          )}
          {seen.has('params') && (
            <div className={cn('min-h-0 flex-1', section !== 'params' && section !== 'dataflows' && 'hidden')}>
              <Parameters shown={section === 'params' || section === 'dataflows' ? id : null} list={false} />
            </div>
          )}
        </main>
      </div>
    </PortalRoot.Provider>
  );
}

/** The figures a case carries on its row of the list, as its pill did: the case's headline, for a check to read. */
const FIGURES = ['insitu_coal', 'rom_coal', 'rom_waste', 'strip_ratio'] as const;

/** A row of the list: its name, and what it is beside it; the one open lit. */
function ListItem({ row, lit, head, object }: { row: ListRow; lit: boolean; head?: CaseHeadline; object?: ObjectSummary }) {
  const figures = head ? Object.fromEntries([['data-case', head.name], ['data-base', head.base ? 'yes' : 'no'], ...FIGURES.map((f) => [`data-${f}`, head[f] ?? ''])]) : {};
  const button = (
    <button
      {...figures}
      type="button"
      aria-current={lit ? 'true' : undefined}
      className={cn('flex w-full items-baseline gap-2 py-0.5 pr-2 pl-3 text-left hover:bg-accent', lit && 'bg-selection font-semibold text-primary')}
      onClick={() => openItem(row.section, row.id)}
      data-slot="tables-item"
      data-section={row.section}
      data-id={row.id ?? ''}
      data-name={row.name}
      title={row.name}
    >
      <span className="min-w-0 flex-1 truncate">{row.name}</span>
      {row.note && <span className="shrink-0 text-[10px] font-normal text-muted-foreground">{row.note}</span>}
    </button>
  );
  // A parameter table, dataflow or script lit: its menu beside it (rename, copy, delete), as the Parameters list had.
  if (!object || !lit) return button;
  return (
    <div className="flex items-center pr-1">
      {button}
      <ObjectMenu object={object} what={object.kind === 'dataflow' ? 'dataflow' : object.kind === 'script' ? 'script' : 'parameter table'} onDeleted={() => openItem(row.section, null)} />
    </div>
  );
}

/** The schedule's place on the Tables side, kept for the scheduler (0100). */
function Schedule() {
  return (
    <div className="flex min-h-0 flex-col" data-slot="tables-schedule">
      <PanelHead line="The schedule quantities a case's run gives, the last stop on the Tables side." />
      <div className="space-y-2 p-3 text-muted-foreground">
        <p>
          <span className="font-semibold text-foreground">Not built yet.</span> The scheduler (decision 0100) maps to a case's Scheduled folder: its quantities, never worked out again, so the scheduled totals equal the reserves. Its stage A waits for the Phase 4 gate.
        </p>
        <p>Until then, a case's Scheduled fields are in its reserves table under Results.</p>
      </div>
    </div>
  );
}

function Empty({ words }: { words: string }) {
  return <div className="p-3 text-muted-foreground">{words}</div>;
}
