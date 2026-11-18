import { MoreHorizontal } from 'lucide-react';
import { useRef, useState } from 'react';
import { DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuTrigger } from '@/design-system/ui/dropdown-menu';
import { Input } from '@/design-system/ui/input';
import { useWorkspace } from '@/workspace/context';
import { IconButton } from './badges';

/**
 * Rename and Delete for an object that isn't drawn, beside the list its own tab picks it from (Filtering plan, L4): a
 * face design, a workflow, a parameter table, a dataflow or a reserves table, none of which is in the layer tree. Both
 * are runs, so one undo puts either back. Rename types over the name in place: Enter or leaving the box renames it, Esc
 * leaves it as it was. Show in Properties picks it, for its revisions and history; picking from the list alone doesn't,
 * as that would drop the solids picked, which the Parameters and Reserves table tabs follow.
 */
export function ObjectMenu({ object, what, onDeleted }: { object: { id: string; name: string } | null; what: string; onDeleted?: () => void }) {
  const { run, select, openPanel } = useWorkspace();
  const [renaming, setRenaming] = useState(false);
  const finished = useRef(false);
  if (!object) return null;
  const finish = (value: string | null) => {
    if (finished.current) return;
    finished.current = true;
    setRenaming(false);
    const name = value?.trim();
    if (name && name !== object.name) void run('object.rename', { object: object.id, name });
  };
  if (renaming) {
    return (
      <Input
        autoFocus
        className="h-6 w-40 px-1 text-xs"
        defaultValue={object.name}
        aria-label={`New name for ${object.name}`}
        ref={(box) => {
          // After the menu that opened this has let go of the focus.
          setTimeout(() => box?.select());
        }}
        onBlur={(e) => finish(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === 'Enter') finish(e.currentTarget.value);
          if (e.key === 'Escape') {
            // The rename put back is this press (decision 0080).
            e.preventDefault();
            finish(null);
          }
        }}
      />
    );
  }
  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <IconButton label={`Show in Properties, rename or delete this ${what}`} data-slot="object-menu">
          <MoreHorizontal />
        </IconButton>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end" data-slot="object-menu-items" onCloseAutoFocus={(e) => e.preventDefault()}>
        <DropdownMenuItem
          onSelect={() => {
            select(object.id);
            openPanel('properties');
          }}
        >
          Show in Properties
        </DropdownMenuItem>
        <DropdownMenuItem
          onSelect={() => {
            finished.current = false;
            setRenaming(true);
          }}
        >
          Rename
        </DropdownMenuItem>
        <DropdownMenuItem variant="destructive" onSelect={() => void run('object.delete', { object: object.id }).then((done) => done && onDeleted?.())}>
          Delete {object.name}
        </DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
