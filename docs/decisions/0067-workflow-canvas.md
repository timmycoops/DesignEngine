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

7. **Tools dragged in add steps** (added 14 November 2026, Tim: "Yes, start it"). A list at the canvas's left holds
   every tool whose panel can add a step (0065): Builder's Build a step; Cut solids' Between surfaces, Into strips, By a
   surface, By seams and By polygons; Reserves' Interrogate. A tool dragged onto a step opens its panel set to it, its
   button reading Add to the workflow and the step going in after the step it was dropped on; what that step last made
   is picked, so a panel taking what's picked starts from it, and the engine finds it again as "what step N made", as
   Add to workflow always has. Dropped on the empty canvas, or clicked, it goes after the step open beside the canvas,
   or at the end. Each step added after it goes after the last, so steps added one after another keep their order. The
   panel sets the step up: a tool with nothing set has no step to add, and every input needs a rule. Tools whose panels
   can't add a step yet (offsets, face design, strip numbers, block lines) are captured from the Run log; each joins the
   list as its panel gains Add to workflow.

## Not here

Steps from tools whose panels can't add one yet; groups that fold away; positions saved by hand;
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
app with the bridge stood in.

The tools (7): `app/src/panels/workflowTools.ts` (the list, where a dropped step goes, what's picked, with known-answer
tests in `app/tests/workflowTools.test.mjs`); the canvas's tool list and drop; `openTool` in the workspace, which opens a
panel and asks it for one of what it does; `add_workflow_step`'s place (`at`) passed from the app. Checked by hand in the
same page (a drop on step 15 of the demo adds as step 16 with step 15's 36 pieces picked; a mouse drag onto the empty
canvas adds at the end), and `zd-app --bench --workflow` now clicks step 1 and By a surface on the canvas, adds the step
from the Cut solids panel and checks it went in as step 2, its solids found as what step 1 makes.
