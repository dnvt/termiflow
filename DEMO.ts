// TermiFlow Demo Timeline
// Concise, day-by-day walkthrough of what shipped. Source of truth for demos.

export type DemoStep = {
  day: number;
  focus: string;
  highlights: string[];
};

export const demoTimeline: DemoStep[] = [
  {
    day: 1,
    focus: "CLI + parser foundation",
    highlights: [
      "Two-pass Mermaid parser with strict/lenient modes",
      "Deterministic node ordering and warning surface for malformed lines",
      "CLI flags (--print, --style, --max-label, --strict) wired with config precedence",
    ],
  },
  {
    day: 2,
    focus: "Layout and rendering baseline",
    highlights: [
      "Waterfall layout with cycle detection/back-edge marking",
      "Edge routing with gutter for cycles and clipping warnings",
      "Label truncation and multi-style rendering for boxes/edges",
    ],
  },
  {
    day: 3,
    focus: "Docs and polish",
    highlights: [
      "Consumer-facing docs tightened; planning artifacts moved to planning/",
      "Demo timeline captured here as single source of truth",
      "All tests green via cargo test",
    ],
  },
];

export default demoTimeline;
