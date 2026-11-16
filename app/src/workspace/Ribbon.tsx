import { useLayoutEffect, useRef, useState } from 'react';
import { ChevronDown, Search } from 'lucide-react';
import { cn } from 'cn';
import { Button } from '@/design-system/ui/button';
import { DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuSeparator, DropdownMenuShortcut, DropdownMenuSub, DropdownMenuSubContent, DropdownMenuSubTrigger, DropdownMenuTrigger } from '@/design-system/ui/dropdown-menu';
import { Separator } from '@/design-system/ui/separator';
import { ALWAYS_ROW, panelTool, type RowItem, type RowMenu, ROWS, type Stage, STAGES, toolByKey } from '@/tools';
import { PANELS } from './panels';
import { BarTool, RowMenuItems, RowToolItem, ToolMenuItems, type ToolUse } from './ToolMenu';
import { IconNamed } from './toolIcons';

export type StageId = Exclude<Stage, 'always'>;

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
  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button variant="ghost" size="sm" className="shrink-0 px-2" title="The tools that don't fit the row" data-slot="row-more">
          <IconNamed name="more" /> More <ChevronDown className="text-muted-foreground" />
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
        if (used + w > room - MORE_ROOM) break;
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
 * The ribbon (decision 0076): a tab a stage, in the order the work is done, and under it the row of that stage's
 * tools, named in words. The project's menu, Save, Undo and Redo sit before the tabs and Find a command after them;
 * Workflow and the Panels menu at the end of the row. Switching stage changes the row only: the panels stay where they were left.
 */
export function Ribbon({ projectName, projectPath, stage, onStage, onFind, use }: { projectName: string; projectPath: string | null; stage: StageId; onStage: (stage: StageId) => void; onFind: () => void; use: ToolUse }) {
  return (
    <header data-slot="ribbon" className="shrink-0 border-b bg-card">
      <div data-slot="tab-row" className="flex h-[31px] items-center gap-0.5 px-2">
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <Button variant="ghost" size="sm" className="h-6 max-w-60 font-semibold" title={projectPath ?? 'Not saved yet'} data-slot="project-menu">
              <span className="truncate">{projectName}</span>
              <ChevronDown className="text-muted-foreground" />
            </Button>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="start">
            <ToolMenuItems menu="project" use={use} />
          </DropdownMenuContent>
        </DropdownMenu>
        {['project.save', 'edit.undo', 'edit.redo'].map((key) => (
          <BarTool key={key} tool={toolByKey(key)!} use={use} />
        ))}
        <div role="tablist" aria-label="Stages of the work" className="ml-3 flex h-full items-stretch">
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
        <button
          type="button"
          data-slot="find-button"
          title="Find a command (Ctrl+K): any tool, from any stage, by typing part of its name"
          className="ml-auto flex h-6 w-60 min-w-0 shrink items-center gap-2 overflow-hidden rounded-md border border-input bg-background px-2 text-xs whitespace-nowrap text-muted-foreground hover:text-foreground"
          onClick={onFind}
        >
          <Search className="size-3.5 shrink-0" />
          <span className="min-w-0 flex-1 truncate text-left">Find a command…</span>
          <kbd className="shrink-0 rounded border px-1 text-[10px]">Ctrl K</kbd>
        </button>
      </div>
      <div data-slot="tool-row" className="flex h-9 items-center gap-0.5 border-t bg-background px-2">
        <ToolRow key={stage} items={ROWS[stage]} use={use} />
        <div className="ml-2 flex shrink-0 items-center gap-0.5">
          {ALWAYS_ROW.map((key) => (
            <BarTool key={key} tool={toolByKey(key)!} use={use} named />
          ))}
          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <Button variant="ghost" size="sm" className="px-2" data-slot="panels-menu">
                <IconNamed name="panels" /> Panels <ChevronDown className="text-muted-foreground" />
              </Button>
            </DropdownMenuTrigger>
            <DropdownMenuContent align="end">
              {PANELS.map((p) => (
                <DropdownMenuItem key={p.id} data-tool={p.id} onSelect={() => use.run(panelTool(p.id))}>
                  {panelTool(p.id).label}
                </DropdownMenuItem>
              ))}
              <DropdownMenuSeparator />
              {['panels.hide', 'panels.reset'].map((key) => (
                <DropdownMenuItem key={key} data-tool={key} onSelect={() => use.run(toolByKey(key)!)}>
                  {toolByKey(key)!.label}
                  {toolByKey(key)!.shortcut && <DropdownMenuShortcut>{toolByKey(key)!.shortcut}</DropdownMenuShortcut>}
                </DropdownMenuItem>
              ))}
            </DropdownMenuContent>
          </DropdownMenu>
        </div>
      </div>
    </header>
  );
}
