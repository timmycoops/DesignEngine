import { Background, Controls, type Edge, Handle, MarkerType, type Node, type NodeProps, Position, ReactFlow } from '@xyflow/react';
import '@xyflow/react/dist/style.css';
import { ListOrdered, PanelLeftClose, PanelLeftOpen, Pause, Scan } from 'lucide-react';
import { useMemo, useState } from 'react';
import type { WorkflowStep } from '@/bridge';
import { Field, IconButton, Select, StatusBadge } from '@/design-system/kit';
import { useWorkspace } from '@/workspace/context';
import { BASE, caseValue, changesOfCase, ruleWords, type SettingRow, settingsOfStep, stepsOfCase, withSetting } from './workflow';
import { type GraphNode, rewire, workflowGraph } from './workflowGraph';
import { LastBadge, plural, StepSettings, usePickedWorkflow } from './workflowParts';
import { CANVAS_TOOLS, type CanvasTool, dropWords, madeByStep, placeOfDrop, toolByKey } from './workflowTools';

/**
 * The workflow canvas (decision 0067): the workflow picked in the Workflow panel drawn as nodes and wires. A node a step,
 * a pause its own kind of node, what steps take from outside the workflow an input node at the left with dashed wires,
 * a wire a step input. Laid out by dependency, the pauses as barriers, worked out each time it's drawn.
 *
 * A click on a node opens the step beside the canvas: its inputs in words and its settings as the case picked has them.
 * Drawing a wire from a step's right edge, or an input node's, onto a step's input changes that input's rule to read
 * from it; a wire from a later step is refused in words. The workflow and case are the Workflow panel's: picking either
 * here picks it there.
 *
 * Tools at the left add steps: one dragged onto a step opens its panel set to it, its button reading Add to the workflow,
 * the step going in after the one it was dropped on, with what that step made picked; dropped on the empty canvas, or
 * clicked, it goes after the step open beside the canvas, or at the end.
 */

/** The drag's own type, so nothing else dropped on the canvas is taken for a tool. */
const TOOL_DRAG = 'application/x-zd-tool';

const COLUMN = 290;
const GAP = 28;
/** A node's height, to stack a column: its header and badges, and a row an input. */
const heightOf = (node: GraphNode) => (node.kind === 'step' ? 64 + 20 * (node.inputs?.length ?? 0) + (node.step?.note ? 16 : 0) : node.kind === 'pause' ? 64 : 40);

interface StepData extends Record<string, unknown> {
  node: GraphNode;
  changed: number;
  focused: boolean;
}
interface InputData extends Record<string, unknown> {
  node: GraphNode;
}

const kindWords: Record<string, string> = { layer: 'layer', named: 'named', picked: 'as picked', gone: 'gone' };

/** The tools, grouped by their panel, each dragged onto the canvas or clicked to add its step. */
function ToolList({ onPick, onClose }: { onPick: (tool: CanvasTool) => void; onClose: () => void }) {
  const groups = [...new Set(CANVAS_TOOLS.map((t) => t.group))];
  return (
    <div className="flex w-44 shrink-0 flex-col gap-1.5 overflow-y-auto border-r border-border p-2" aria-label="Tools to add">
      <div className="flex items-center gap-1">
        <span className="flex-1 text-[11px] font-semibold uppercase tracking-wide text-muted-foreground">Add a step</span>
        <IconButton label="Hide the tools" onClick={onClose}>
          <PanelLeftClose />
        </IconButton>
      </div>
      {groups.map((group) => (
        <div key={group} className="flex flex-col gap-0.5">
          <div className="text-muted-foreground">{group}</div>
          {CANVAS_TOOLS.filter((t) => t.group === group).map((tool) => (
            <button
              key={tool.key}
              type="button"
              draggable
              className="cursor-grab rounded-md border border-border bg-card px-2 py-1 text-left hover:border-primary active:cursor-grabbing"
              title={`${tool.command}, on ${tool.takes}: drag onto a step to add it after that step, or click to add it after the step open, or at the end`}
              aria-label={`Add ${group}: ${tool.label}`}
              onDragStart={(e) => {
                e.dataTransfer.setData(TOOL_DRAG, tool.key);
                e.dataTransfer.effectAllowed = 'copy';
              }}
              onClick={() => onPick(tool)}
            >
              {tool.label}
            </button>
          ))}
        </div>
      ))}
    </div>
  );
}

