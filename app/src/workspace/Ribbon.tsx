import { useLayoutEffect, useRef, useState } from 'react';
import { ChevronDown, Search } from 'lucide-react';
import { cn } from 'cn';
import { Button } from '@/design-system/ui/button';
import { DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuSeparator, DropdownMenuShortcut, DropdownMenuSub, DropdownMenuSubContent, DropdownMenuSubTrigger, DropdownMenuTrigger } from '@/design-system/ui/dropdown-menu';
import { Separator } from '@/design-system/ui/separator';
import { ALWAYS_ROW, EDIT_ROW, type Group, panelTool, PROJECT_MENU, type RowItem, type RowMenu, ROWS, rowName, type Side, STAGES, toolByKey } from '@/tools';
import { PANELS } from './panels';
import { CaseRun } from '@/panels/CaseRun';
import { BarTool, RowMenuItems, RowToolItem, type ToolUse } from './ToolMenu';
import { IconNamed } from './toolIcons';
import { toolShortcut } from '@/viewport/viewControls';

export type StageId = Group;

/** The room More takes at the end of a row that doesn't fit (CSS px): its button and the gap before it. */
const MORE_ROOM = 76;
/** The gap between a row's items (CSS px): `gap-0.5`. */
const GAP = 2;

/** A menu on a row: Import, Export, or a few tools under a name. */
function RowMenuButton({ menu, use }: { menu: RowMenu; use: ToolUse }) {
  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button variant="ghost" size="sm" className="px-2" title={menu.tip} data-row-menu={menu.label}>
          <IconNamed name={menu.icon} /> {menu.label} <ChevronDown className="text-muted-foreground" />
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="start">
        <RowMenuItems menu={menu} use={use} />
      </DropdownMenuContent>
    </DropdownMenu>
  );
}

function RowPart({ item, use }: { item: RowItem; use: ToolUse }) {
  if (item === '|') return <Separator orientation="vertical" className="mx-1 h-5 shrink-0" />;
  if (typeof item !== 'string') return <RowMenuButton menu={item} use={use} />;
  const tool = toolByKey(item);
  return tool ? <BarTool tool={tool} use={use} named /> : null;
}

/** What didn't fit the row, as a menu: each tool under its row name, a rule where the row had one, a row's menu as a menu within. */
function More({ items, use }: { items: readonly RowItem[]; use: ToolUse }) {
  // No rule first or last, where the row's rule fell at the fold.
  const shown = items.filter((item, k) => item !== '|' || (k > 0 && k < items.length - 1));
  // The tool in use, if it's one folded in here: More is lit and says which, as the tool's own button would be.
  const inUse = items.flatMap((item) => (typeof item === 'string' && item !== '|' ? [toolByKey(item)] : [])).find((t) => t && use.lit(t)) ?? null;
  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button
          variant="ghost"
          size="sm"
          className={cn('shrink-0 px-2', inUse && 'bg-selection text-primary')}
          title={inUse ? `${rowName(inUse)} is in use; the tools that don't fit the row` : "The tools that don't fit the row"}
          data-slot="row-more"
          data-tool={inUse?.key}
          aria-pressed={inUse ? true : undefined}
        >
          <IconNamed name="more" /> {inUse ? `More: ${rowName(inUse)}` : 'More'} <ChevronDown className="text-muted-foreground" />
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end">
        {shown.map((item, k) => {
          if (item === '|') return <DropdownMenuSeparator key={k} />;
          if (typeof item !== 'string') {
            return (
              <DropdownMenuSub key={item.label}>
                <DropdownMenuSubTrigger title={item.tip}>
                  <IconNamed name={item.icon} /> {item.label}
                </DropdownMenuSubTrigger>
                <DropdownMenuSubContent>
                  <RowMenuItems menu={item} use={use} />
                </DropdownMenuSubContent>
              </DropdownMenuSub>
            );
          }
          const tool = toolByKey(item);
          return tool ? <RowToolItem key={item} tool={tool} use={use} /> : null;
        })}
      </DropdownMenuContent>
    </DropdownMenu>
  );
}

/**
 * A stage's row of tools. Every tool is named in words; the ones that don't fit the window fold into More at the row's
 * end, so nothing is clipped. Each item is measured once, when the row is first drawn whole, and the fold follows the
 * row's width from then on. Made again for each stage (its `key`), so each row is measured afresh.
 */
