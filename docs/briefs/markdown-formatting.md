# Brief: Conservative Markdown Source Formatting

Researched: 2026-11-01 | Versions: Prettier 3.x, Biome v2.x, mdformat 1.0.0, dprint-plugin-markdown (Rust, MIT), comrak 0.59 | Overall confidence: high on documented formatter behavior; medium on Biome's Markdown support (new, thin docs)

## 1. Baseline

A Markdown auto-formatter sits between two architectures. **Full reprint** parses to an AST and prints it back (Prettier via micromark→mdast, mdformat, Biome): every construct is re-emitted in canonical style, so the whole document changes even when only one thing was "wrong." **Source-faithful** parsing retains each node's exact source text and re-emits untouched nodes verbatim (dprint-plugin-markdown: "every node knows the exact text it was parsed from and nothing in the file is discarded along the way").

The central tension is **prose wrapping**. A soft line break (single newline inside a paragraph) is insignificant in strict CommonMark but significant in linebreak-sensitive renderers (e.g. GitHub comments, BitBucket) — so joining or splitting prose lines can change what readers see. All four tools resolve this with the same tri-state control, and all four default to the conservative setting:

| Tool | Setting | Values | Default |
|---|---|---|---|
| Prettier | `proseWrap` | `preserve` / `always` / `never` | `preserve` |
| Biome | `markdown.formatter.proseWrap` | `preserve` / `always` / `never` | `preserve` |
| mdformat | `--wrap` | `keep` / `no` / `INTEGER` | `keep` |
| dprint markdown | wrapping config | maintain-style default (`lead`) | maintain-like |

`always` reflows the whole paragraph to the width (joining short lines *and* splitting long ones); `never` joins each block to one line; the default leaves line breaks exactly as authored. Vocabulary to use: **preserve/keep** (byte-stable prose), **reflow** (join+split to width), **wrap-only** (split overlong lines, never join — offered by none of the four as a preset; it is custom behavior).

Two cross-tool facts a fresh session needs. First, **print width is a preference, not a cap**: Prettier's docs state explicitly that `printWidth` is "roughly how long you'd like lines to be," not a maximum — long unbreakable content still exceeds it, and short lines get lengthened toward it under `always`. Second, **idempotency is a hard requirement**: formatting twice must be a fixpoint, and mdformat enforces a stronger variant as a CLI safety check — it refuses to write output whose parsed AST differs from the input's (HTML-equivalence before/after).

## 2. What research adds

- **The conservative default is unanimous, and its rationale matches the task-notes use case.** Prettier: "will not change wrapping in markdown text since some services use a linebreak-sensitive renderer." mdformat goes further, endorsing Semantic Line Breaks (one sentence per line, after Kernighan 1974) as the authoring style its `keep` default protects. A formatter for human-maintained notes should therefore default to preserve/keep; `always` exists for projects that want uniform width. (Prettier options doc, mdformat style doc — both primary.)
- **Reflow-to-width is precisely the "short text gets rewritten" mechanism.** Under `always`, the printer fills every paragraph to the width: short authored lines are joined. Any width-setting render option on a reprint path does this — it cannot selectively split only overlong lines, because the printer never sees the original breaks as content. If wrap-only is wanted, it must be built as a separate pass over source lines, not as a renderer width. (Prettier options doc + prose-wrap behavior table — primary.)
- **Source-faithful parsing is an established architecture, not an exotic one.** dprint-plugin-markdown exists because its author concluded rendering parsers are the wrong substrate: it ships its own Rust parser "written for formatting rather than rendering." For a Rust project that already depends on a parser exposing per-node source positions (comrak populates `sourcepos` start/end line+column on every AST node), the equivalent is a **splice architecture**: parse for structure, apply edits as source-range replacements, re-emit untouched ranges verbatim. No new dependency is implied. (dprint-plugin-markdown README — primary, read in full.)
- **The agreed normalization set (reprint tools) is small and stable.** Across mdformat's style doc and Prettier's behavior: ATX-only headings, fenced-only code blocks, one bullet/ordered marker style each (mdformat uses `-` and non-consecutive `1.` explicitly to minimize diffs), single blank line between blocks, trailing-whitespace stripping, single trailing newline. Notably, mdformat sorts/moves link reference definitions and minimizes code-span backticks — opinionated steps a conservative formatter should *omit* unless asked. (mdformat style doc — primary, read in full.)
- **Hard breaks normalize to backslash, not double-space.** mdformat rewrites trailing-double-space breaks to `backslash + newline` because trailing spaces are invisible. Any strip-trailing-whitespace pass must therefore exempt (or pre-convert) hard-break markers, or it silently deletes line breaks. (mdformat style doc — primary.)
- **Table pipes are the data-loss hotspot.** Pipes inside code spans must be escaped (`\|`) in canonical output; any alignment pass that splits rows on raw `|` characters corrupts them. Alignment must use parser-given cell boundaries, never string splitting. GFM table pipe-alignment itself is standard reprint-tool behavior (`lead`: verified from usage, not a primary doc read).
- **Wrapping inside containers is where formatters break.** Biome, the newest entrant (Markdown formatting landed in the v2.x line; config reference documents `markdown.formatter.lineWidth` default 80 plus the same proseWrap tri-state), has an open issue of `always`/`never` dropping `>` markers across multi-paragraph quotes (`lead`: single issue report). Rule: a v1 wrapping pass should handle top-level paragraphs only and leave blockquote/list-nested prose byte-identical.
- **Fenced code blocks are a formatting boundary, and tools recurse into them.** Prettier formats embedded code by default (`embeddedLanguageFormatting: auto`, fenced blocks included); Biome dispatches on the info string and leaves unrecognized ones unchanged; dprint delegates to sibling plugins. For a conservative v1, leaving code-block contents untouched is the safe, documented option. (Prettier options + Biome language-support docs — primary.)
- **When NOT to use full reprint.** If the corpus is human-maintained notes with meaningful soft breaks, short files, and frontmatter-like metadata, full reprint maximizes diff noise for zero content gain — every save re-renders the file in the tool's house style. That is the documented cost Prettier's `preserve` default and mdformat's `keep` default both exist to avoid. Prefer splice + preserve unless the project explicitly wants house style. (Reasoning from the above primaries.)

