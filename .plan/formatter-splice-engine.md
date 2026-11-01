---
status: in-progress
type: feature
priority: medium
area: backend
---

## Splice-engine markdown formatter

Replace the format round-trip with conservative source-splicing:
parse for structure, apply desired edits only, re-emit the rest
byte-identical. Fixes short-line joining, hardbreak restyling,
pipe-in-code-span table loss.

- Design: `docs/plans/markdown-formatter-splice.md`
- Research: `docs/briefs/markdown-formatting.md`
- Branch: `formatter/splice-engine` (worktree `/tmp/opencode/fmt-spike`
  holds the earlier dprint/flowmark spike, `spike/dprint-eval`)

Done: shared `comrak_options()` constructor (renderer + formatter).
Requirements v3: configurable prose wrap (default 80, split-only,
containers wrapped with continuation indent, code spans atomic).
Next: D-2 answer, then iterative implementation breakdown.
