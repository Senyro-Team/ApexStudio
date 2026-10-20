# Phase 04 – Lokale PNG-Pipeline, Dateinamen und Hashes

## Status

**Teilweise umgesetzt; vollständige Phase-4-Abnahme offen.** Der lokale Rust-Kern und ein CLI-Einstieg sind vorhanden. Die bestehende Desktop-Oberfläche besitzt noch keinen Importdialog oder Batch-Workflow. Phasenberichte 2 und 3 bleiben in ihrem bisherigen Status.

## Implementiert

- `apex-images` enthält einen versionierten Aliasparser für kanonische Slots, CamelCase/Unicode-Token, Fenster-/Kennzeichen-Aliase, sichtbare Indexannahmen und Konflikte. Er liest nur den Dateibasisnamen, liefert Bytebereiche im Originalnamen und unterscheidet `high`, `inferred` und `ambiguous`.
- `find_duplicate_slots` gruppiert nur innerhalb des vom Aufrufer gesetzten Design-/Vehicle-Variantschlüssels. Variantenerkennung selbst bleibt Sache des Projektmodells.
- PNG wird mit `png` 0.18.1 real dekodiert. Grenzen: höchstens 20.000.000 Eingabebytes, jede Kante 1–8.000 Pixel. APNG und 16-Bit-PNG werden vor Pixelkonvertierung abgewiesen. Es gibt kein Resize oder Rekodieren. Der Decoder nutzt `EXPAND` nur für gepackte/palettierte Formate und tRNS; keine EXIF-Rotation oder Farbprofil-/Gamma-Transformation wird angefordert.
- `file_sha256` hasht die exakten Quelldateibytes. `pixel_sha256_v1` hasht `APXSTD-PIXEL-v1\0`, Breite/Höhe als u32 BE und RGBA8 oben-nach-unten; bei Alpha 0 werden RGB-Bytes auf 0 gesetzt.
- `phash_v1` ist eine feste Ganzzahlpipeline: Alpha auf Weiß mit `(channel*a + 255*(255-a) + 127)/255`, Luma `(299R+587G+114B+500)/1000`, 32×32 Center-nearest Rasterung mit `floor(((2*i+1)*source_dimension)/64)` und Clamping, zweidimensionaler DCT-II. Die Q20-Cosinuskoeffizienten sind als Konstanten abgelegt; DC wird vom Median ausgeschlossen, Median ist sortierter Eintrag 31 der übrigen 63 Koeffizienten. Bit 0 ist DC und bleibt 0; Bit `v*8+u` wird gesetzt, wenn der Koeffizient strikt größer als der Median ist. Das ist ein Ähnlichkeitssignal, kein Identitäts- oder Urheberschaftsnachweis.
- `phash_hamming_distance` liefert die Bitdistanz nur als Ähnlichkeitsmaß. `suggested_roblox_display_name` liefert `APXSTD-` plus volle lowercase Datei-SHA-256; ein Providerlimit wird erst in Phase 5 geprüft.
- `BlobStore` schreibt SHA-256-adressierte Rohbytes unter `blobs/<prefix>/<sha256>.png`, publiziert ohne Überschreiben per Hardlink, macht den finalen Blob schreibgeschützt und prüft Referenzpfad sowie Inhalte beim Lesen erneut. `import_path` verweigert Symlink-Quellpfade und nicht reguläre Dateien. Der Originaldateiname bleibt in der Blobreferenz erhalten.
- CLI-Einstiege: `apex image inspect <path> [--json]` und `apex image import <path> [--json]`. Importablage ist `%LOCALAPPDATA%/Apex Studio/blobs` (über `dirs::data_local_dir`). Tauri bietet `suggest_image_slot(filename)` und überträgt keine Bilddaten als JSON.

## Versions- und API-Vertrag

