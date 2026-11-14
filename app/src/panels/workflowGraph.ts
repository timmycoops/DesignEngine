/**
 * A workflow as a graph (decision 0067): a node a step, an input node for each thing steps take from outside the
 * workflow, and a wire a step input. Laid out by dependency, the pauses as barriers, worked out each time it's drawn.
 * Drawing a wire changes an input's rule. Plain data here; the canvas draws it.
 */
import type { InputRule, Workflow, WorkflowStep } from '../bridge';

/** What a node stands for. */
export type GraphNodeKind = 'step' | 'pause' | 'layer' | 'named' | 'picked' | 'gone';

export interface GraphNode {
  /** `step:<id>`, `layer:<name>`, `named:<name>`, `picked:<step>:<input>` or `gone:<step>`. */
  id: string;
  kind: GraphNodeKind;
  /** Column from the left (0 holds the input nodes) and row from the top within it. */
  column: number;
  row: number;
  /** The step, for step and pause nodes. */
  step?: WorkflowStep;
  /** Where the step is, from 1. */
  position?: number;
  /** An input node's words: the layer's or the object's name, or how many objects were picked. */
  label?: string;
  /** For a step node, the inputs it takes, in order: its wires' ends. */
  inputs?: string[];
}

export interface GraphEdge {
  id: string;
  source: string;
  target: string;
  /** The input of the target step the wire is. */
  input: string;
  /** From an input node, not a step: drawn dashed. */
  outside: boolean;
  /** The kind and filter the rule keeps, in words, if any. */
  detail?: string;
}

export interface Graph {
  nodes: GraphNode[];
  edges: GraphEdge[];
  columns: number;
}

const stepNode = (id: number) => `step:${id}`;

/** The node an input reads from, and its words. */
function sourceOf(rule: InputRule, step: number, input: string, known: Set<number>): { id: string; kind: GraphNodeKind; label: string } {
  const from = rule.from;
  if ('step' in from) return known.has(from.step) ? { id: stepNode(from.step), kind: 'step', label: '' } : { id: `gone:${step}:${input}`, kind: 'gone', label: 'a step no longer there' };
  if ('layer' in from) return { id: `layer:${from.layer}`, kind: 'layer', label: from.layer };
  if ('named' in from) return { id: `named:${from.named}`, kind: 'named', label: from.named };
  return { id: `picked:${step}:${input}`, kind: 'picked', label: `${from.picked.length} picked` };
}

/** A rule's kind and filter in words: `solids where material is coal`. */
function detailOf(rule: InputRule): string | undefined {
  const parts: string[] = [];
  if (rule.kind) parts.push(`${rule.kind.replace('_', ' ')}${rule.many ? 's' : ''}`);
  for (const [key, value] of Object.entries(rule.where ?? {})) parts.push(`${key} is ${String(value)}`);
  return parts.length ? parts.join(' where ') : undefined;
}

/**
 * The workflow as a graph. A step stands one column right of the furthest step it reads from; a pause right of every
 * step before it, and every step after a pause right of it; within a column, steps go down in their order. Input nodes
 * stand in column 0, in the order they're first read.
 */
export function workflowGraph(workflow: Workflow): Graph {
  const known = new Set(workflow.steps.map((s) => s.id));
  const columnOf = new Map<number, number>();
  const nodes: GraphNode[] = [];
  const edges: GraphEdge[] = [];
  const inputs = new Map<string, GraphNode>();
  const rows = new Map<number, number>();
  const place = (column: number) => {
    const row = rows.get(column) ?? 0;
    rows.set(column, row + 1);
    return row;
  };
  let barrier = 1;
  let furthest = 0;
  workflow.steps.forEach((step, at) => {
    let column = barrier;
    if (step.action.kind === 'pause') {
      column = Math.max(barrier, furthest + 1);
      barrier = column + 1;
    } else {
      for (const [input, rule] of Object.entries(step.action.inputs)) {
        const source = sourceOf(rule, step.id, input, known);
        if (source.kind === 'step') {
          const from = columnOf.get((rule.from as { step: number }).step);
          if (from !== undefined) column = Math.max(column, from + 1);
        } else if (!inputs.has(source.id)) {
          inputs.set(source.id, { id: source.id, kind: source.kind, column: 0, row: inputs.size, label: source.label });
        }
        edges.push({ id: `${source.id}->${stepNode(step.id)}:${input}`, source: source.id, target: stepNode(step.id), input, outside: source.kind !== 'step', detail: detailOf(rule) });
      }
    }
    columnOf.set(step.id, column);
    furthest = Math.max(furthest, column);
    nodes.push({
      id: stepNode(step.id),
      kind: step.action.kind === 'pause' ? 'pause' : 'step',
      column,
      row: place(column),
      step,
      position: at + 1,
      inputs: step.action.kind === 'command' ? Object.keys(step.action.inputs) : [],
    });
  });
  return { nodes: [...inputs.values(), ...nodes], edges, columns: furthest + 1 };
}

/** What a wire's start reads as in a rule: a step, a layer or a named object. Null for a node a rule can't name. */
function ruleSource(node: string): InputRule['from'] | null {
  const [kind, ...rest] = node.split(':');
  const name = rest.join(':');
  if (kind === 'step' && /^\d+$/.test(name)) return { step: Number(name) };
  if (kind === 'layer') return { layer: name };
  if (kind === 'named') return { named: name };
  return null;
}

/**
 * The workflow with a step's input read from another node, as a wire drawn onto it does: the rule keeps its kind, filter
 * and whether it takes many. A reason in words where it can't be: a step reads only what a step before it made, as the
 * engine says; objects as picked can't be wired, since they're kept only as captured.
 */
export function rewire(workflow: Workflow, source: string, target: string, input: string): Workflow | string {
  const from = ruleSource(source);
  if (!from) return 'Objects as picked are kept as captured: wire from a step, a layer or a named object';
  const to = workflow.steps.findIndex((s) => stepNode(s.id) === target);
  const step = workflow.steps[to];
  if (!step || step.action.kind !== 'command' || !(input in step.action.inputs)) return 'Wires go onto a step’s input';
  if ('step' in from) {
    const at = workflow.steps.findIndex((s) => s.id === from.step);
    if (at < 0) return 'That step is no longer there';
    if (workflow.steps[at].action.kind !== 'command') return 'A pause makes nothing to read';
    if (at >= to) return `Step ${at + 1} comes after step ${to + 1}: a step reads only what a step before it made`;
  }
  const rule = step.action.inputs[input];
  if (JSON.stringify(rule.from) === JSON.stringify(from)) return workflow;
  const steps = workflow.steps.map((s, k) => (k === to && s.action.kind === 'command' ? { ...s, action: { ...s.action, inputs: { ...s.action.inputs, [input]: { ...rule, from } } } } : s));
  return { ...workflow, steps };
}
