import { Fragment, type ReactNode } from 'react';
import { ToolButton } from '@/design-system/kit';
import { DropdownMenuItem, DropdownMenuLabel, DropdownMenuSeparator, DropdownMenuShortcut, DropdownMenuSub, DropdownMenuSubContent, DropdownMenuSubTrigger } from '@/design-system/ui/dropdown-menu';
import { type MenuId, menuTools, menuWords, type RowMenu, rowName, tipOf, type Tool, toolByKey, type ToolNeeds } from '@/tools';
import { ToolPicture } from './toolIcons';

/** How the toolbar uses a tool: carries it out, says how many of what it needs there are (none: it's off), and whether it's the one in use. */
export interface ToolUse {
  run: (tool: Tool) => void;
  count: (needs: ToolNeeds) => number;
  lit: (tool: Tool) => boolean;
}

/** A tool as a menu's item: its rule and heading above it, its words as the menu has them, its keys at the end. */
function Item({ tool, use, words }: { tool: Tool; use: ToolUse; words?: string }) {
  const count = tool.needs ? use.count(tool.needs) : null;
  return (
    <>
      {tool.menu?.rule && <DropdownMenuSeparator />}
      {tool.menu?.heading && <DropdownMenuLabel className="text-xs font-normal text-muted-foreground">{tool.menu.heading}</DropdownMenuLabel>}
      <DropdownMenuItem title={tool.tip} disabled={count === 0} data-tool={tool.key} onSelect={() => use.run(tool)}>
        <ToolPicture tool={tool} /> {words ?? menuWords(tool, count)}
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
    const within: Tool[] = [];
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

/** What a row's menu holds: one of the toolbar's menus whole, or its few tools as their menu words them. */
export function RowMenuItems({ menu, use }: { menu: RowMenu; use: ToolUse }) {
  if (menu.menu) return <ToolMenuItems menu={menu.menu} use={use} />;
  return (
    <>
      {(menu.keys ?? []).map((key) => {
        const tool = toolByKey(key);
        return tool ? <Item key={key} tool={tool} use={use} /> : null;
      })}
    </>
  );
}

/** A tool as a menu's item under its row name: what More holds for a tool that didn't fit its row. */
export function RowToolItem({ tool, use }: { tool: Tool; use: ToolUse }) {
  return <Item tool={{ ...tool, menu: undefined }} use={use} words={rowName(tool)} />;
}

/** A tool as a button on the toolbar: its icon, with its name in words where the row shows one, lit while it's the one in use. */
export function BarTool({ tool, use, named }: { tool: Tool; use: ToolUse; named?: boolean }) {
  return (
    <ToolButton
      tip={tipOf(tool)}
      active={use.lit(tool) || undefined}
      disabled={tool.needs ? use.count(tool.needs) === 0 : false}
      data-tool={tool.key}
      aria-label={named ? undefined : tool.label}
      className={named ? 'px-2' : undefined}
      onClick={() => use.run(tool)}
    >
      <ToolPicture tool={tool} />
      {named && ` ${rowName(tool)}`}
    </ToolButton>
  );
}
