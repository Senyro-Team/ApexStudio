# Workspace and ERLCX stability

- The workspace retains an idle SQLite connection for its lifetime to avoid repeated WAL cleanup between short-lived operation connections. Connection setup retries only SQLite locking-protocol failures, with a bounded delay; it does not replay writes or external requests. Persistent filesystem or external locking faults can still require investigation.
- `rusqlite` 0.39 bundles SQLite 3.51.3, including SQLite's documented WAL-reset correction. This update is preventative; the reported locking-protocol message has not been proven to have been caused by that separate SQLite bug. WAL and `synchronous=FULL` remain enabled.
- The ERLCX assistant allows the wizard up to ten seconds to hydrate when the initial layout is unknown. It still stops for an unknown layout after that interval, login problems, CAPTCHA, conflicts, validation errors, and at final review.
- ERLCX review groups livery codes behind expandable summaries. A failed initial draft or livery-list load can be retried without restarting or discarding edits.

Reference: https://www.sqlite.org/wal.html and https://www.sqlite.org/rescode.html

## Native review (2026-10-28)

- Built the optimized Windows app and launched it through Explorer in Tiki Workspace. The existing 16-livery draft displayed 196 codes behind the collapsed review summary; expanding it revealed per-livery summaries.
- After reloading the unpacked extension, tested a separate local demo draft with a fictional decal ID and the two authorized Apex banners. The assistant reached ERLCX Submit without a manual resume. The agreement remained unchecked and neither Save Draft nor Publish was selected. Apex Studio reported the final review after returning to its window.
- The previous intermittent locking-protocol failure was not reproduced in this session; a single successful run cannot establish that every cause has been eliminated.
