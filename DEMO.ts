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
  {
    day: 4,
    focus: "Composite styling system",
    highlights: [
      "9 base styles: ascii, unicode, double, rounded, heavy, dots, plus, stars, blocks",
      "Composite syntax: corner:dots,border:heavy for mix-and-match",
      "6 style components: corner, border, arrow, edge, junction, back",
    ],
  },
  {
    day: 5,
    focus: "Expanded edge routing (v1.5)",
    highlights: [
      "Universal vertical stems for all styles",
      "Clear junction characters at edge splits",
      "Four-phase algorithm: stem -> junction -> drops -> arrows",
      "RFC-001 specification implemented",
    ],
  },
  {
    day: 6,
    focus: "Edge labels (v1.6)",
    highlights: [
      "Pipe syntax: A -->|label| B",
      "Text syntax: A -- label --> B",
      "Labels positioned on vertical edge segments",
      "Smart positioning for straight vs L-shaped edges",
    ],
  },
  {
    day: 7,
    focus: "Node shapes (9 shapes)",
    highlights: [
      "Rectangle [Label], Rounded (Label), Diamond {Label}",
      "Circle ((Label)), Stadium ([Label]), Hexagon {{Label}}",
      "Database [(Label)], Subroutine [[Label]], Asymmetric >Label]",
    ],
  },
  {
    day: 8,
    focus: "Codebase consolidation",
    highlights: [
      "Parser refactoring: 124 lines -> 25 lines via helper extraction",
      "29 new render module tests (canvas + edge routing)",
      "Fixed junction direction semantics (T-down at box bottoms)",
      "110 tests passing, comprehensive coverage",
    ],
  },
];

export default demoTimeline;
