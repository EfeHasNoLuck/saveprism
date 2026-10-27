/** The steps of the first-launch tour. Pure, so the missing-element rule can be tested. */

export interface TourStep {
  id: string;
  /** CSS selector of the real element the step spotlights. */
  selector: string;
  titleKey: string;
  bodyKey: string;
}

export const TOUR_STEPS: readonly TourStep[] = [
  {
    id: "drop",
    selector: '[data-tour="drop"]',
    titleKey: "tour.drop.title",
    bodyKey: "tour.drop.body",
  },
  {
    id: "where",
    selector: '[data-tour="where"]',
    titleKey: "tour.where.title",
    bodyKey: "tour.where.body",
  },
  {
    id: "language",
    selector: '[data-tour="language"]',
    titleKey: "tour.language.title",
    bodyKey: "tour.language.body",
  },
  {
    id: "bell",
    selector: '[data-tour="bell"]',
    titleKey: "tour.bell.title",
    bodyKey: "tour.bell.body",
  },
  {
    id: "settings",
    selector: '[data-tour="settings"]',
    titleKey: "tour.settings.title",
    bodyKey: "tour.settings.body",
  },
] as const;

/** The steps whose element is on screen; a step whose element is missing is skipped. */
export function availableTourSteps(
  steps: readonly TourStep[],
  hasElement: (selector: string) => boolean,
): TourStep[] {
  return steps.filter((step) => hasElement(step.selector));
}
