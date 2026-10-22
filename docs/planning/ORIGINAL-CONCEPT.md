# Apex Studio — Product Concept & Development Roadmap

Open Source

Desktop Application · CLI/TUI · Local-First · Modular Architecture

## 1. Product Overview

Apex Studio is a free, open-source creative production and asset management application designed for livery creators, designers, independent developers, and design studios.

The goal is to provide a unified workspace for managing commissions, organizing projects, uploading assets to Roblox, documenting creative work, and eventually analyzing marketplace performance and collaborating with other designers.

The application will be developed under the Apex Designs branding while remaining an independently maintained open-source project.

### Core Principles

- English Only: The entire application, including the desktop interface, installer, CLI/TUI, notifications, errors, settings, dialogs, documentation, and default templates, must use English.
- Local-First: All essential features work offline, except services that inherently require internet access.
- No Mandatory Accounts: Users do not need an Apex Studio account.
- Open Source: Source code is publicly available, with appropriate licensing for the code and separate terms for branded assets.
- Modular: Features are implemented as independent modules sharing a common application core.
- Cross-Platform Ready: Windows is the initial target, with macOS and Linux compatibility considered in architectural decisions.
- Extensible: Future features such as real-time team synchronization, ERLCX publishing, and AI analytics must be possible without rebuilding the application.

## 2. Application Modules

Dashboard & Analytics

A unified overview showing active projects, commissions, deadlines, asset uploads, recent activity, and future marketplace performance metrics.

Project & Commission Management

Full project lifecycle management, including customer information, assignments, design requirements, progress tracking, revisions, licensing, payment records, and deadlines.

Roblox Asset Uploader

Automatic template recognition, filename parsing, image validation, Roblox Open Cloud uploads, asset ID collection, and detailed upload logging.

Asset & Code Manager

Structured storage of Image IDs, Decal IDs, vehicle templates, asset versions, original files, hashes, and CSV import/export mappings.

Markdown Notes

An Obsidian-inspired notes editor supporting tables, headings, checklists, hyperlinks, linked images, embedded external images, and local attachments.

Command Line Interface

A scriptable CLI and an interactive TUI featuring keyboard navigation, menus, confirmations, upload progress, and settings.

Team Synchronization — Future

An optional, self-hostable synchronization service with shared projects, access permissions, change tracking, and near-real-time updates.

## 3. Recommended Technology Stack

| Component           | Technology                           |
| ------------------- | ------------------------------------ |
| Desktop Application | Tauri 2                              |
| Frontend            | React + TypeScript                   |
| Styling             | Tailwind CSS + shadcn/ui             |
| Application Core    | Rust                                 |
| Local Database      | SQLite                               |
| CLI                 | Rust + Clap                          |
| Interactive TUI     | Ratatui                              |
| Project Packages    | Versioned ZIP-based `.apexstd`       |
| Browser Extension   | TypeScript + WebExtensions           |
| Future Sync Server  | Rust backend, PostgreSQL, WebSockets |
| Future AI           | OpenAI API integration, opt-in       |

Desktop and CLI functionality must share the same Rust core. Business logic must not be duplicated between the interfaces.

## 4. Commission & Project Data Model

A commission represents a customer order, while a project represents an actual creative deliverable.

A commission can contain multiple projects, and projects can exist independently of commissions.

The system must support the following metadata.

Commission Details

Commission ID (`COM-2026-0001`), internal UUIDv7, Commission Name, Client, Organisation, Commission Type, Description, Requirements, Reference Materials, Created At, and Due Date.

Workflow & Management

Status, Priority, Assigned Designer, Started At, Completed At, Delivered At, Revision Count, and Internal Notes.

Asset & Design Data

Design Type, Vehicle Model, Template, Figma File URL, Export Files, Roblox Asset IDs, File Hashes, and Version.

Payment & Licensing

Total Price, Currency, Payment Status, Payment Method, Usage Rights, Exclusivity, and License Notes.

Asset data and internal notes must be private by default. Financial information must also remain private unless explicitly selected for a controlled export or authorized team sharing.

