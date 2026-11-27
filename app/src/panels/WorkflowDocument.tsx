import { WorkflowCanvas } from './WorkflowCanvas';
import { WorkflowPanel } from './WorkflowPanel';

/**
 * A workflow as one document over the view (decision 0108, decision 7): its steps drawn as nodes and wires, and beside
 * them the workflow picked, its cases, its steps in order, its report and its run. What were the Workflow panel and the
 * Workflow canvas, each with its own picker, are one place.
 */
export function WorkflowDocument() {
  return (
    <div className="flex h-full min-h-0" data-slot="workflow-document">
      <div className="min-w-0 flex-1 border-r">
        <WorkflowCanvas />
      </div>
      <div className="h-full w-[400px] shrink-0 overflow-hidden">
        <WorkflowPanel />
      </div>
    </div>
  );
}
