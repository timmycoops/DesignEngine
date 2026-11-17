import { Background, type Edge, Handle, MarkerType, type Node, type NodeProps, Position, ReactFlow } from '@xyflow/react';
import '@xyflow/react/dist/style.css';
import { useMemo } from 'react';
import type { ColumnSteps } from '@/bridge';
import { cn } from 'cn';
import { stepLabel } from './columnSteps';
import { inputsOf, stripSteps, tablesRead, volumeShort, writeTip } from './dataflowView';

/**
 * The Dataflow view's Show as graph (decision 0084, point 6): the strip drawn as nodes and wires on React Flow. The inputs
 * across the top, the blocks into the column as input and each parameter table wired, dashed, to the steps that read it
 * for this block; the steps below in run order, wrapping a row every few; what each wrote under it. Laid out by the run
 * order, never dragged; zoom and pan only. A step clicked is picked.
 */

const PER_ROW = 6;
const STEP_W = 150;
const GAP_X = 46;
const WRITE_H = 24;

interface StepData extends Record<string, unknown> {
  label: string;
  formula: boolean;
  quiet: boolean;
  picked: boolean;
}
interface TagData extends Record<string, unknown> {
  label: string;
  title: string;
  lit: boolean;
}

function StepNode({ data }: NodeProps<Node<StepData>>) {
  return (
    <div
      className={cn(
        'rounded-md border bg-card px-2 py-1.5 text-center text-xs',
        data.picked ? 'border-primary bg-selection text-primary ring-1 ring-primary' : data.formula ? 'border-dashed border-muted-foreground' : 'border-border',
        data.quiet && !data.picked && 'text-muted-foreground',
      )}
      style={{ width: STEP_W }}
    >
      <Handle type="target" position={Position.Left} id="in" className="!size-1.5 !border-0 !bg-muted-foreground" />
      <Handle type="target" position={Position.Top} id="read" className="!size-1.5 !border-0 !bg-transparent" />
      {data.formula && <span className="text-muted-foreground">ƒ </span>}
      {data.label}
      <Handle type="source" position={Position.Right} id="out" className="!size-1.5 !border-0 !bg-muted-foreground" />
      <Handle type="source" position={Position.Bottom} id="wrote" className="!size-1.5 !border-0 !bg-transparent" />
    </div>
  );
}

function WriteNode({ data }: NodeProps<Node<TagData>>) {
  return (
    <div className="rounded border border-border bg-muted px-1.5 text-[11px] text-muted-foreground" title={data.title}>
      <Handle type="target" position={Position.Top} className="!size-1 !border-0 !bg-transparent" />
      {data.label}
    </div>
  );
}

function InputNode({ data }: NodeProps<Node<TagData>>) {
  return (
    <div className={cn('rounded-md border px-1.5 py-0.5 text-[11px]', data.lit ? 'border-dashed border-primary bg-selection text-primary' : 'border-border bg-background text-muted-foreground')} title={data.title}>
      {data.label}
      <Handle type="source" position={Position.Bottom} className="!size-1 !border-0 !bg-transparent" />
    </div>
  );
}

const nodeTypes = { step: StepNode, write: WriteNode, input: InputNode };

export function DataflowGraph({ column, quiet, picked, unit, onPick }: { column: ColumnSteps; quiet: boolean[]; picked: number | null; unit: number | null; onPick: (at: number) => void }) {
  const { nodes, edges } = useMemo(() => {
    const steps = stripSteps(column.steps, quiet);
    const inputs = inputsOf(column, picked, unit);
    const nodes: Node[] = [];
    const edges: Edge[] = [];
    const lit = 'var(--primary)';
    const plain = 'var(--muted-foreground)';
    // The inputs across the top.
    inputs.forEach((c, i) => nodes.push({ id: `in:${c.key}`, type: 'input', position: { x: i * (STEP_W + GAP_X) * 0.85, y: 0 }, data: { label: c.label, title: c.title, lit: c.lit }, draggable: false, selectable: false }));
    // The steps in rows, each row as tall as its most writes.
    let y = 90;
    for (let row = 0; row * PER_ROW < steps.length; row++) {
      const these = steps.slice(row * PER_ROW, (row + 1) * PER_ROW);
      const most = Math.max(0, ...these.map((s) => s.writes.length));
      these.forEach((s, j) => {
        const x = j * (STEP_W + GAP_X);
        nodes.push({ id: `step:${s.at}`, type: 'step', position: { x, y }, data: { label: s.at === 0 ? s.name : stepLabel(s.name), formula: s.kind === 'formula', quiet: s.quiet, picked: picked === s.at }, draggable: false });
        s.writes.forEach((w, k) => {
          const id = `wrote:${s.at}:${w.folder}`;
          nodes.push({ id, type: 'write', position: { x: x + 12, y: y + 52 + k * WRITE_H }, data: { label: `${w.folder} ${volumeShort(w.volume)}`, title: writeTip(w), lit: false }, draggable: false, selectable: false });
          edges.push({ id: `e:${id}`, source: `step:${s.at}`, sourceHandle: 'wrote', target: id, style: { stroke: plain, strokeWidth: 1 } });
        });
        if (s.at + 1 < steps.length) {
          edges.push({
            id: `flow:${s.at}`,
            source: `step:${s.at}`,
            sourceHandle: 'out',
            target: `step:${s.at + 1}`,
            targetHandle: 'in',
            type: 'smoothstep',
            label: String(s.units),
            labelStyle: { fill: plain, fontSize: 10 },
            labelBgStyle: { fill: 'var(--background)' },
            markerEnd: { type: MarkerType.ArrowClosed, width: 12, height: 12 },
            style: { stroke: plain, strokeWidth: 1.25 },
          });
        }
      });
      y += 90 + most * WRITE_H;
    }
    // The blocks into the column as input; each table, dashed, to the steps that read it.
    edges.push({ id: 'read:blocks', source: 'in:blocks', target: 'step:0', targetHandle: 'read', style: { stroke: picked === 0 ? lit : plain, strokeDasharray: '4 3', strokeWidth: 1 } });
    column.steps.forEach((s, at) => {
      for (const t of tablesRead(s)) {
        edges.push({ id: `read:${t}:${at}`, source: `in:table:${t}`, target: `step:${at}`, targetHandle: 'read', style: { stroke: picked === at ? lit : plain, strokeDasharray: '4 3', strokeWidth: picked === at ? 1.5 : 1, opacity: picked === null || picked === at ? 1 : 0.45 } });
      }
    });
    return { nodes, edges };
  }, [column, quiet, picked, unit]);

  return (
    <div className="h-[45vh] min-h-64 w-full rounded-md border border-border" data-slot="dataflow-graph">
      <ReactFlow
        nodes={nodes}
        edges={edges}
        nodeTypes={nodeTypes}
        colorMode="dark"
        fitView
        minZoom={0.2}
        deleteKeyCode={null}
        nodesDraggable={false}
        nodesConnectable={false}
        edgesFocusable={false}
        onNodeClick={(_, node) => {
          if (node.id.startsWith('step:')) onPick(Number(node.id.slice(5)));
        }}
        proOptions={{ hideAttribution: true }}
        style={{ background: 'var(--background)' }}
      >
        <Background gap={24} size={1} color="var(--border)" />
      </ReactFlow>
    </div>
  );
}
