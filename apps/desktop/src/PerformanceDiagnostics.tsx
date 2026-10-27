import { useEffect, useState } from "react";
// Measures real renderer stalls; never reads project contents or sends telemetry.
export default function PerformanceDiagnostics() {
  const [metrics, setMetrics] = useState({ count: 0, max: 0, total: 0 });
  useEffect(() => {
    if (
      !globalThis.PerformanceObserver ||
      !PerformanceObserver.supportedEntryTypes.includes("longtask")
    )
      return;
    let count = 0,
      max = 0,
      total = 0;
    const observer = new PerformanceObserver((list) => {
      for (const e of list.getEntries()) {
        count++;
        max = Math.max(max, e.duration);
        total += e.duration;
      }
      setMetrics({ count, max, total });
    });
    observer.observe({ type: "longtask", buffered: true });
    return () => observer.disconnect();
  }, []);
  return (
    <details className="performance-diagnostics">
      <summary>Interface performance</summary>
      <dl>
        <dt>Tasks over 50 ms</dt>
        <dd>{metrics.count}</dd>
        <dt>Longest task</dt>
        <dd>{metrics.max.toFixed(1)} ms</dd>
        <dt>Total long-task time</dt>
        <dd>{metrics.total.toFixed(1)} ms</dd>
      </dl>
    </details>
  );
}
