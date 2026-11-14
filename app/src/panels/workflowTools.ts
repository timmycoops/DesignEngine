/**
 * The tools beside the workflow canvas (decision 0067): what can be dragged in to add a step, and where a dropped one goes.
 * A tool is one of what a tool panel does whose button can add its step to a workflow (decision 0065); dropping it opens
 * that panel set to it, its button reading Add to the workflow, the step going in after the step it was dropped on. What
 * that step last made is picked, so a panel taking what's picked starts from it, and the engine finds it again as "what
 * step N made". Kept free of React, so the tests can run it.
 */
import type { Workflow, WorkflowStep } from '../bridge';
import type { PanelId } from '../workspace/Workspace';

export interface CanvasTool {
  /** Unique; the drag carries it. */
  key: string;
  /** The panel's own heading for the group. */
  group: string;
  /** As the panel's choice names it. */
  label: string;
  /** The command the step runs, as the node shows it. */
  command: string;
  panel: PanelId;
  /** Which of what the panel does, as its choice holds it. */
  mode?: string;
  /** What it works on, in a few words. */
  takes: string;
}

/** Every tool whose panel can add a step, in the order the panels show them. */
export const CANVAS_TOOLS: readonly CanvasTool[] = [
  { key: 'builder', group: 'Builder', label: 'Build a step', command: 'face.build_step', panel: 'builder', takes: 'the line picked' },
  { key: 'cut:between', group: 'Cut solids', label: 'Between surfaces', command: 'solid.between_surfaces', panel: 'cut', mode: 'between', takes: 'a closed string, top and bottom' },
  { key: 'cut:strips', group: 'Cut solids', label: 'Into strips', command: 'solid.slice_by_faces', panel: 'cut', mode: 'strips', takes: 'solids and a face a strip' },
  { key: 'cut:cut', group: 'Cut solids', label: 'By a surface', command: 'solid.cut_by_surface', panel: 'cut', mode: 'cut', takes: 'solids and a surface, grid or RL' },
  { key: 'cut:seams', group: 'Cut solids', label: 'By seams', command: 'solid.cut_by_seams', panel: 'cut', mode: 'seams', takes: 'solids and a seam model' },
  { key: 'cut:polygons', group: 'Cut solids', label: 'By polygons', command: 'solid.cut_by_polygons', panel: 'cut', mode: 'polygons', takes: 'solids and block polygons' },
  { key: 'reserves:interrogate', group: 'Reserves', label: 'Interrogate', command: 'solid.interrogate', panel: 'reserves', mode: 'interrogate', takes: 'blocks and a seam model' },
];

export const toolByKey = (key: string): CanvasTool | undefined => CANVAS_TOOLS.find((t) => t.key === key);

/**
 * Where a step dropped on the canvas goes in the workflow's list (from 0): after the step it was dropped on, or at the end
 * when dropped on nothing, an input node or a step no longer there.
 */
export function placeOfDrop(workflow: Workflow, onStep: number | null): number {
  const k = onStep === null ? -1 : workflow.steps.findIndex((s) => s.id === onStep);
  return k < 0 ? workflow.steps.length : k + 1;
}

/** What a step last made or revised that's still in the project, to pick for the tool dropped after it. */
export function madeByStep(step: WorkflowStep | undefined, there: (id: string) => boolean): string[] {
  const last = step?.last;
  if (!last) return [];
  return [...last.made, ...(last.revised ?? [])].filter((id, k, all) => all.indexOf(id) === k && there(id));
}

/** What the canvas says once a tool is dropped: where its step will go, and what's picked for it. */
export function dropWords(workflow: Workflow, name: string, tool: CanvasTool, at: number, picked: number): string {
  const after = at > 0 ? workflow.steps[at - 1] : undefined;
  const where = at >= workflow.steps.length ? `as step ${at + 1}, at the end` : `as step ${at + 1}`;
  const from = after && picked > 0 ? `; what step ${at} made is picked (${picked})` : '';
  return `Set up ${tool.label} in ${tool.group}, then Add to ${name}: it goes in ${where}${from}`;
}
