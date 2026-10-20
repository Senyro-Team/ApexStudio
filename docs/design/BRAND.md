# Apex Studio brand and desktop system

## Source inventory and rights

The supplied package manifest records five PNG references and their SHA-256 values. The first three assistance files are byte-identical (1898×586, SHA-256 `1e34dc...e89cc0`); `apex-designs-banner.png` is 1920×201 (`a63296...b439`); `apex-designs-symbol.png` is 760×761 (`4664de...c8ccd5`). The source manifest says the assets came from user attachments and that no public redistribution license is evidenced. The raster files stay in the supplied reference package and are not copied into the application bundle.

## Observations

The supplied references show near-black/charcoal backgrounds, white lettering and an outlined, flowing A mark. The wordmark is broad and heavy, with italic treatment in some imagery. These are visual observations, not measured official brand color values. The exact typeface is not established by the raster references.

## UI decisions

- Semantic palette: background `#111213`, surface `#191B1D`, elevated `#222529`, border `#34383D`, text `#F5F5F2`, muted `#ABB1B8`; these are working tokens, not claimed official brand values.
- A restrained grayscale palette keeps image work central. Focus uses a visible light blue-gray outline; errors have both text and a distinct surface.
- Typography uses system UI (`Segoe UI`, then platform sans-serif); no font is fetched at startup and no unlicensed font is bundled. Display style is limited to short headings and the A monogram.
- The app uses the product name Apex Studio. No personal operator name, support URL or repository URL is invented. The small geometric icon in `apps/desktop/icons/apex-mark.svg` is an application mark, not claimed as a faithful reconstruction of the supplied raster symbol.
- Shell navigation covers Projects, Uploads, Notes, Analytics and Settings. Unimplemented workflow areas state their current status instead of showing fabricated data.

## Accessibility and motion

Controls expose text labels, browser-native keyboard operation and a visible `:focus-visible` ring. The layout reflows to a compact icon rail and wraps forms on narrower viewports; reduced-motion preferences suppress animation. A full 125/150% Windows DPI, screen-reader and native Tauri visual sweep is still outstanding.
