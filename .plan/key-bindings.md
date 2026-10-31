---
status: open
type: feature
priority: medium
area: 
---

# Need more/consistent key bindings


## interactive view (plan b)

- search option (typing / opens a search field in status bar, allow typing
  immediately and use the current entered string as a filter on the displayed
  slugs. The matching part of the slug should be highlighted. Use esc to exit the
  search and reset the filter. Enter to confirm the filter and return to filtered
  interactive kanban view. Esc lifts the current filter.

  This replaces the option of 'f' filtering by selecting a slug. The left-right
  selection and 'f' filter feature should be limited to type/priority/area
  columns.

- when pressing Esc to exit the view, add a debounce: the esc to exit function
  should only actually quit if the last Esc press was more than 2 seconds ago.
  If less than two seconds, require another explicit Esc action 
  print in the status bar: 'Press Esc again to exit', swallow any remaining Esc
  key presses (200ms timeout?), and only exit on the next explicit 'Esc' press.

- page-up/page-down for browsing large issue lists
- the order should be reversed: in-progress first, followed by open, backlog, idea, then blocked and done
- support wrap-around from top-to-bottom, bottom-to-top
  - except that pageup/down should first land on the final item, only wrap-around if already on final or first item)
- add "add" function in kanban browser to start editing a new slug.
  - interactive dialog for the slug name
  - next to it, a matrix for selecting type/status/priority/area using arrow keys (values taken from current schema)
    - space selects the selected type/status/priority/area, enter launches into editor

- make keybindings fully configurable?


# gloss / viewer features

- pageup/pagedown is only going about half a page. need to orient based on
  current dimensions

- the search command should have a history and and allow forward/backward
  search, like 'less'

- think about other essential 'less' features to support

- tables currently cut off cell content, should wrap and build multi-line cells.

- table borders should be more subtle?

- need a real test harness for markdown rendering

- for supported editors, launch editor with same/current cursor position