UUIDv7 identifiers will serve as immutable technical identities, while readable commission numbers will be configurable per studio or workspace.

## 5. Roblox Upload Workflow

Import PNG Templates

Select images exported from the design application

Recognize Vehicle Sides

Parse filenames, aliases and numbered slots

Validate Files

Check PNG integrity, dimensions and file size

Upload to Roblox

Use supported Roblox Open Cloud authentication

Collect Asset IDs

Track Image IDs and Decal IDs separately

Record Upload Evidence

Hashes, timestamps, Roblox IDs and outcomes

### Supported Template Sides

`Left`, `Right`, `Front`, `Back`, `Rear`, `Top`, `WindowLeft`, `WindowRight`, `WindowBack`, `WindowTop`, and `FrontPlate`.

Filename recognition must support aliases, different separators, numbering schemes, and user-defined parsing rules. Ambiguous matches require user confirmation.

### Upload Requirements

- PNG image validation.
- A configurable file-size limit, initially 20 MB.
- Dimension validation against the applicable Roblox API constraints. The exact 8000-pixel boundary must be verified rather than assumed.
- Uploaded asset names using `APXSTD-{HASH}`.
- Support for multiple Roblox accounts where authorized.
- Upload queues, retry handling, operation polling, and error reporting.
- Both Image IDs and Decal IDs must be supported, with a preferred type configurable in Settings.
- Unavailable IDs must never be fabricated or inferred from another ID.

### Upload Logging

Each upload record must include SHA-256 file hash, normalized pixel SHA-256, perceptual hash (pHash), Roblox Asset ID, asset type, upload timestamp, upload status, and associated project.

These records support traceability and matching. Hashes alone do not constitute conclusive proof of copyright ownership or creation date.

## 6. Portable Project Format

Recommended extension: `.apexstd`

The format will be a versioned, portable archive containing project metadata, commission references, notes, vehicle mappings, asset IDs, file hashes, and optionally embedded files.

Two export modes should be supported: Full Project and Metadata Only.

Exports must exclude credentials and secrets, and require explicit selection before including private commission, client, or payment information. Imports must validate manifests, archive paths, schema versions, and project identity conflicts.

# Development Roadmap

The following is the recommended order for implementing Apex Studio. Each phase should build on the previous phases without replacing completed systems.

01

Foundation & Architecture

Establish the Rust workspace, Tauri desktop application, React frontend, SQLite infrastructure, service boundaries, configuration system, error handling, CI, testing framework, and project documentation.

Deliverable: Compilable application foundation and working shared core.

02

Branding, Design System & Desktop UI

Integrate the Apex Designs branding assets, define light/dark-capable design tokens with a dark-first experience, build reusable UI components, navigation, settings, empty states, dialogs, and responsive application layouts.

Deliverable: Consistent Apex-branded desktop interface with English-only UI text.

03

Project & Commission Management

Implement project and commission entities, UUIDv7 generation, readable commission IDs, all specified metadata, clients and organizations, workflow states, assignments, revisions, search, filtering, and local persistence.

Deliverable: Functional project and commission management with full metadata support.

04

Markdown Notes & Project File Format

Build the Markdown editor with live preview, tables, hyperlinks, images, attachments, checklists, and internal links. Implement versioned `.apexstd` import/export with privacy controls and validation.

Deliverable: Documented, portable project packages and a functional notes workspace.

05

Asset Library & Code Management

Implement vehicles, template sides, numbered slots, aliases, Image/Decal ID models, preferred ID settings, CSV import/export, asset references, asset history, and duplicate detection.

Deliverable: Complete local asset library independent of live Roblox uploads.

06

Roblox Upload Engine

Develop the Roblox Open Cloud integration from scratch, including image validation, filename classification, account configuration, hashing, asset naming, queue management, operation tracking, result mapping, and audit logging.

Deliverable: End-to-end PNG-to-Roblox upload workflow integrated into projects.

07

CLI & Interactive TUI

Build the Clap command structure and Ratatui interface with arrow-key navigation, Enter/Escape actions, menus, configuration editing, upload status, structured JSON output, and non-interactive automation support.

