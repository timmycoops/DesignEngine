import { useEffect, useRef } from 'react';
import { useWorkspace } from '@/workspace/context';

/** Mounts the app's single viewport canvas; the Viewport itself lives as long as the app. */
export function ViewportPanel() {
  const { viewportHost, viewportMounted } = useWorkspace();
  const ref = useRef<HTMLDivElement>(null);
  useEffect(() => {
    ref.current!.appendChild(viewportHost);
    viewportMounted();
    return () => viewportHost.remove();
  }, [viewportHost, viewportMounted]);
  return <div ref={ref} className="relative h-full w-full overflow-hidden bg-[#161e29]" />;
}