**Recommended approach for this context.** Splice architecture on the existing parser: one shared options constructor so renderer and formatter can never drift on extensions; normalization set limited to (a) trailing-whitespace strip with hard-break exemption, (b) table alignment from AST cell ranges, (c) wrap-only overlong top-level paragraph lines at the configured width on word boundaries, (d) everything else byte-identical; validate with a fixpoint test plus an AST/HTML-equivalence check on every format (mdformat-style safety net as a test, not only a CLI flag).

## 3. Worker checklist

Do:
- Confirm the parser exposes 1-based start/end source positions on block nodes (verify against the pinned parser version's AST struct, not memory).
- Build exactly one options constructor shared by rendering and formatting paths; add a test asserting both enable the same extensions.
- Write one failing test per normalization item before implementing (short-lines-preserved, hardbreak-preserved, pipe-in-code-span table, overlong-wrap, fixpoint).
- Add an equivalence gate: rendered-HTML-before must equal rendered-HTML-after for every formatter test input.
- Restrict wrapping to top-level paragraphs in v1; assert container-nested prose passes through untouched.

Don't:
- Set any renderer "width" option on the formatting path — that is the reflow mechanism, not a cap.
- Split table rows on raw delimiter characters anywhere in the pipeline.
- Strip trailing whitespace without exempting hard-break markers.
- Normalize marker styles, heading styles, reference-link placement, or code-span backticks in a conservative formatter — each is diff noise, not normalization.
- Trust snippets or secondary posts for option names/behavior; verify each against the primary doc or the tool's own test suite.

## 4. Sources

- Prettier options reference (`prettier.io/docs/options`, read in full): proseWrap/printWidth semantics, micromark parser, embedded-code default, LF default — the canonical statement of preserve-by-default.
- mdformat formatting-style doc (`mdformat.readthedocs.io`, read in full): keep-default rationale, semantic line breaks, backslash hard breaks, diff-minimizing marker choices, AST-equality safety check.
- mdformat CLI usage doc (read via search excerpt + docs): `--wrap {keep,no,INTEGER}` default keep, `--check`/`--no-validate` flags.
- dprint-plugin-markdown README (read in full): source-faithful parser architecture ("written for formatting rather than rendering"), MIT, Rust.
- dprint Markdown plugin page (read in full): ignore-comment mechanism (`dprint-ignore`), code-block delegation to sibling plugins.
- Biome language-support + configuration reference (read in full): Markdown parse/format/lint supported in v2.x, `markdown.formatter.lineWidth`/`proseWrap` with preserve default, info-string code formatting.
- Biome issue #12507 on quote-marker loss (`lead`, single issue snippet): cautionary evidence for the containers rule.
- CommonMark spec + GFM extensions (background, uncited specifics): what "extension parity" means for the shared options constructor.
