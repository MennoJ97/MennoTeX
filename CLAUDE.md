# MennoTeX: instructions for Claude (and other agents)

MennoTeX is a TeX Live based distribution for Apple Silicon that installs
packages on the fly. **Start every session with `knowledge/project/handoff.md`**:
current state, how to rebuild a working setup, decisions the user made, and the
prioritized next steps. `knowledge/index.md` maps the rest of the curated
knowledge; `PLAN.md` holds the original design and roadmap.

When you stop, update `knowledge/project/handoff.md` so the next session can
continue: what changed, what is next, what the user decided.

## Keep the knowledge base current (required)

`knowledge/` is an [Open Knowledge Format](https://github.com/GoogleCloudPlatform/knowledge-catalog/blob/main/okf/SPEC.md)
v0.2 bundle. Treat it as part of the code: a change is not done until the
bundle reflects it.

- **Same commit.** When a change alters behaviour, layout, a file format, a
  module's responsibility, the roadmap status, or reveals an upstream fact,
  update the matching concept in the same commit:
  - `knowledge/project/status.md`: phase progress and measurements
  - `knowledge/architecture/*`: how mtx works (flow, formats, module map)
  - `knowledge/upstream/*`: verified facts about kpathsea, tlnet, tlmgr, MiKTeX, with file:line or URL
  - `knowledge/decisions/NNNN-*.md`: one file per decision (new decisions get the next number)
  - `knowledge/playbooks/*`: how to build, test, debug
- **Log it.** Add a line under today's date (`## YYYY-MM-DD`, newest first) in
  `knowledge/log.md`, starting with `**Creation**`, `**Update**` or `**Deprecation**`.
- **Frontmatter.** Every concept needs `type`. Also set `title`, `description`,
  and `generated: { by: claude-code/<model>, at: <ISO 8601 UTC> }` on every edit.
  Add `verified` only for what was actually checked: `process:cargo-test` for
  tested behaviour, `human:<github-id>` only when the user confirmed it. Cite
  external facts in `sources` and attribute them with footnotes keyed to `sources[].id`.
- **Index.** New concepts get an entry in their directory's `index.md` using the
  concept's `description`. Mark superseded concepts `status: deprecated`;
  do not delete them.
- **Check.** Run `python3 tools/check_okf.py knowledge` before committing; it
  must report `OK` with no broken-link warnings.
- Record failures and surprises (bad mirrors, upstream quirks) as facts, not
  just fixes; they are the most valuable knowledge here.

## Working conventions

- Commit in small, traceable steps with descriptive messages, and push to
  `origin main` (private GitHub repo `MennoJ97/MennoTeX`).
- `cargo build --release && cargo test` must pass before each commit.
- Test end to end on real `.tex` files when a change affects installing or
  compiling: bootstrap a scratch root (never `~/Library/MennoTeX` unless asked),
  then compile with `PATH=<root>/bin/universal-darwin:/usr/bin:/bin`. This Mac
  has MiKTeX symlinks in `/usr/local/bin` that otherwise shadow ours. See
  `knowledge/playbooks/development.md`.
- `mtx ensure` must never write to stdout except the resolved path: kpathsea reads it.
- Never weaken verification (signature, size, SHA-512) to work around a bad
  mirror; fail over to another mirror instead.
