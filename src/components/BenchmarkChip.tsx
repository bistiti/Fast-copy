import { runBenchmark } from "../api";
import { useStore } from "../store";
import { Spinner } from "./Spinner";

export function BenchmarkChip() {
  const benchmark = useStore((s) => s.benchmark);
  const phase = useStore((s) => s.phase);
  const scanning = useStore((s) => s.scanning);

  const running = benchmark.state === "running";

  const label = (() => {
    switch (benchmark.state) {
      case "running":
        return "Benchmarking…";
      case "completed":
        return `Tuned · ${benchmark.thresholdMib} MiB · ${benchmark.threads} threads`;
      case "failed":
        return "Benchmark failed";
      case "cancelled":
        return "Benchmark cancelled";
      default:
        return "Not benchmarked";
    }
  })();

  const canRun = phase === "idle" && !running && !scanning;

  return (
    <div className="bench">
      <span className={`chip chip-${benchmark.state}`}>
        {running ? <Spinner size={11} /> : <span className="dot" />}
        {label}
      </span>
      <button
        className="btn small ghost"
        disabled={!canRun}
        onClick={() => void runBenchmark()}
        title={benchmark.message ?? "Calibrate the buffered/unbuffered threshold"}
      >
        Run benchmark
      </button>
    </div>
  );
}
