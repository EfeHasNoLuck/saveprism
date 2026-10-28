// @vitest-environment node
import { describe, expect, it } from "vitest";
import { nextProgress, progressPercent, runUpdateCheck, startInstall } from "./update";

const found = { version: "0.2.0" };

describe("runUpdateCheck", () => {
  it("offers a newer version, quiet or not", async () => {
    for (const quiet of [true, false]) {
      const outcome = await runUpdateCheck(() => Promise.resolve(found), quiet);
      expect(outcome.check).toEqual({ status: "available", version: "0.2.0" });
      expect(outcome.update).toBe(found);
    }
  });

  it("says up to date only when the player asked", async () => {
    expect((await runUpdateCheck(() => Promise.resolve(null), false)).check).toEqual({
      status: "current",
    });
    expect((await runUpdateCheck(() => Promise.resolve(null), true)).check).toEqual({
      status: "idle",
    });
  });

  it("shows a failure only when the player asked", async () => {
    const offline = () => Promise.reject(new Error("could not fetch a valid release JSON"));
    expect(await runUpdateCheck(offline, false)).toEqual({ check: { status: "failed" }, update: null });
    expect(await runUpdateCheck(offline, true)).toEqual({ check: { status: "idle" }, update: null });
  });

  it("treats a check that throws at once like one that rejects", async () => {
    const broken = (): Promise<null> => {
      throw new Error("not inside the desktop app");
    };
    expect((await runUpdateCheck(broken, true)).check).toEqual({ status: "idle" });
    expect((await runUpdateCheck(broken, false)).check).toEqual({ status: "failed" });
  });
});

describe("install progress", () => {
  it("counts the downloaded bytes against the announced size", () => {
    let progress = startInstall("0.2.0");
    progress = nextProgress(progress, { event: "Started", data: { contentLength: 1000 } });
    progress = nextProgress(progress, { event: "Progress", data: { chunkLength: 250 } });
    progress = nextProgress(progress, { event: "Progress", data: { chunkLength: 250 } });
    expect(progress).toEqual({ version: "0.2.0", phase: "download", downloaded: 500, total: 1000 });
    expect(progressPercent(progress)).toBe(50);
  });

  it("has no percent when the size is unknown", () => {
    let progress = nextProgress(startInstall("0.2.0"), { event: "Started", data: {} });
    progress = nextProgress(progress, { event: "Progress", data: { chunkLength: 4096 } });
    expect(progress.total).toBeNull();
    expect(progressPercent(progress)).toBeNull();
    expect(nextProgress(startInstall("0.2.0"), { event: "Started", data: { contentLength: 0 } }).total).toBeNull();
  });

  it("never goes past 100 percent", () => {
    let progress = nextProgress(startInstall("0.2.0"), { event: "Started", data: { contentLength: 10 } });
    progress = nextProgress(progress, { event: "Progress", data: { chunkLength: 25 } });
    expect(progressPercent(progress)).toBe(100);
  });

  it("moves to installing when the download finished", () => {
    const progress = nextProgress(startInstall("0.2.0"), { event: "Finished" });
    expect(progress.phase).toBe("install");
  });
});
