import { beforeEach, describe, expect, it } from "vitest";
import { useStore } from "./store";
import type { CopyBatchPayload, QueueEntryDto, ThroughputPayload } from "./types";

const entries: QueueEntryDto[] = [
  { index: 0, name: "a.bin", size: 100, mode: "buffered" },
  { index: 1, name: "b.bin", size: 200, mode: "unbuffered" },
  { index: 2, name: "c.bin", size: 300, mode: "buffered" },
];

function throughput(over: Partial<ThroughputPayload> = {}): ThroughputPayload {
  return {
    speed: 1000,
    totalCopied: 0,
    totalBytes: 600,
    eta: 5,
    elapsedSecs: 1,
    filesDone: 0,
    filesFailed: 0,
    filesSkipped: 0,
    foldersDone: 0,
    foldersTotal: 1,
    currentIndex: null,
    ...over,
  };
}

function batch(
  rows: CopyBatchPayload["rows"],
  over: Partial<ThroughputPayload> = {},
): CopyBatchPayload {
  return { rows, throughput: throughput(over) };
}

describe("store.applyBatch", () => {
  beforeEach(() => {
    useStore.getState().clearCopy();
    useStore.getState().beginCopy(entries);
  });

  it("applies row deltas and updates the throughput aggregate in one pass", () => {
    useStore.getState().applyBatch(
      batch(
        [
          { index: 1, status: "done", bytesCopied: 200, error: null },
          { index: 0, status: "inProgress", bytesCopied: 50, error: null },
        ],
        { filesDone: 1, totalCopied: 250 },
      ),
    );

    const { queue, progress, throughputHistory } = useStore.getState();
    expect(queue[0].status).toBe("inProgress");
    expect(queue[0].bytesCopied).toBe(50);
    expect(queue[1].status).toBe("done");
    expect(queue[1].bytesCopied).toBe(200);
    expect(queue[2].status).toBe("pending"); // untouched
    expect(progress?.filesDone).toBe(1);
    expect(throughputHistory).toEqual([1000]);
  });

  it("does not rebuild untouched rows (only changed rows get new objects)", () => {
    const before = useStore.getState().queue;
    const row0 = before[0];
    const row2 = before[2];

    useStore
      .getState()
      .applyBatch(batch([{ index: 1, status: "done", bytesCopied: 200, error: null }]));

    const after = useStore.getState().queue;
    expect(after).not.toBe(before); // new array (one row changed)
    expect(after[0]).toBe(row0); // unchanged rows keep identity
    expect(after[2]).toBe(row2);
    expect(after[1]).not.toBe(before[1]); // changed row is a fresh object
  });

  it("keeps the same queue reference when a batch carries no rows", () => {
    const before = useStore.getState().queue;
    useStore.getState().applyBatch(batch([], { speed: 2000 }));
    const { queue, progress } = useStore.getState();
    expect(queue).toBe(before); // no array churn on a throughput-only tick
    expect(progress?.speed).toBe(2000);
  });

  it("carries an error string through for failed rows", () => {
    useStore.getState().applyBatch(
      batch([{ index: 2, status: "failed", bytesCopied: 0, error: "disk full" }]),
    );
    const row = useStore.getState().queue[2];
    expect(row.status).toBe("failed");
    expect(row.error).toBe("disk full");
  });
});
