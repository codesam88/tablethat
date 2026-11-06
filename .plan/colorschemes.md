---
status: done
type: feature
priority: medium
area: backend
---

## try using ratatui styles?

Done — replaced `parse_ratatui_color()` with ratatui's built-in
`FromStr` and `serde::Deserialize`. Config structs now store
`ratatui::style::Color` directly. Theme TOML files support named colors,
ANSI256 index, and hex (`#FF8800`). Added `ratatui_to_termcolor()`
bridge for CLI stdout rendering.

## make better colorschemes

Created 12 theme files in `themes/`:

- **default** — cyan headlines, yellow code
- **matrix** — phosphor green CRT digital rain
- **matrix2** — cyan headlines, fluorescent green code
- **codesam** — orange/brown headlines, green code
- **morning** — blue headlines, fluorescent green code
- **dracula** — purple headlines, pink code
- **nord** — cyan headlines, green code
- **molokai** — green headlines, cyan code
- **solarized** — orange headlines, cyan code
- **warm** — yellow headlines, red code
- **mono** — no color, terminal defaults

Added `code_block_color` field for separate inline vs block code
styling.

### try opencode style

Implemented better one as `codesam` theme — orange/brown headlines
with green code variants.
