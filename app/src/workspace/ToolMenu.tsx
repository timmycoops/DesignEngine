import { Fragment, type ReactNode } from 'react';
import { ToolButton } from '@/design-system/kit';
import { DropdownMenuItem, DropdownMenuLabel, DropdownMenuSeparator, DropdownMenuShortcut, DropdownMenuSub, DropdownMenuSubContent, DropdownMenuSubTrigger } from '@/design-system/ui/dropdown-menu';
import { type MenuId, type MenuPlace, menuTools, menuWords, tipOf, type Tool, type ToolNeeds } from '@/tools';
import { ToolPicture } from './toolIcons';

/** How the toolbar uses a tool: carries it out, and says how many of what it needs there are (none: it's off). */
export interface ToolUse {
  run: (tool: Tool) => void;
  count: (needs: ToolNeeds) => number;
}

type Placed = Tool & { menu: MenuPlace };

function Item({ tool, use }: { tool: Placed; use: ToolUse }) {
  const count = tool.needs ? use.count(tool.needs) : null;
  return (
    <>
      {tool.menu.rule && <DropdownMenuSeparator />}
      {tool.menu.heading && <DropdownMenuLabel className="text-xs font-normal text-muted-foreground">{tool.menu.heading}</DropdownMenuLabel>}
      <DropdownMenuItem title={tool.tip} disabled={count === 0} data-tool={tool.key} onSelect={() => use.run(tool)}>
        <ToolPicture tool={tool} /> {menuWords(tool, count)}
        {tool.shortcut && <DropdownMenuShortcut>{tool.shortcut}</DropdownMenuShortcut>}
      </DropdownMenuItem>
    </>
  );
}

/** One of the toolbar's menus, drawn from the tool list: its tools top to bottom, with their rules, headings and menus within. */
export function ToolMenuItems({ menu, use }: { menu: MenuId; use: ToolUse }) {
  const tools = menuTools(menu);
  const parts: ReactNode[] = [];
  for (let k = 0; k < tools.length; k++) {
    const tool = tools[k];
    const under = tool.menu.under;
    if (!under) {
      parts.push(<Item key={tool.key} tool={tool} use={use} />);
      continue;
    }
    // Every tool in a row under the same name makes one menu within the menu.
    const within: Placed[] = [];
    while (k < tools.length && tools[k].menu.under?.name === under.name) within.push(tools[k++]);
    k--;
    parts.push(
      <DropdownMenuSub key={under.name}>
        <DropdownMenuSubTrigger title={under.tip}>{under.name}</DropdownMenuSubTrigger>
        <DropdownMenuSubContent>
          {within.map((t) => (
            <Fragment key={t.key}>
              <Item tool={t} use={use} />
            </Fragment>
          ))}
        </DropdownMenuSubContent>
      </DropdownMenuSub>,
    );
  }
  return <>{parts}</>;
}

/** A tool as a button on the toolbar: its icon, with its short name where the row shows one. */
export function BarTool({ tool, use, named }: { tool: Tool; use: ToolUse; named?: boolean }) {
  return (
    <ToolButton tip={tipOf(tool)} disabled={tool.needs ? use.count(tool.needs) === 0 : false} data-tool={tool.key} aria-label={named ? undefined : tool.label} onClick={() => use.run(tool)}>
      <ToolPicture tool={tool} />
      {named && ` ${tool.short ?? tool.label}`}
    </ToolButton>
  );
}