function StepNode({ data }: NodeProps<Node<StepData>>) {
  const { node, changed, focused } = data;
  const step = node.step!;
  const command = step.action.kind === 'command' ? step.action.command : 'Pause';
  return (
    <div
      className={`w-60 rounded-md border bg-card text-xs text-card-foreground shadow-sm ${focused ? 'border-primary ring-1 ring-primary' : 'border-border'} ${step.on === false ? 'opacity-50' : ''}`}
      title={step.on === false ? 'Switched off: passed over' : undefined}
    >
      <div className="relative flex items-center gap-1.5 border-b border-border/60 px-2 py-1.5">
        <span className="tabular-nums text-muted-foreground">{node.position}</span>
        <span className="min-w-0 flex-1 truncate font-medium">{command}</span>
        <Handle type="source" position={Position.Right} id="made" className="!size-2.5 !border-0 !bg-primary" title="What it makes: drag onto a later step's input" />
      </div>
      <div className="flex flex-col gap-0.5 py-1">
        {(node.inputs ?? []).map((input) => (
          <div key={input} className="relative pl-3 pr-2 text-muted-foreground">
            <Handle type="target" position={Position.Left} id={input} className="!size-2 !border-0 !bg-muted-foreground" />
            {input}
          </div>
        ))}
        {step.note && <div className="truncate px-2 italic text-muted-foreground">{step.note}</div>}
        <div className="flex gap-1 px-2 pt-0.5">
          {changed > 0 && <StatusBadge tone="edited">{changed} changed</StatusBadge>}
          <LastBadge step={step} />
        </div>
      </div>
    </div>
  );
}

function PauseNode({ data }: NodeProps<Node<StepData>>) {
  const { node, focused } = data;
  return (
    <div className={`w-52 rounded-md border border-dashed bg-card px-2 py-1.5 text-xs ${focused ? 'border-primary ring-1 ring-primary' : 'border-warning'}`}>
      <div className="flex items-center gap-1.5 text-warning">
        <span className="tabular-nums text-muted-foreground">{node.position}</span>
        <Pause className="size-3.5" /> Pause: hand work
      </div>
      <div className="truncate italic text-muted-foreground">{node.step?.note ?? 'Look, or work by hand, then Continue'}</div>
    </div>
  );
}

function InputNode({ data }: NodeProps<Node<InputData>>) {
  const { node } = data;
  return (
    <div className={`relative w-44 rounded-full border border-dashed bg-background px-3 py-1 text-xs ${node.kind === 'gone' ? 'border-destructive text-destructive' : 'border-muted-foreground'}`}>
      <span className="text-muted-foreground">{kindWords[node.kind]} </span>
      <span className="font-medium">{node.label}</span>
      <Handle type="source" position={Position.Right} id="made" className="!size-2 !border-0 !bg-muted-foreground" />
    </div>
  );
}

const nodeTypes = { step: StepNode, pause: PauseNode, input: InputNode };