function ToolRow({ items, use }: { items: readonly RowItem[]; use: ToolUse }) {
  const row = useRef<HTMLDivElement>(null);
  const widths = useRef<number[] | null>(null);
  /** The room More takes: as allowed for until it's drawn, then as wide as it is, with the gap before it. */
  const moreRoom = useRef(MORE_ROOM);
  const [fit, setFit] = useState(items.length);

  useLayoutEffect(() => {
    const element = row.current;
    if (!element) return;
    const fold = () => {
      if (!widths.current) {
        const parts = [...element.querySelectorAll<HTMLElement>(':scope > [data-row-part]')];
        if (parts.length !== items.length) return;
        widths.current = parts.map((p) => p.getBoundingClientRect().width);
      }
      const all = widths.current;
      const room = element.clientWidth;
      const whole = all.reduce((sum, w) => sum + w + GAP, -GAP);
      if (whole <= room) return setFit(all.length);
      let used = 0;
      let count = 0;
      for (const w of all) {
        if (used + w > room - moreRoom.current) break;
        used += w + GAP;
        count++;
      }
      // A rule is never the last thing before More.
      while (count > 0 && items[count - 1] === '|') count--;
      setFit(count);
    };
    fold();
    const resized = new ResizeObserver(fold);
    resized.observe(element);
    return () => resized.disconnect();
  }, [items]);

  // More drawn wider than allowed for (its words, the icons): the row folds again with its real width, so nothing before
  // it is clipped.
  useLayoutEffect(() => {
    const more = row.current?.querySelector<HTMLElement>(':scope > [data-slot="row-more"]');
    const element = row.current;
    if (!more || !element || !widths.current) return;
    const needed = Math.ceil(more.getBoundingClientRect().width) + GAP;
    if (needed <= moreRoom.current) return;
    moreRoom.current = needed;
    const room = element.clientWidth;
    let used = 0;
    let count = 0;
    for (const w of widths.current) {
      if (used + w > room - needed) break;
      used += w + GAP;
      count++;
    }
    while (count > 0 && items[count - 1] === '|') count--;
    setFit(count);
  }, [fit, items]);

  return (
    <div ref={row} data-slot="tool-row-tools" className="flex min-w-0 flex-1 items-center gap-0.5 overflow-hidden">
      {items.slice(0, fit).map((item, k) => (
        <span key={k} data-row-part className="flex shrink-0 items-center">
          <RowPart item={item} use={use} />
        </span>
      ))}
      {fit < items.length && <More items={items.slice(fit)} use={use} />}
    </div>
  );
}

/**
 * The project menu (☰, decision 0108): New, Open, Save as, Import, Export, Licence and View controls, drawn from the
 * tool list's `PROJECT_MENU`, Open, Import and Export as menus within it.
 */
function ProjectMenu({ use }: { use: ToolUse }) {
  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button variant="ghost" size="sm" className="h-6 px-1.5" title="The project: New, Open, Save as, Import, Export, Licence and View controls" data-slot="project-menu" aria-label="Project menu">
          <IconNamed name="menu" />
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="start">
        {PROJECT_MENU.map((item, k) => {
          if (item === '|') return <DropdownMenuSeparator key={k} />;
          if (typeof item !== 'string') {
            return (
              <DropdownMenuSub key={item.label}>
                <DropdownMenuSubTrigger title={item.tip} data-row-menu={item.label}>
                  <IconNamed name={item.icon} /> {item.label}
                </DropdownMenuSubTrigger>
                <DropdownMenuSubContent>
                  <RowMenuItems menu={item} use={use} />
                </DropdownMenuSubContent>
              </DropdownMenuSub>
            );
          }
          const tool = toolByKey(item);
          return tool ? <RowToolItem key={item} tool={tool} use={use} /> : null;
        })}
      </DropdownMenuContent>
    </DropdownMenu>
  );
}

/** Design or Tables (decision 0108): which side of the app is in front, each with its own layout. */
function SideSwitch({ side, onSide }: { side: Side; onSide: (side: Side) => void }) {
  const sides: { id: Side; title: string; tip: string }[] = [
    { id: 'design', title: 'Design', tip: 'Design: strings, walls, surfaces, solids and geology, in the view' },
    { id: 'tables', title: 'Tables', tip: 'Tables: inputs, cases, dataflows, parameters and results, on to the schedule' },
  ];
  return (
    <div role="radiogroup" aria-label="Side of the app" data-slot="side-switch" className="ml-3 flex h-6 items-center rounded-md border border-input p-0.5">
      {sides.map((s) => (
        <button
          key={s.id}
          type="button"
          role="radio"
          aria-checked={s.id === side}
          data-side={s.id}
          title={s.tip}
          className={cn(
            'h-full rounded-[5px] px-3 text-[0.8rem] font-medium transition-colors outline-none focus-visible:ring-2 focus-visible:ring-ring',
            s.id === side ? 'bg-primary text-primary-foreground' : 'text-muted-foreground hover:text-foreground',
          )}
          onClick={() => onSide(s.id)}
        >
          {s.title}
        </button>
      ))}
    </div>
  );
}