- Verwendete Toolchain: `rustc`/`cargo` 1.96.0, Node.js 24.19.0, pnpm 12.8.1. Neue direkte Bibliotheken: `png` 0.18.1 und `unicode-normalization` 0.1.26; SHA-256 nutzt das bereits vorhandene `sha2` 0.11.1. `cargo info` und die offiziellen API-Seiten wurden am 2026-10-20 geprüft: [png 0.18.1 Decoder](https://docs.rs/png/0.18.1/png/struct.Decoder.html), [png Transformations](https://docs.rs/png/0.18.1/png/struct.Transformations.html), [png Info](https://docs.rs/png/0.18.1/png/struct.Info.html), [Unicode Normalization](https://docs.rs/unicode-normalization/0.1.26/unicode_normalization/).
- Parser-Version 1, Pixelhash-Version 1 und pHash-Version 1 sind getrennte Konstanten.
- Golden-Vektor: 2×1 RGBA8 `[10,20,30,255, 40,50,60,0]` ergibt `pixel_sha256_v1=a1cbae2ab2650fb3b043ee38eb9b450f5d1cef7e6ada3990e0b0c9778f6c371a` und `phash_v1=0000000000000088`.

## Verifikation

- `cargo test --workspace --locked` – bestanden, insgesamt 26 Tests, davon 8 Bildtests. Enthalten sind Aliasvarianten, `Bright`-Abweisung, Mehrdeutigkeit, Varianten-Dubletten, PNG-Signatur, APNG-/16-Bit-Ablehnung, Byte-Guard, Kanten 7.999/8.000/8.001, Null-/extreme Header, transparenter RGB-Vertrag, Größenbindung, Golden-Hashes, Blobpfadprüfung und gefrorener Blob nach Quelländerung.
- `cargo clippy --workspace --all-targets --locked -- -D warnings` – bestanden. `pnpm typecheck` und `pnpm build` – bestanden. CLI-Smoke `apex image inspect apps/desktop/src-tauri/icons/StoreLogo.png --json` – PNG dekodiert und JSON mit Hashes und Slotkonflikt ausgegeben.
- `cargo check --workspace --offline` – bestanden nach Einbindung in CLI/Tauri. Das Bildprüfen und -importieren selbst benötigt keine Netzwerkaktivität.
- Frühere erfolgreiche Phase-3-Workspace-Tests sind in `PHASE-03.md` dokumentiert; ein erneuter vollständiger Workspace-Test, Clippy, Frontend-Build sowie Windows-GUI-E2E sind noch nicht gelaufen.

## Offene Kriterien und Risiken

- Keine Desktop-Dateiauswahl/-Importansicht, korrigierbare Slotliste, Fortschritt, Batchabbruch oder echte Uploadsperre in der UI. Der Parsercommand liefert nur einen Vorschlag; UI-Bestätigung muss vor späterer Uploadvorbereitung umgesetzt werden.
- Kein begrenzter Workerpool/Jobqueue. CLI arbeitet synchron und einzeln. 100-Dateien-Fortschritt/Abbruch und 8.000×8.000-Pixel-UI-Reaktionsfähigkeit wurden nicht ausgeführt. Pixelverarbeitung hält einen dekodierten Puffer von bis zu 256 MB; Decoderinterne Limits sind best-effort und auf 32 MiB gesetzt.
- SQLite-Asset-/Assetversion-Datensätze, relationale Blobreferenzen, Crash-Recovery für Importtransaktionen und Phase-5-Uploadjobbindung sind nicht implementiert. Der Blobspeicher ist derzeit ein eigenes Dateisystemmodul und kein Phase-2-Repositoryadapter.
- Nicht alle Einzelwerte der 19.999.999/20.000.000-Bytegrenze wurden mit gültigen PNGs erzeugt; es wurde geprüft, dass 20.000.001 Bytes vor Decoderzugriff am Byte-Guard scheitern und 20.000.000 Bytes diesen Guard passieren. APNG, Quell-Symlink und tatsächliche Quelländerung während laufenden Imports haben noch keine eigenen ausführbaren Tests.
- `BlobStore::import_path` vergleicht Größe und Änderungszeit vor/nach dem Lesen; exotische Dateisysteme mit gleichbleibendem Zeitstempel können parallele gleich große Quelländerungen nicht zuverlässig erkennen. Der importierte Blob selbst wird anschließend eingefroren und auf Hash-/Metadatenabweichung geprüft.
- pHash-Golden-Vektor wurde auf der aktuellen Windows-Toolchain ausgeführt. Q20-Konstanten machen die DCT-Rechnung plattformstabil; zusätzliche Windows/Linux- und ARM-Goldenläufe fehlen.
- Kein echter Upload, Roblox-Account, Netzwerkadapter oder Server wird in Phase 4 verwendet.

## Übergabe

Phase 5 kann nach Abschluss eines Asset-Repository-/Importtransaktionsvertrags die geprüfte `BlobRef` samt Hashes übernehmen. Phase 6 kann Parser-Suggestion, Span, Regelbegründung und Konfliktstatus präsentieren. Die nächste notwendige Integrationsarbeit ist UI-Dateiauswahl und bestätigbarer Import mit begrenzter Workerqueue; die noch offenen Phase-2/3-Datenschichtpunkte bleiben eigenständig offen.
