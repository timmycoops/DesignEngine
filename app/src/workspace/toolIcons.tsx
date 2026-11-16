import { FileInput, FileOutput, FilePlus, FolderOpen, Hammer, type LucideIcon, MountainSnow, PanelsTopLeft, Redo2, Save, SaveAll, Sheet, Sigma, Slice, SlidersHorizontal, Table2, Undo2, Workflow } from 'lucide-react';
import type { Tool, ToolIcon } from '@/tools';

/** The picture for each icon the tool list names. A name with no picture doesn't compile. */
const ICONS: Record<ToolIcon, LucideIcon> = {
  new: FilePlus,
  open: FolderOpen,
  save: Save,
  'save-as': SaveAll,
  undo: Undo2,
  redo: Redo2,
  import: FileInput,
  export: FileOutput,
  face: MountainSnow,
  builder: Hammer,
  cut: Slice,
  reserves: Sigma,
  blocks: Table2,
  'reserves-table': Sheet,
  parameters: SlidersHorizontal,
  workflow: Workflow,
  panels: PanelsTopLeft,
};

/** A tool's icon, or nothing where it has none. */
export function ToolPicture({ tool }: { tool: Tool }) {
  const Icon = tool.icon ? ICONS[tool.icon] : null;
  return Icon ? <Icon /> : null;
}

/** An icon from the tool list by name, for what isn't a tool: a menu's own button. */
export function IconNamed({ name }: { name: ToolIcon }) {
  const Icon = ICONS[name];
  return <Icon />;
}
