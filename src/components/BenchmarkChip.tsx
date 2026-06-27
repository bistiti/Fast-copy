import { runBenchmark } from "../api";
import { useStore } from "../store";

export function BenchmarkChip() {
  const benchmark = useStore((s) => s.benchmark);
  const phase = useStore((s) => s.phase);

  const label = (() => {
    switch (benchmark.state) {
      case "running":
        return "Benchmarking…";
      case "completed":
        return `Tuned · ${benchmark.thresholdMib} MiB · ${benchmark.threads} threads`;
      case "failed":
        return `Benchmark failed`;
      default:
        return "Not benchmarked";
    }
  })();

  const canRun = phase === "idle" && benchmark.state !== "running";

  return (
    <div className="bench">
      <span className={`chip chip-${benchmark.state}`}>
        <span className="dot" />
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
