import { Command } from 'cmdk';
import { Search } from 'lucide-react';
import { findGroups, findScore, type Tool, toolByKey } from '@/tools';
import type { ToolUse } from './ToolMenu';
import { ToolPicture } from './toolIcons';

/**
 * Find a command (decision 0076): every tool in the tool list, a stage at a time, found by typing words from its name,
 * its stage, what it does or its engine command. Enter, or a click, carries the tool out as its button would: its panel
 * opens set to it, or its action runs. A tool that needs something it hasn't got (a selection, something to undo) is
 * listed but off. Ctrl+K opens it from anywhere, typing in a box included; Esc closes it.
 */
export function FindCommand({ open, onOpenChange, use }: { open: boolean; onOpenChange: (open: boolean) => void; use: ToolUse }) {
  const pick = (tool: Tool) => {
    onOpenChange(false);
    // Once the list has closed and given the keys back, so a tool that takes the next click or key gets it.
    window.setTimeout(() => use.run(tool), 0);
  };
  return (
    <Command.Dialog
      open={open}
      onOpenChange={onOpenChange}
      label="Find a command"
      // Matched by whole words typed, ranked by where they're found (`findScore`), not by scattered letters.
      filter={(key, typed) => {
        const tool = toolByKey(key);
        return tool ? findScore(tool, typed) : 0;
      }}
      overlayClassName="fixed inset-0 z-50 bg-black/40"
      contentClassName="fixed top-24 left-1/2 z-50 w-[36rem] max-w-[calc(100vw-2rem)] -translate-x-1/2 overflow-hidden rounded-lg border bg-popover text-popover-foreground shadow-2xl outline-none"
      data-slot="find-command"
    >
      <div className="flex items-center gap-2 border-b px-3">
        <Search className="size-4 shrink-0 text-muted-foreground" />
        <Command.Input placeholder="Find a command: a tool's name, its stage, or what it does" aria-label="Find a command" className="h-10 min-w-0 flex-1 bg-transparent text-sm outline-none placeholder:text-muted-foreground" />
        <kbd className="shrink-0 rounded border px-1 text-[10px] text-muted-foreground">Esc</kbd>
      </div>
      <Command.List className="max-h-[26rem] overflow-y-auto p-1">
        <Command.Empty className="px-3 py-6 text-center text-xs text-muted-foreground">No tool by that name.</Command.Empty>
        {findGroups().map((group) => (
          <Command.Group
            key={group.title}
            heading={group.title}
            className="[&_[cmdk-group-heading]]:px-2 [&_[cmdk-group-heading]]:pt-2 [&_[cmdk-group-heading]]:pb-1 [&_[cmdk-group-heading]]:text-[11px] [&_[cmdk-group-heading]]:font-semibold [&_[cmdk-group-heading]]:tracking-wide [&_[cmdk-group-heading]]:text-muted-foreground [&_[cmdk-group-heading]]:uppercase"
          >
            {group.tools.map((tool) => (
              <Command.Item
                key={tool.key}
                value={tool.key}
                disabled={tool.needs ? use.count(tool.needs) === 0 : false}
                data-tool={tool.key}
                className="flex cursor-pointer items-center gap-2 rounded-md px-2 py-1.5 text-xs data-[disabled=true]:cursor-default data-[disabled=true]:opacity-50 data-[selected=true]:bg-selection data-[selected=true]:text-primary [&_svg]:size-3.5 [&_svg]:shrink-0"
                onSelect={() => pick(tool)}
              >
                <span className="flex w-4 justify-center">
                  <ToolPicture tool={tool} />
                </span>
                <span className="shrink-0 font-medium">{tool.label}</span>
                <span className="min-w-0 flex-1 truncate text-muted-foreground">{tool.tip.charAt(0).toUpperCase() + tool.tip.slice(1)}</span>
                {tool.shortcut && <kbd className="shrink-0 rounded border px-1 text-[10px] text-muted-foreground">{tool.shortcut}</kbd>}
              </Command.Item>
            ))}
          </Command.Group>
        ))}
      </Command.List>
    </Command.Dialog>
  );
}