export function WorkflowCanvas() {
  const { summary, setSelection, zoomTo, openPanel, openTool, setAddingTo } = useWorkspace();
  const [toolsOpen, setToolsOpen] = useState(true);
  const { workflows, chosen, workflow, revision, caseName, pick, pickCase, save } = usePickedWorkflow();
  const [focus, setFocus] = useState<number | null>(null);
  const [said, setSaid] = useState<{ error?: string; text?: string }>({});
  const cases = workflow?.cases ?? [];

  const graph = useMemo(() => (workflow ? workflowGraph({ ...workflow, steps: stepsOfCase(workflow, caseName) }) : null), [workflow, caseName]);
  const changes = useMemo(() => (workflow ? changesOfCase(workflow, caseName) : new Map<number, number>()), [workflow, caseName]);

  const nodes = useMemo<Node[]>(() => {
    if (!graph) return [];
    const tops = new Map<number, number>();
    return graph.nodes.map((node) => {
      const y = tops.get(node.column) ?? 0;
      tops.set(node.column, y + heightOf(node) + GAP);
      const position = { x: node.column * COLUMN, y };
      if (node.kind === 'step' || node.kind === 'pause') {
        const data: StepData = { node, changed: changes.get(node.step!.id) ?? 0, focused: focus === node.step!.id };
        return { id: node.id, type: node.kind, position, data, draggable: false };
      }
      return { id: node.id, type: 'input', position, data: { node }, draggable: false, connectable: node.kind !== 'picked' && node.kind !== 'gone' };
    });
  }, [graph, changes, focus]);

  const edges = useMemo<Edge[]>(
    () =>
      (graph?.edges ?? []).map((e) => {
        const near = focus !== null && (e.target === `step:${focus}` || e.source === `step:${focus}`);
        return {
          id: e.id,
          source: e.source,
          sourceHandle: 'made',
          target: e.target,
          targetHandle: e.input,
          animated: near,
          markerEnd: { type: MarkerType.ArrowClosed, width: 14, height: 14 },
          style: { strokeDasharray: e.outside ? '5 4' : undefined, stroke: near ? 'var(--primary)' : 'var(--muted-foreground)', strokeWidth: near ? 1.75 : 1.25 },
        };
      }),
    [graph, focus],
  );

  const focused: WorkflowStep | undefined = workflow ? stepsOfCase(workflow, caseName).find((s) => s.id === focus) : undefined;
  const focusAt = workflow && focused ? workflow.steps.findIndex((s) => s.id === focused.id) + 1 : 0;

  const setCell = async (row: SettingRow, text: string) => {
    if (!workflow) return;
    const next = withSetting(workflow, caseName, row, text);
    if (!next) return setSaid({ error: `'${text}' isn't a ${typeof row.base === 'number' ? 'number' : 'true or false'}, as ${row.field} has to be` });
    const error = await save(next);
    setSaid(error ? { error } : {});
  };

  const wire = async (source: string | null, target: string | null, input: string | null | undefined) => {
    if (!workflow || !source || !target || !input) return;
    const next = rewire(workflow, source, target, input);
    if (typeof next === 'string') return setSaid({ error: next });
    if (next === workflow) return setSaid({});
    const error = await save(next);
    const at = (node: string) => workflow.steps.findIndex((s) => `step:${s.id}` === node) + 1;
    const [kind, ...name] = source.split(':');
    const from = kind === 'step' ? `step ${at(source)}` : kind === 'layer' ? `layer '${name.join(':')}'` : `'${name.join(':')}'`;
    setSaid(error ? { error } : { text: `Step ${at(target)}'s ${input} now reads from ${from}` });
  };

  const show = (step: WorkflowStep) => {
    const made = (step.last?.made ?? []).filter((id) => summary?.objects.some((o) => o.id === id));
    if (made.length === 0) return;
    setSelection(made);
    zoomTo(made);
  };

  /** A tool dropped after a step (or at the end): its panel opened set to it, adding there, what that step made picked. */
  const addTool = (tool: CanvasTool, onStep: number | null) => {
    if (!workflow || !chosen) return;
    const at = placeOfDrop(workflow, onStep);
    const after = at > 0 ? stepsOfCase(workflow, caseName)[at - 1] : undefined;
    const picked = madeByStep(after, (id) => summary?.objects.some((o) => o.id === id) ?? false);
    if (picked.length > 0) setSelection(picked);
    setAddingTo({ id: chosen.id, name: chosen.name, at });
    openTool(tool.panel, tool.mode);
    setSaid({ text: dropWords(workflow, chosen.name, tool, at, picked.length) });
  };

  const dropped = (e: React.DragEvent) => {
    const tool = toolByKey(e.dataTransfer.getData(TOOL_DRAG));
    if (!tool) return;
    e.preventDefault();
    const id = (e.target as Element).closest?.('.react-flow__node')?.getAttribute('data-id') ?? '';
    addTool(tool, id.startsWith('step:') ? Number(id.slice(5)) : focus);
  };

  if (!chosen) return <div className="p-3 text-xs text-muted-foreground">No workflow yet: capture runs from the Run log, then the canvas draws it.</div>;

  return (
    <div className="flex h-full min-h-0 text-xs" data-tool-panel="canvas">
      {toolsOpen && <ToolList onPick={(tool) => addTool(tool, focus)} onClose={() => setToolsOpen(false)} />}
      <div className="flex min-w-0 flex-1 flex-col">
        <div className="flex shrink-0 flex-wrap items-center gap-x-3 gap-y-1 border-b border-border px-2 py-1">
          {!toolsOpen && (
            <IconButton label="Show the tools, to add a step" onClick={() => setToolsOpen(true)}>
              <PanelLeftOpen />
            </IconButton>
          )}
          <Field label="Workflow">
            <Select value={chosen.id} aria-label="Canvas workflow" onChange={(e) => pick(e.target.value)}>
              {workflows.map((o) => (
                <option key={o.id} value={o.id}>
                  {o.name}
                </option>
              ))}
            </Select>
          </Field>
          <Field label="Case">
            <Select value={caseName} aria-label="Canvas case" onChange={(e) => pickCase(e.target.value)}>
              {[BASE, ...cases.map((c) => c.name)].map((name) => (
                <option key={name} value={name}>
                  {name}
                </option>
              ))}
            </Select>
          </Field>
          <span className="min-w-0 flex-1 truncate text-muted-foreground">
          {said.error ? <span className="text-destructive">{said.error}</span> : said.text ? <span className="text-ok">{said.text}</span> : 'Drag a tool onto a step to add one after it; drag from a step’s right edge onto a later step’s input to wire it; click a step for its settings'}
          </span>
        </div>
        <div
          className="relative min-h-0 flex-1"
          onDragOver={(e) => {
            if (!e.dataTransfer.types.includes(TOOL_DRAG)) return;
            e.preventDefault();
            e.dataTransfer.dropEffect = 'copy';
          }}
          onDrop={dropped}
        >
        <ReactFlow
          key={chosen.id}
          nodes={nodes}
          edges={edges}
          nodeTypes={nodeTypes}
          colorMode="dark"
          fitView
          minZoom={0.2}
          deleteKeyCode={null}
          nodesDraggable={false}
          edgesFocusable={false}
          onNodeClick={(_, node) => {
            const step = (node.data as Partial<StepData>).node?.step;
            setFocus(step ? (step.id === focus ? null : step.id) : null);
          }}
          onPaneClick={() => setFocus(null)}
          onConnect={(c) => void wire(c.source, c.target, c.targetHandle)}
          proOptions={{ hideAttribution: true }}
          style={{ background: 'var(--background)' }}
        >
          <Background gap={24} size={1} color="var(--border)" />
          <Controls showInteractive={false} />
        </ReactFlow>
        </div>
      </div>
      {focused && workflow && (
        <aside className="flex w-72 shrink-0 flex-col gap-1.5 overflow-y-auto border-l border-border p-2" aria-label="Step on the canvas">
          <div className="flex items-center gap-1">
            <span className="min-w-0 flex-1 truncate font-medium">
              {focusAt}. {focused.action.kind === 'command' ? focused.action.command : 'Pause'}
            </span>
            <IconButton label="Select and zoom to what it last made" disabled={!focused.last?.made.length} onClick={() => show(focused)}>
              <Scan />
            </IconButton>
            <IconButton label="The steps in order, in the Workflow panel" onClick={() => openPanel('workflow')}>
              <ListOrdered />
            </IconButton>
          </div>
          <div className="flex gap-1">
            {changes.get(focused.id) ? <StatusBadge tone="edited">{caseName} changes {plural(changes.get(focused.id)!, 'setting')}</StatusBadge> : null}
            <LastBadge step={focused} />
          </div>
          {focused.note && <div className="italic text-muted-foreground">{focused.note}</div>}
          {focused.action.kind === 'command' ? (
            <>
              <div className="text-muted-foreground">
                {Object.entries(focused.action.inputs).map(([input, rule]) => (
                  <div key={input}>
                    {input}: {ruleWords(workflow, rule)}
                  </div>
                ))}
              </div>
              {settingsOfStep(workflow, focused.id).length > 0 ? (
                <StepSettings
                  rows={settingsOfStep(workflow, focused.id)}
                  hidden={Object.keys(focused.action.settings ?? {}).length - settingsOfStep(workflow, focused.id).length}
                  caseName={caseName}
                  revision={revision}
                  valueOf={(row) => caseValue(workflow, caseName, row)}
                  set={(row, text) => void setCell(row, text)}
                />
              ) : (
                <div className="text-muted-foreground">No plain settings: set in its tool.</div>
              )}
            </>
          ) : (
            <div className="text-muted-foreground">A run stops here for a look or hand work; Continue in the Workflow panel runs on.</div>
          )}
        </aside>
      )}
    </div>
  );
}
