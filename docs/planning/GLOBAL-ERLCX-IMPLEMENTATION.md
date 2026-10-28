# Global ERLCX, trash and Frontplate implementation

Baseline: a616cb1, schema 7. Work is split into Frontplate, trash (schema 8), then independent publishing drafts (schema 9) and Chrome native messaging. Check current Git state and schema numbering before each block; preserve concurrent changes. No automatic push.

## Interfaces and ownership

- Core side parser/storage slot validation/UI SlotEditor: canonical `Frontplate N`, default N=1 only for explicit front-plate aliases. Generic plate names stay unassigned. Existing side rules remain.
- Trash: backend-owned recursive deletion groups; typed preview/list/restore/purge IPC. Preserve immutable blob references, versions and upload history. Terminal upload hashes become metadata references independent of available managed bytes; active jobs keep content pinned. No automatic cleanup of user originals.
- Global publishing: typed list/create/read/save/export commands with independent draft ID and revision. Migrate legacy project drafts. Public bundle v1 stays unchanged. Global post drafts travel in workspace backups, not new project packages; legacy package drafts import globally.
- Browser connection: separate Windows native-messaging helper, user-local Chrome host registration, approved extension origin, bounded package chunks and named-pipe IPC. Native settings are machine-local. Explicit public snapshot handoff, one active run, manual final consent/publish.

## Block 1 — Frontplate

Added to filename recognition, storage slot validation and the manual side picker. Number defaults to 1 only for recognized Frontplate names; first following number wins. Downstream Roblox, Markdown and ERLCX use side strings and retain the canonical value without a format change.
