# `.apexstd` Paketformat v1

**Stand:** 2026-10-20. Ein `.apexstd` ist ein UTF-8 ZIP-Projekttransfer, keine SQLite-Kopie und kein Credentials-/Accounttransfer.

## Einträge und Integrität

Kanonische Metadaten sind `manifest.json`, `project.json` und `checksums.sha256`. Nutzdateien verwenden relative POSIX-Pfade und liegen üblicherweise unter `assets/`, `notes/`, `references/` oder `history/`. `manifest.files` enthält `project.json` und sämtliche Nutzdateien, aber weder Manifest noch Checksum-Datei. `checksums.sha256` enthält eine Zeile `sha256hex␠␠relativer/pfad` pro Nutzdatei und für das endgültige `manifest.json`; die Checksum-Datei enthält sich nicht selbst.

Der Manifestvertrag steht in [`schemas/manifest.schema.json`](schemas/manifest.schema.json), das Projekt-DTO in [`schemas/project.schema.json`](schemas/project.schema.json). Das [Beispielpaket](examples/client-share-empty/client-share-empty.apexstd) liegt sowohl als lesbare Einträge als auch als ZIP zum Dry-Run vor.

## Exportprofile

- `backup_private`: vollständige lokale Projekt-/Commission-Metadaten ohne Secrets oder aktive Uploadjobs.
- `team_transfer_private`: private Metadaten und nur pro Export bewusst ausgewählte Dateien.
- `client_share`: Client-Allowlist; der aktuelle Kern nimmt zusätzlich nur Dateien der Klasse `final_asset` auf. Designquellen, Notizen, Hashhistorie und Finanzdaten bleiben im Paket ausgeschlossen.

Private Profile benötigen in der CLI `--confirm-private`. Die Vorschau zeigt Dateiliste, Metadatenklassen, externe Referenzen sowie lokale Commission-Dateireferenzen, die in der Auswahl fehlen. Die Verfügbarkeit lokaler Dateien kann nur ein Adapter mit Zugriff auf die Ablage bestätigen.

## Importgrenzen der Implementierung

Der aktuelle Import prüft ZIP-Pfade, reservierte Windows-Namen, doppelte case-insensitive Namen, Entry-Typen, Dateianzahl, komprimierte und entpackte Größen, CRC beim Streaming, Manifest-/Dateiliste und SHA-256. Nutzdateien werden zuerst in ein automatisch bereinigtes, isoliertes Staging-Verzeichnis gestreamt. Unbekannte Formatversionen werden abgelehnt. Keine URL wird geöffnet und keine importierte Datei ausgeführt.

Die CLI bietet derzeit `project import --dry-run`; ein dauerhafter Import-/Kopiercommit in SQLite und Dateispeicher ist **noch nicht implementiert**. Es gibt noch kein Importjournal, ID-Neuzuordnung, Konfliktdialog, Restore-Profil für Dateien oder Absturz-Recovery über einen DB-/Datei-Commit. Der aktuelle Export bekommt einen Snapshot aus getrennten Repository-Aufrufen, daher ist während paralleler Änderungen noch keine konsistente Datenbank-Snapshotgrenze garantiert. Notizen und Referenzen können im Format mitgeführt werden, sind aber noch nicht aus einem verwalteten Dateispeicher integriert.

## Paketentscheidung

ZIP wird mit `zip` 9.0.5 (nur Deflate aktiviert), SHA-256 mit `sha2` 0.11.1 erstellt. Beide Versionen sind im Workspace festgehalten und `Cargo.lock` enthält die reproduzierbare Auflösung. Export erstellt zuerst eine partielle Datei im Zielverzeichnis und veröffentlicht danach per Hardlink ohne bestehendes Ziel zu überschreiben; Dateisysteme ohne Hardlink-Unterstützung liefern einen Fehler, statt auf nichtatomisches Überschreiben auszuweichen.
