# CLI JSON and shared-service contract

## Versioning

This document defines the first MCP-facing CLI contract (`v1`). MCP adapters should call the public APIs in `apex-application` and `apex-core`; they must not parse TUI text or shell output. `ApplicationServices` is the composition root. Storage and validation rules remain in the shared crates, while desktop, CLI, and future MCP transports remain adapters.

The command line is still an executable interface, not a remote API. The global `--json` flag selects machine-readable output for commands that support it. Success output is command-specific JSON on stdout; errors are written to stderr and return a non-zero exit code. This release does not wrap every success value in a new envelope, so consumers should use the command-specific shapes below rather than assuming one generic success schema.

## Error response

When `--json` is set, errors use this stable envelope:

```json
{"ok":false,"error":{"code":"not_found","message":"The project or file no longer exists. Refresh and try again."}}
```

`error.code` is the machine-readable field. `error.message` is display text and may change. Consumers should branch on the code, not message wording. Codes are lowercase `snake_case`. Core codes are defined by `apex_core::Error::code`; command-specific codes are defined at the point where the CLI validates inputs and integrations. New codes may be added without changing the envelope.

The envelope schema is in [`schemas/v1/error-envelope.schema.json`](schemas/v1/error-envelope.schema.json).

## Command output conventions

- `projects list --json`: JSON array of [`Project` values](schemas/v1/project.schema.json).
- `projects show --json`: one `Project` value using the same schema.
- `workspace --json`: [`WorkspaceInfo`](schemas/v1/workspace.schema.json) with `name`, `root`, `schemaVersion`, and `managedFileCount`.
- `notes list/show/create/edit/archive/restore`: note arrays or one [`Note`](schemas/v1/note.schema.json).
- `assets list`: [`AssetBoard`](schemas/v1/asset-board.schema.json); import returns a typed import report.
- `posts list`: post-summary array; `posts show/save`: [`PublishingDraft`](schemas/v1/publishing-draft.schema.json). Create/import return an object with `id` and `draft`.
- `roblox queue`: [`QueueSnapshot`](schemas/v1/roblox-queue.schema.json).
- `statistics history`: array of [`StatisticsSnapshot`](schemas/v1/statistics-snapshot.schema.json).
- `trash list`, `revenue groups`, and `sync status/projects/members/bindings`: serialized typed result or command report; arrays remain arrays, including when empty. The application service types are the authoritative integration contract; these command-specific JSON forms remain CLI transport shapes.
- Mutating commands return their resulting shared model or a small operation object. Commands requiring confirmation require `--yes` in JSON mode and never prompt on stdin.
- Secrets are never included in JSON output. Passwords and API keys are not accepted as command-line values. Credentials remain in the operating system credential store.
- `completions <shell>` writes the completion script to stdout and does not open or migrate a workspace.

Types shared with the desktop are Rust types in `apex-core` and `apex-application`, serialized with the field naming declared on each type. Most application-facing records use camelCase. For a durable integration, use the typed application service rather than treating undocumented output from an individual subcommand as a stable RPC method.

## Shared application service surface

`ApplicationServices` in `crates/apex-application` is the supported in-process boundary. Its typed methods cover workspace/project/notes/assets/trash, packages and backups, Roblox uploads and credentials, ERLCX posts and handoff, statistics/revenue, and Team Sync. Long-running upload/sync work is represented by durable service state; adapters must request state and control through those owners instead of duplicating their state machines.

An MCP server should expose narrow tools that call these services, validate arguments into the existing input types, and return typed results. It should apply the same explicit confirmation policy for destructive actions, uploads, sync, and browser handoff. It must never expose stored credentials, arbitrary workspace file access, or terminal-rendered strings as a substitute for a service result.

## Shell completions

Generate a script with `apex-studio-cli completions powershell` (or `bash`, `zsh`, `fish`, or `elvish`). To load PowerShell completions for the current session:

```powershell
apex-studio-cli completions powershell | Out-String | Invoke-Expression
```

To make that persistent, add the same pipeline to the PowerShell profile. The CLI does not edit shell profiles automatically.
