# Apex Studio licensing rationale

**Status: adopted.** Original application source code is licensed under Apache-2.0 in the repository's `LICENSE`; Apex Marks are governed separately by `BRANDING.md`. This file records the rationale and remaining release hygiene; the operative terms are in those files.

## Recommended split

### Application source code: Apache License 2.0

Apache-2.0 is used for original application source code. It is a widely used permissive open-source licence, permits commercial reuse and forks, includes an express patent grant, requires preservation of notices, and explicitly does not grant rights to project trademarks. Its warranty and liability terms are expressly subject to applicable law.

The complete licence text is included in the repository. Do not apply this licence to excluded Apex Marks or third-party materials.

### Apex Studio name and brand assets: separate brand policy

The Apex Studio name, wordmark, logo, app icon and other brand artwork are outside the code licence. `BRANDING.md` permits marks to identify the official application or an unofficial modified fork subject to its identification and non-endorsement terms. It requires written permission to use Apex Marks as the identity of an unrelated product or service, and lets forks remove/rebrand them.

Open-source code licences must allow modified and derivative works. Therefore, “keep Apex branding” is a preference and a limited trademark permission, not a condition imposed on receiving or redistributing the open-source code. Trademark terms are in the separate `BRANDING.md`.

### Other assets and contributions

Fonts and third-party files remain under their own licences; maintain an inventory and include required notices. Apache-2.0's contribution terms cover contributions submitted for inclusion in the project. Document third-party services and credentials used by optional integrations separately from the Apex Studio code licence.

## Warranty and liability wording

The code licence retains Apache-2.0's standard “AS IS” warranty disclaimer and liability limitation. Avoid promising that all liability can be excluded. In Germany, BGB § 276(3) prevents advance exclusion of liability for intent, and § 309(7) limits certain exclusions for personal injury and gross negligence. The exact application depends on context; have German counsel review any standalone German-language disclaimer before release.

The README uses this summary: “The software is provided ‘AS IS’, without warranties, under the terms and limitations in the licence. Nothing in the licence excludes liability where applicable law does not permit it.”

## Release checklist

1. Confirm the copyright/licensor identity and years for future release notices.
2. Maintain dependency and asset attribution where required.
3. Add a security contact and support boundary.
4. Keep signing keys, update channels and release provenance documented separately from the source licence.
