// @vitest-environment node
import { describe, expect, it } from "vitest";
import { availableTourSteps, TOUR_STEPS, type TourStep } from "./tour";

const STEPS: TourStep[] = [
  { id: "a", selector: "[data-a]", titleKey: "a", bodyKey: "a" },
  { id: "b", selector: "[data-b]", titleKey: "b", bodyKey: "b" },
  { id: "c", selector: "[data-c]", titleKey: "c", bodyKey: "c" },
];

describe("availableTourSteps", () => {
  it("keeps the order and skips a step whose element is missing", () => {
    const found = new Set(["[data-a]", "[data-c]"]);
    expect(availableTourSteps(STEPS, (selector) => found.has(selector)).map((step) => step.id)).toEqual([
      "a",
      "c",
    ]);
  });

  it("gives no steps when nothing is on screen", () => {
    expect(availableTourSteps(STEPS, () => false)).toEqual([]);
  });

  it("lists the five real start-screen stops", () => {
    expect(TOUR_STEPS.map((step) => step.id)).toEqual([
      "drop",
      "where",
      "language",
      "bell",
      "settings",
    ]);
  });
});
