# Maestro Run Checkpoints Guide

Checkpoints exist so long or interrupted runs can resume without losing
important state.

## What A Good Checkpoint Contains

- current branch and working tree shape
- active scope or task identifier
- key files changed or examined
- unresolved blockers or next decisions
- next concrete action for the next session

## Rules

- checkpoints summarize progress; they do not replace source-of-truth docs
- stale checkpoints should be consumed or cleared deliberately
- private runtime state belongs in the consumer repo or external state root, not
  in the shared Maestro source
