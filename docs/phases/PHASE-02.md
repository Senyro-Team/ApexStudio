# Phase 02 – SQLite, Projekte und Commission-Metadaten

## Status

**In Arbeit; persistenter Projekt-/Commission-Kern umgesetzt, vollständige Phase-2-Abnahme offen.** Der Phasenauftrag ist der Implementierungsrahmen. Eingebettete Paketregeln zu weiteren Phasen oder automatischen Folgeaufträgen erweitern diesen Arbeitsauftrag nicht.

## Implementiert

- `rusqlite` 0.40.2 ist mit `bundled` und `backup` in `Cargo.lock` fixiert. SQLite wird mit `foreign_keys=ON`, WAL, `synchronous=FULL` und fünf Sekunden Busy Timeout geöffnet. Migration v1 läuft vor der Repository-Nutzung; eine Datenbank mit neuerem `user_version` wird abgewiesen und nicht zurückgestuft.
- Standardpfad: `dirs::data_local_dir()/Apex Studio/apex-studio.sqlite3` (Windows: `%LOCALAPPDATA%/Apex Studio/apex-studio.sqlite3`). CLI und Desktop verwenden diesen Pfad und die gleichen Core-Services.
- Schema v1 enthält Workspace, Projekte, Commissions, Clients, Organisationen, Designer, DesignSets, Assets/Versionen, Referenzmaterial, Notizen, Uploadjobs/-versuche, Asset-IDs, Hashes, Einstellungen, Revisionen und Outbox. Die späteren Asset-/Uploadtabellen sind Verträge/Grundgerüst, keine aktiven Workflows.
- Projekte speichern Name, Beschreibung, Tags, UTC-Zeitpunkte, Archivstatus und Revision. Suchen verwendet gebundene Parameter. Änderungen und Archivierung sind revisionsgeprüft, atomar und in einer Transaktion mit Revision-/Outboxeintrag.
- Commissions tragen interne UUIDv7 und transaktional vergebene `COM-YYYY-NNNN`-Nummer. Modell und SQLite-Roundtrip umfassen Client/Organisation, Anforderungen/Referenzen, Workflowdaten, Design-/Figma-/Exportreferenzen, getrennte Roblox Image-/Decal-IDs, Hashes, Version, Minor-Unit-Preis, Währung/Präzision sowie Zahlungs- und Lizenzdaten. Datumsfolgen, Geldfelder und Währungspräzision werden validiert.
- Client-DTOs werden per Allowlist aufgebaut; interne Notizen, Geld, Zahlungsart und nicht freigegebene Referenzen fehlen in dieser Projektion.
- Backup benutzt rusqlite Online Backup API in eine neue temporäre Datei und veröffentlicht diese danach ohne eine existierende Zieldatei zu überschreiben. Es kopiert nicht nur die aktive SQLite-Hauptdatei.
- Desktop unterstützt Projekt anlegen/bearbeiten/archivieren sowie Commission anlegen/auflisten. Tauri registriert zusätzlich Commission-Update und sichere revisionsgeprüfte Löschung. CLI unterstützt Projekt anlegen/auflisten/ändern/archivieren sowie Commission anlegen/auflisten.

Backup-Verhalten orientiert sich an der [SQLite Online Backup API](https://sqlite.org/backup.html); die gebundene Rust-Schnittstelle ist [rusqlite 0.40.2](https://docs.rs/crate/rusqlite/0.40.2). Versionsstand: 2026-10-20.

## Verifikation

Ausgeführt mit Rust 1.96.0, Node.js 24.19.0 und pnpm 12.8.1:

- `cargo test --workspace --locked` – bestanden; 11 Tests: Domain 4, Core 2, SQLite 5.
- `cargo fmt --all -- --check` – bestanden.
- `cargo clippy --workspace --all-targets --locked -- -D warnings` – bestanden.
- `pnpm typecheck` – bestanden.
- `pnpm build` – bestanden; Vite-Produktionsbundle erstellt.
- CLI-Neustart-Smoke mit isoliertem `%LOCALAPPDATA%`: Projekt in Prozess A angelegt, in Prozess B gefunden; danach Commission `COM-2026-0001` mit zwei Anforderungen angelegt und im Folgeprozess aus SQLite gelesen.
- SQLite-Tests prüfen persistentes Wiederöffnen, stale revision conflict ohne Überschreiben, reversible Archivfilterung, gebundene Suche, sichere WAL-Sicherung samt Restore und Ablehnung einer neueren DB-Version. Commission-Roundtrip prüft private/finanzielle Felder, fehlende Deadline/Preis, ausdrücklich kostenlosen JPY-Preis mit Präzision 0, Jahreszähler und Client-Allowlist.

## Noch offene Kriterien / bekannte Grenzen

- Nur die Erstinstallation (Schema v0 → v1) ist implementiert. Upgradepfade von älteren produktiv genutzten Schemata, absichtlich fehlschlagende Migration, Rollback und Wiederherstellung aus einem Migrationssnapshot sind nicht getestet.
- Keine echten parallelen Mehrverbindungs-Tests für Commissionnummern oder konkurrierende Änderungen; die Repository-Connection ist innerhalb eines Prozesses mutex-geschützt. Atomare `IMMEDIATE`-Transaktionen und UNIQUE-/Revision-Constraints sind umgesetzt, aber das geforderte Paralleltestkriterium bleibt offen.
- Das Commission-Erstellungsformular nimmt nur einen Teil der Felder an. Update/Delete sind als Tauri-Kommandos und Core-Services verfügbar, aber nicht in Desktop- oder CLI-Bedienabläufen verdrahtet. Daher ist das Akzeptanzkriterium „jedes Metadatum erstellen, ändern, lesen“ noch nicht erfüllt.
- In SQLite werden einige strukturierte Felder zusätzlich zum JSON-Payload relational materialisiert; der Payload ist derzeit die Roundtrip-Quelle des Commission-Repositorys. Fremdschlüssel und Kernfelder sind relational abgesichert. Ein Migrationstest für Payload-/Relationalkonsistenz fehlt.
- Commission-Änderungen enthalten die vollständige Commission in Revision/Outbox-Payloads. Das Outbox-Format ist lokal; es gibt keinen Netzwerkversand.
- SQLite ist nicht verschlüsselt. Keine Secrets werden in der DB vorgesehen; ein OS-Schlüsselspeicher ist noch nicht umgesetzt.
- Sichtbares Tauri-Desktop-/E2E-Testing wurde nicht ausgeführt. Es wurde kein echter Upload, Roblox-Account oder Netzwerkadapter gestartet.
- DTO-Spiegelung in TypeScript ist weiterhin manuell; generierte Drift-Prüfung ist offen.

## Übergabe

Die nächste Arbeit in dieser Phase sollte zuerst End-to-End-Bedienung für die vollständigen Commission-Metadaten (inklusive Update/Delete mit Konfliktanzeige) ergänzen, danach Migration-/Paralleltests und einen sichtbaren Desktop-Smoke ausführen. Phase 3 darf `.apexstd`-Import/Export separat umsetzen; die Schema-Grundtabellen hier gelten nicht als fertige Datei-/Uploadfeatures.
