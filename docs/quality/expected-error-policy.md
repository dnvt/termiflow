# Expected-error policy review

The full visual packet includes intentional failures as first-class corpus rows. They are not sent through perceptual diagram review because they have no diagram frame, but they must still be reviewed and accounted for before the corpus is considered complete.

The packet generator enumerates every Markdown input under `tests/fixtures/inputs` across the requested directions, styles, and render modes. A normal full run currently contains 242 inputs and 968 rows: 956 renderable rows plus 12 expected-error rows.

Use the typed QA flow to close the expected-error side of the packet:

```sh
scripts/visual_audit.sh \
  --styles ascii,unicode \
  --modes default,optimized \
  --out /tmp/termiflow-packet
scripts/visual_validate.sh \
  --packet /tmp/termiflow-packet \
  --baseline tests/fixtures/quality_baseline.json
scripts/review_expected_errors.sh \
  --packet /tmp/termiflow-packet \
  --records /tmp/termiflow-expected-errors.jsonl \
  --next
```

`--next` emits exactly one unrecorded row and a hash-bound record template. Fill in the observation, owner, hypothesis, expected observation, falsifier, and next command, then append it with `--record PATH`. Repeat until `--next` reports `done: true`; finish with `--validate`.

Each record binds the packet manifest, identity, completion marker, packet checksum, run identity, effective policy, metadata, input bytes, stdout/stderr blobs, and checked-in expected stderr. Stale packets, duplicate or unknown rows, changed process outcomes, extra fields, and missing falsifiers fail closed. This ledger is intentionally separate from visual perceptual decisions, golden approval, homolog review, and holdout review.
