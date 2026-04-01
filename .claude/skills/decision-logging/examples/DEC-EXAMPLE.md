# Scoring Recipe Promotion Gate

**Status:** Decided
**Date:** 2026-03-26
**Decider:** Training lead
**Context:** The best Path A fine-tune recipe reached `rho = 0.2919`, just
below the `rho >= 0.30` start gate for Phase 1D. The team needs to choose
whether to run a narrow confirmatory search or relax the gate immediately.
**Decision:** Run one narrow local search around the best recipe before any gate
relaxation. The gap is small enough that a confirmatory run is justified, but
large enough that a silent exception would weaken future gate discipline.
**Consequences:** Phase 1D remains blocked until the confirmatory search
completes or leadership explicitly changes the gate. The next plan and run
artifacts should reference this decision.

## Options Considered

1. **Proceed immediately on the near-pass.**
   Pros: faster sequencing.
   Cons: weakens trust in the gate.
2. **Run a narrow confirmatory search.**
   Pros: preserves discipline while keeping cost bounded.
   Cons: adds short-term delay.
3. **Pause for a broader redesign first.**
   Pros: may produce a bigger leap.
   Cons: delays learning on the current frontier.

## Criteria

- gate integrity
- time-to-learning
- incremental GPU cost
- confidence in the next sequencing decision

## Revisit Trigger

Reopen immediately if the confirmatory run fails to improve, or if new external
evidence changes the expected ceiling of the current recipe family.