Deliverable: Independent terminal application using the same core as the desktop app.

08

Installer, Distribution & Open-Source Release

Create a customized Windows NSIS setup wizard with optional components, desktop shortcuts, CLI PATH registration, `.apexstd` file associations, uninstall support, build signing strategy, release automation, license notices, and support links.

Deliverable: Installable and distributable Apex Studio release.

09

Analytics & ERLCX Statistics

Implement a read-only analytics architecture for publicly accessible or authorized ERLCX data, including view/copy histories, trend charts, local metric snapshots, synchronization scheduling, and graceful connector failure handling.

Deliverable: Analytics dashboards, subject to validated ERLCX access permissions.

10

Optional Team Synchronization

Implement a separately deployable sync service, workspace membership, scoped permissions, WebSocket change notifications, secure transport, offline reconciliation, conflict resolution, and shared project updates.

Deliverable: Optional multi-user collaboration without requiring an Apex account for offline use.

11

AI-Powered Analytics & Insights

Add optional OpenAI API configuration with user-provided credentials, data minimization, explainable suggestions, statistical analysis, performance comparisons, and explicitly labeled uncertainty.

Deliverable: Opt-in AI recommendations based on available marketplace data.

12

ERLCX Publisher & Browser Extension

Prepare an optional publishing workflow with post metadata, vehicle codes, CSV mappings, validation, and browser form assistance. Implement the actual automated submission only after the permitted integration methods have been clarified with ERLCX.

Status: On hold pending ERLCX approval or clarification.

## 7. Mandatory Language Policy

This policy must be included in the specification and implementation prompts for every development phase.

Application Language: English (en-US)

Required

All user-facing content must be written in natural, consistent English:

- Desktop navigation, buttons, forms, tooltips, modals, status indicators, and error messages.
- Installer wizard, component descriptions, license notices, and uninstall flows.
- CLI/TUI prompts, help output, progress messages, confirmations, and errors.
- Default project templates, commission field names, Markdown templates, notifications, documentation, and release notes.

Implementation Rules

Use English for source-code identifiers, comments, documentation, API contracts, database field naming, and test descriptions. Use ISO 8601 for stored dates and UTC for timestamps, with localized display as appropriate.

No German UI strings, German placeholder text, German command descriptions, or mixed-language interfaces.

Users may still enter their own project content in any language.

## 8. Global Development Requirements

Every phase must preserve the project's existing architecture, maintain compatibility with previously implemented features, add appropriate unit and integration tests, update documentation, and avoid implementing external integrations using unsupported or unapproved API behavior.

The following security and product constraints are permanent: no embedded credentials, no mandatory Apex account, private-by-default customer data, no silent third-party data transfers, explicit confirmation for destructive operations, and no unverified conversion between Roblox Image IDs and Decal IDs.

Additionally, feature flags or modular boundaries should preserve support for later team synchronization and AI functionality, without building their full infrastructure prematurely.

## 9. Suggested Release Milestones

| Milestone               | Phases            | Target                                              |
| ----------------------- | ----------------- | --------------------------------------------------- |
| Apex Studio Alpha       | 1–4               | Local project management and portable project files |
| Apex Studio Beta        | 5–7               | Functional Roblox workflow and complete CLI/TUI     |
| Apex Studio 1.0         | 8 + stabilization | Public installer and open-source release            |
| Apex Studio 1.x         | 9                 | Analytics and marketplace insights                  |
| Apex Studio 2.x         | 10–11             | Optional team synchronization and AI                |
| Conditional Integration | 12                | ERLCX Publisher when permitted                      |

These are proposed milestones, not release commitments.

Final project definition: Apex Studio is an English-only, open-source, local-first creative production platform combining project and commission management, Roblox asset workflows, documentation, portable project files, and professional tooling. Its extended ecosystem can later include synchronization, marketplace publishing, and AI-assisted analytics.

All phase-specific AI implementation prompts should be written in English and enforce the English-only application requirement, including any work already planned for the prompt package.