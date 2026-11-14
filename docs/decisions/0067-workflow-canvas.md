# 0067 — The workflow canvas: the same workflow drawn as nodes and wires

Status: agreed 14 November 2026 (Tim: "Yep I like it - let's do it", on the plan with the canvas as stage 2b, before the
report; React Flow agreed as a new dependency: "If react flow is the best option happy to use it"). Stage 2b of the Build
Plan's "Workflow plan" tab, on 0065 and 0066.

## Why

A workflow is a graph underneath: each step takes its inputs by rule, and "the solids step 15 made" is a wire from step
15. The list hides that. Seeing which steps feed the cut into blocks, what a change to the toe flows through, and where
the hand work sits matters more as workflows grow and manual steps join them. The reserves dataflow (stage 5) is a node
graph too, Spry-style, and wants the same canvas.

## Decision

1. **A second view of the same workflow, not another workflow.** Nothing changes in the engine or the saved object:
   steps still run in their order (engine rule 4), one undo a run. The list in the Workflow panel stays for order, notes
   and adding steps; the canvas is for seeing the shape, wiring inputs and opening a step's settings.
2. **React Flow** (`@xyflow/react`, MIT) draws and edits the graph. It's only the drawing: what a node does is the
   engine's. Added to the locked stack (Interface row).
3. **Nodes and wires.** A node a step: its number, command, note, last result and the case's changes, greyed when
   switched off. A pause is its own kind of node, the hand work. What steps take from outside the workflow (a layer, a
   named object, objects as picked) is an input node at the left, shared, its wires dashed. A wire a step input, from the
   step or input node it reads, labelled with the input's name.
4. **Laid out by dependency, the pauses as barriers.** A step stands one column right of the furthest step it reads
   from; a pause stands right of every step before it, and every step after it right of the pause, so hand work reads
   left to right in the order it happens. Within a column, steps go down in their order. Positions aren't saved: the
   layout is worked out each time, so it never goes stale.
5. **A click on a node** opens the step beside the canvas: its inputs in words, its settings as the case picked has them
   (0066), its note, Show (select and zoom to what it last made) and a way back to the list. Running stays in the
   Workflow panel, with its Continue and tidy. The workflow and case picked are shared with the Workflow panel.
6. **Drawing a wire changes an input.** From a step or an input node onto a step's input, it sets that input's rule to
   read from it, keeping the kind, the filter and whether it takes many. A wire from a step after it is refused, as the
   engine refuses a rule reading a later step; a wire can't be deleted, since every input needs a rule.

## Not here

Dragging tools in from a list to add steps (next, as its own PR); groups that fold away; positions saved by hand;
branching (if/then nodes), which would be an engine change; the dataflow's table nodes (stage 5).

## Done when

The demo workflow opens on the canvas with a node a step, an input node a shared input and a wire an input, the pause
standing between the steps before and after it; a click on a node shows its settings for the case picked; and a wire
drawn from an earlier step changes that input's rule, saved as the workflow's next revision, a wire from a later step
refused. Known-answer tests for the layout and the rewiring.

## Built

`app/src/panels/workflowGraph.ts` (the layout and `rewire`, with known-answer tests in `app/tests/workflowGraph.test.mjs`),
`WorkflowCanvas.tsx` (the dock tab "Workflow canvas", opened from the Workflow panel's Canvas button, below the viewport),
and `workflowParts.tsx` (what the panel and the canvas share: the workflow and case picked, a step's settings, its badge).
Checked by hand on the demo workflow (19 steps, 7 input nodes, the pause between steps 13 and 15) in a page outside the
app with the bridge stood in; `zd-app --bench --workflow` doesn't drive the canvas yet.
