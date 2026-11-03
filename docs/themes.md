# Themes

Canonical reference for theme loading, fields, and color grammar. Linked
from `README.md`, `AGENTS.md`, and `docs/config.md`.

## Lookup paths

Theme TOML files are discovered in priority order:

1. Explicit `themes_dir` from config (or `PLAN_THEMES_DIR` / `GLOSS_THEMES_DIR`).
2. `./themes/` relative to the current directory.
3. Platform config dir: `~/.config/<app>/themes/`.
4. User share dir: `~/.local/share/tablethat/themes/` (honors `$XDG_DATA_HOME`).
5. System dirs: `/usr/local/share/tablethat/themes/`, `/usr/share/tablethat/themes/`
   (honors `$XDG_DATA_DIRS`).

The first directory that exists and yields at least one valid theme wins;
later directories are not consulted.

Cycle themes at runtime by pressing `c` in the markdown viewer.

## Theme file format

```toml
name = "my-theme"

[theme]
h1_color = "#FF8800"
h2_color = "#FF8800"
h3_color = "#FF8800"
code_color = "cyan"
code_block_color = "yellow"
bold_style = "bold"
emphasis_style = "underlined"
```

## Fields (7)

All fields are required except `code_block_color`, which defaults to
`yellow`:

| Field             | Type   | Description                              |
| ----------------- | ------ | ---------------------------------------- |
| `h1_color`        | color  | Level-1 heading color (bold + underlined) |
| `h2_color`        | color  | Level-2 heading color (underlined)       |
| `h3_color`        | color  | Level-3+ heading color                   |
| `code_color`      | color  | Inline-code color                        |
| `code_block_color`| color  | Fenced-code-block color (default yellow) |
| `bold_style`      | string | `bold`, `dim`, `italic`, or `underlined` |
| `emphasis_style`  | string | Same vocabulary (default `underlined`)   |

## Color grammar

Color values may be:

- **Named:** `red`, `green`, `blue`, `cyan`, `magenta`, `yellow`, `gray`,
  `darkgray`, `white`, `lightred`, `lightgreen`, … (ratatui named colors).
- **ANSI256 decimal:** `42` (a number 0–255).
- **Hex:** `#FF8800` (RGB).

## Included themes

`codesam`, `default`, `dracula`, `matrix`, `matrix2`, `molokai`, `mono`,
`morning`, `nord`, `solarized`, `warm`.