/**
 * The ribbon (decisions 0076, 0108). Along the top: the project menu, the project's name, Save, Undo and Redo, the
 * Design or Tables switch, on Design a tab for each group of tools and on Tables the case and its Run, and Find a command.
 * Under it on Design, the group's row of tools named in words, with Move, Rotate, Mirror and Delete, Workflow and the
 * Panels menu at its end; on Tables no row, as the side's list down its left is its row. Switching group changes the row
 * only; switching side changes the layout.
 */
export function Ribbon({
  projectName,
  projectPath,
  side,
  onSide,
  stage,
  onStage,
  onFind,
  use,
}: {
  projectName: string;
  projectPath: string | null;
  side: Side;
  onSide: (side: Side) => void;
  stage: StageId;
  onStage: (stage: StageId) => void;
  onFind: () => void;
  use: ToolUse;
}) {
  const design = side === 'design';
  return (
    <header data-slot="ribbon" data-side={side} className="shrink-0 border-b bg-card">
      <div data-slot="tab-row" className="flex h-[31px] items-center gap-0.5 px-2">
        <ProjectMenu use={use} />
        <span className="max-w-60 truncate px-1.5 text-[0.8rem] font-semibold" title={projectPath ?? 'Not saved yet'} data-slot="project-name">
          {projectName}
        </span>
        {['project.save', 'edit.undo', 'edit.redo'].map((key) => (
          <BarTool key={key} tool={toolByKey(key)!} use={use} />
        ))}
        <SideSwitch side={side} onSide={onSide} />
        {/* The one place on the Tables side to run (decision 0108, workstream 4). */}
        {!design && <CaseRun />}
        {design && (
          <div role="tablist" aria-label="Groups of design tools" className="ml-2 flex h-full items-stretch">
            {STAGES.map((s) => (
              <button
                key={s.id}
                type="button"
                role="tab"
                aria-selected={s.id === stage}
                data-stage={s.id}
                className={cn(
                  'px-3 text-[0.8rem] transition-colors outline-none focus-visible:bg-muted/50',
                  s.id === stage ? 'bg-background font-medium text-foreground shadow-[inset_0_2px_0_var(--primary)]' : 'text-muted-foreground hover:text-foreground',
                )}
                onClick={() => onStage(s.id)}
              >
                {s.title}
              </button>
            ))}
          </div>
        )}
        <button
          type="button"
          data-slot="find-button"
          title="Find a command (Ctrl+K): any tool, on either side, by typing part of its name"
          className="ml-auto flex h-6 w-60 min-w-0 shrink items-center gap-2 overflow-hidden rounded-md border border-input bg-background px-2 text-xs whitespace-nowrap text-muted-foreground hover:text-foreground"
          onClick={onFind}
        >
          <Search className="size-3.5 shrink-0" />
          <span className="min-w-0 flex-1 truncate text-left">Find a command…</span>
          <kbd className="shrink-0 rounded border px-1 text-[10px]">Ctrl K</kbd>
        </button>
      </div>
      <div data-slot="tool-row" className={cn('flex h-9 items-center gap-0.5 border-t bg-background px-2', !design && 'hidden')}>
        {design && <ToolRow key={stage} items={ROWS[stage]} use={use} />}
        <div className="ml-2 flex shrink-0 items-center gap-0.5">
          {design && (
            <div data-slot="edit-row" className="mr-1 flex items-center gap-0.5 border-l pl-2">
              {EDIT_ROW.map((key) => (
                <BarTool key={key} tool={toolByKey(key)!} use={use} named />
              ))}
            </div>
          )}
          {design &&
            ALWAYS_ROW.map((key) => (
              <BarTool key={key} tool={toolByKey(key)!} use={use} named />
            ))}
          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <Button variant="ghost" size="sm" className="px-2" data-slot="panels-menu">
                <IconNamed name="panels" /> Panels <ChevronDown className="text-muted-foreground" />
              </Button>
            </DropdownMenuTrigger>
            <DropdownMenuContent align="end">
              {PANELS.filter((p) => p.dock !== 'item').map((p) => (
                <DropdownMenuItem key={p.id} data-tool={p.id} onSelect={() => use.run(panelTool(p.id))}>
                  {panelTool(p.id).label}
                </DropdownMenuItem>
              ))}
              <DropdownMenuSeparator />
              {['panels.hide', 'panels.reset'].map((key) => (
                <DropdownMenuItem key={key} data-tool={key} onSelect={() => use.run(toolByKey(key)!)}>
                  {toolByKey(key)!.label}
                  {toolShortcut(toolByKey(key)!) && <DropdownMenuShortcut>{toolShortcut(toolByKey(key)!)}</DropdownMenuShortcut>}
                </DropdownMenuItem>
              ))}
            </DropdownMenuContent>
          </DropdownMenu>
        </div>
      </div>
    </header>
  );
}
