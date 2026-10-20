# Phase 03 – `.apexstd` Paketformat, Export und Import

## Status

**Begonnen; Export und sicherheitsbegrenzte Importvalidierung umgesetzt, vollständige Phase-3-Abnahme offen.** Der Nutzer hat Phase 3 ausdrücklich freigegeben, obwohl Phase 2 noch offene Abnahmepunkte hat. Das Phasenpaket bleibt Spezifikation; nur der aktuelle Auftrag autorisiert den Start.

## Implementiert

- Format v1 mit `manifest.json`, `project.json`, `checksums.sha256` und relativen Paketdateien. Manifest listet `project.json` und Nutzdateien, aber nicht sich selbst oder die Checksumdatei. Die Checksumdatei deckt Nutzdateien und endgültiges Manifest ab.
- Profile `backup_private`, `team_transfer_private` und `client_share`. Private Profile benötigen im CLI eine bewusste `--confirm-private`-Option. Clientmetadaten werden über die vorhandene Commission-Allowlist erstellt. Für Clientdateien lässt der Kern nur explizit als `final_asset` markierte Dateien durch; andere ausgewählte Dateien werden in der Vorschau als ausgeschlossen angezeigt.
- Exportvorschau listet ausgewählte Pfade, Größe, Medientyp, Inhaltsklasse und tatsächliche Aufnahme. Commission-Referenzen auf nicht beigefügte lokale Dateien und HTTP(S)-Referenzen werden als ungeprüft/nicht automatisch geladen angezeigt.
- `.apexstd`-Export schreibt eine partielle Datei im Zielverzeichnis, verweigert vorhandene Ziele und publiziert per Hardlink. ZIP-Einträge enthalten keine SQLite-Datei. SHA-256 ist Integritätsprüfung, keine Signatur oder Urheberbestätigung.
- Import streamt Dateien nach Größenprüfung in ein isoliertes `tempfile`-Staging. Geprüft werden Formatversion, Pfade, Windows-reservierte Namen, case-insensitive Duplikate, spezielle Unix-Dateitypen, Datei-/Gesamt-/Archivlimits, CRC beim vollständigen Lesen, Dateiliste, Medientypen, Größen sowie SHA-256/Checksumdatei. Ein Fehler gibt das Staging beim Verlassen des Scopes frei.
- CLI: `project export ... --profile ...`; private Profile erfordern `--confirm-private`. `project import ... --dry-run` prüft und staged das Paket und ändert keine Projektdaten. Import ohne `--dry-run` wird klar abgewiesen.
- JSON-Schemata und ein kleines, tatsächlich erzeugtes `client_share`-Beispielpaket liegen unter `docs/packages/`.

Verwendete direkte ZIP-/Hashabhängigkeiten: `zip` 9.0.5 mit nur Deflate aktiviert und `sha2` 0.11.1; Versionen sind in Cargo-Manifest und `Cargo.lock` gesperrt. Die offiziellen Paketstände wurden am 2026-10-20 geprüft: [zip 9.0.5](https://docs.rs/zip/9.0.5/zip/) und [sha2 0.11.1](https://docs.rs/sha2/0.11.1/sha2/).

## Verifikation

- `cargo test -p apex-package --locked` – bestanden; 7 Tests zu Profil-Allowlist, Export-/Staging-Roundtrip, Hashabweichung, Zip-Slip-Abweisung und Nichtüberschreiben.
- CLI-Smoke mit isoliertem `%LOCALAPPDATA%`: Projekt angelegt, als `client_share` exportiert und anschließend mit `project import --dry-run` validiert. Zusätzlich wurde mit leerem, isoliertem `%LOCALAPPDATA%` bestätigt, dass der Dry-run keine SQLite-Datenbank anlegt.
- `cargo fmt --all -- --check`, `cargo test --workspace --locked`, `cargo clippy --workspace --all-targets --locked -- -D warnings`, `pnpm typecheck` und `pnpm build` – bestanden. 18 Rust-Verhaltenstests: Core 2, Domain 4, Package 7, Storage 5; weitere Workspace-Crates haben derzeit keine Tests.
- CLI-/Paket-Kompilierung mit `cargo check -p apex-cli --locked` – bestanden.

## Offene Kriterien und Risiken

- **Kein dauerhafter Import.** Es gibt noch kein ID-Mapping für unabhängige Kopien, Commission-ID-Konfliktbehandlung, Updatevorschau, DB-/Blob-Journal, atomaren Commit oder Recovery nach Prozessabsturz. `--dry-run` ist bisher der einzige CLI-Importpfad.
- **Kein konsistenter DB-Snapshot während Export.** CLI liest Projekt und Commissions über getrennte Repositoryaufrufe; parallele Änderungen können einen gemischten Stand ergeben. Unveränderliche Datei-Blobs und Snapshot-Isolation sind noch nicht angeschlossen.
- **Keine verwaltete Asset-/Notizablage im Speicheradapter.** Exporter kann ausgewählte Byteinhalte paketieren, erhält sie aber vom Aufrufer. CLI besitzt noch keinen Datei-Store und exportiert daher derzeit keine lokalen Assets oder Markdown-Notizen. Nicht vorhandene und absichtlich ausgelassene lokale Referenzen lassen sich ohne Datei-Store noch nicht zuverlässig unterscheiden.
- Clientdateien werden anhand einer vertrauenswürdigen Aufrufer-Klassifikation `final_asset` gefiltert. Der Dateiinhalts-/Bildtyp wird in dieser Phase noch nicht dekodiert oder gegen reale Assets validiert. Ein bösartiger Aufrufer könnte Dateien falsch klassifizieren; deshalb ist die Clientdateiauswahl noch nicht als fertiger Desktop-Exportdialog zu verwenden.
- Import hält nur kleine Metadaten (`manifest`, Projekt JSON und Checksums) im Speicher; Nutzdateien werden gestreamt. Standardlimits: 10.000 Manifestdateien, 2 GiB entpackt insgesamt, 100 MiB je Datei, 4 MiB Manifest und 2.250 MiB Archivdatei. Größere legitime Projekte brauchen noch einen bestätigten Limits-Override.
- JSON-Schemas werden dokumentiert, aber noch nicht maschinell in CI validiert. Kein Migrationsadapter für ältere Formatversionen; Versionen ungleich v1 werden abgelehnt.
- Hardlink-Publikation kann auf Dateisystemen ohne Hardlink-Unterstützung scheitern; sie fällt bewusst nicht auf ein Überschreiben oder nichtatomare Veröffentlichung zurück.
- ZIP-Checksum schützt vor versehentlicher/gezielter Veränderung nur im Verhältnis zum beigefügten Manifest; ein Angreifer kann beides neu hashen. Es gibt keine kryptografische Signatur.

## Übergabe

Phase 3 braucht als nächste Schritte eine konsistente SQLite-Lese-Transaktion plus Blob-Snapshot, einen echten Projekt-/Datei-Store und eine wiederanlaufbare ID-gemapte Importtransaktion. Erst danach kann die Format-Roundtrip-Abnahme über Rechnergrenzen, parallele Änderung, ZIP-Angriffe, Absturzpunkte und zweifachen Import abgeschlossen werden.
