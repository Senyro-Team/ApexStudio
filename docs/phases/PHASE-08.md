# Phase 08 – Apex Branding und Desktop-Shell

**Status: Teilabnahme; Phase nicht vollständig abgenommen.** Checked 2026-10-20. Brand inventory and UI decisions: [BRAND.md](../design/BRAND.md).

## Implemented

Replaced font assumptions with a local system-font stack and semantic tokens based on the design brief. The desktop shell has labeled navigation for Projects, Uploads, Notes, Analytics and Settings, a local/offline status, responsive wrapping and reduced-motion behavior. Future workflow views show honest empty states rather than fabricated metrics or upload state. No remote fonts or startup requests are introduced. Project/commission interactions remain in place.

The reference package manifest reports no public redistribution license for its raster assets; source references remain unmodified and are not copied into the app. Existing `apex-mark.svg` is documented as an app mark, not a faithful raster recreation.

## Verification and acceptance

| Criterion | Result |
|---|---|
| Apex recognition and consistent Apex Studio naming | Shell branding and palette updated; full reference visual compare still open. Partial |
| Offline entry and provider-unconnected state | UI has explicit local/no-upload status. Production network trace not captured. Partial |
| Keyboard interaction and focus | Native buttons, labels and visible focus rules; keyboard E2E/accessibility audit not run. Partial |
| Support links valid and voluntary | No support link is configured or invented; setup UX remains open. Partial |
| No third-party font/image requests | Bundled frontend references no external font/image URL; runtime network capture still open. Partial |

`pnpm typecheck` and `pnpm build` pass after shell change. The Vite shell was visually checked at the available 878×884 browser viewport, and all five navigation destinations were exercised; the regular browser correctly showed the Tauri-only project bridge as unavailable. Visual acceptance at 1024×720, 1280×800, large monitor, 125/150% DPI, native Tauri, long labels, screen reader and contrast tooling is still required; therefore Phase 8 is not fully accepted.
