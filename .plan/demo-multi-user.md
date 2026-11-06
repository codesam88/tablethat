---
status: idea
type: feature
priority: low
area: backend
---

## Multi-user collaboration on .plan/

### Problem

The .plan/ directory is local filesystem only. Two people can't work on
the same board without sharing a filesystem (NFS, shared git repo).
There's no locking, no conflict resolution, no real-time sync.

### Approaches

| Approach       | Sync model   | Conflict resolution | Real-time | Complexity |
| -------------- | ------------ | ------------------- | --------- | ---------- |
| Git-only       | Pull/push    | Manual merge        | No        | Low        |
| File server    | Central      | Last-write-wins     | Yes       | Medium     |
| CRDT           | Peer-to-peer | Automatic           | Yes       | High       |
| Git + presence | Pull/push    | Manual + awareness  | Partial   | Medium     |

### Git-based (simplest)

Treat .plan/ as the source of truth. Users `git pull` before working,
`git push` after. Conflicts are regular git conflicts on YAML
frontmatter.

```
User A:  plan add feature-x  ─►  git push
User B:  git pull            ─►  plan list (sees feature-x)
```

**Pros:** No new infrastructure. Works today. **Cons:** No real-time.
Merge conflicts on YAML are ugly. No presence awareness.

### Server-based

A lightweight daemon watches .plan/ and serves changes over WebSocket:

```
User A  ──HTTP──►  plan server  ──filesystem──►  .plan/*.md
User B  ──WS────►  plan server  ◄─inotify──────
```

**Operations:**

- `plan server` — start the sync daemon
- `plan connect <url>` — connect to a remote server
- Changes broadcast to all connected clients

### CRDT-based

Each .plan/\*.md file becomes a CRDT document (automerge, yrs).
Frontmatter fields merge automatically, body text uses operational
transform.

```toml
# .plan/feature-x.md — CRDT-merged
status: "open"      # ← last-write-wins on conflict
type: "feature"     # ← simple key-value, easy to merge
priority: "high"    # ← easy

## Notes
Body text here...   # ← harder, needs OT or block-level merge
```

### Conflict analysis

| Field      | Type              | Merge strategy                 |
| ---------- | ----------------- | ------------------------------ |
| `status`   | enum              | Last-write-wins                |
| `type`     | enum              | Last-write-wins                |
| `priority` | enum              | Last-write-wins                |
| `area`     | string            | Last-write-wins                |
| Body text  | freeform markdown | Block-level or last-write-wins |

Frontmatter is trivially mergeable. Body text is the hard part — but
for task management, the frontmatter is what matters most.

### Exploration needed

- How do other file-based tools handle this? `todo.txt` has no sync.
  `org-mode` uses git or org-protocol. Obsidian uses git or their paid
  sync service.
- Is "share via git" good enough for v1? Probably yes — document the
  workflow, revisit real-time later.
- Does this even belong in the core tool? Or is it a separate
  `plan-sync` utility?
