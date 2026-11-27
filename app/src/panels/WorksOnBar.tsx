import { concealed } from '@/bridge';
import { Choice } from '@/design-system/kit';
import { useWorkspace } from '@/workspace/context';
import { READ_BY, solidsWorkedOn, WORKS_ON } from './worksOn';

/**
 * What the tool in front works on, at the foot of the view (decision 0108, decision 6): Picked, Its layer or On show,
 * each with how many solids it takes. Shown while a tool that reads it is in front; set once here for all of them.
 */
export function WorksOnBar() {
  const { summary, selection, worksOn, setWorksOn, panelsShown } = useWorkspace();
  if (!panelsShown.some((p) => READ_BY.includes(p))) return null;
  const { labels } = solidsWorkedOn(summary?.objects ?? [], summary?.layers ?? [], selection, worksOn, concealed);
  return (
    <div
      data-slot="works-on"
      className="pointer-events-auto absolute bottom-3 left-1/2 z-10 flex -translate-x-1/2 items-center gap-2 rounded-md border bg-card px-2 py-1 text-xs shadow-sm"
      title="What the tool in front works on: the solids picked, every solid on the first one's layer, or every solid on show"
    >
      <span className="text-muted-foreground">Works on</span>
      <Choice value={worksOn} options={WORKS_ON} labels={labels} onChange={setWorksOn} label="What the tool works on" />
    </div>
  );
}
