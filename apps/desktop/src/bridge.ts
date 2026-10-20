import { invoke } from "@tauri-apps/api/core";
import type { AppErrorDto, ArchiveProjectInput, CommissionDto, CreateCommissionInput, CreateProjectInput, ProjectDto, UpdateProjectInput } from "./types";

export class BridgeError extends Error {
  constructor(readonly contract: AppErrorDto) {
    super(contract.message);
    this.name = "BridgeError";
  }
}

async function call<T>(command: string, payload?: unknown, wrapInput = true): Promise<T> {
  try {
    const args = payload === undefined ? undefined : wrapInput ? { input: payload } : payload as Record<string, unknown>;
    return await invoke<T>(command, args);
  } catch (reason: unknown) {
    if (typeof reason === "object" && reason !== null && "code" in reason && "message" in reason) {
      throw new BridgeError(reason as AppErrorDto);
    }
    throw new BridgeError({ code: "bridge.unavailable", message: "Die Desktop-Verbindung ist nicht verfügbar.", retryable: false, details: null });
  }
}

export const projectBridge = {
  create: (input: CreateProjectInput) => call<ProjectDto>("create_project", input),
  list: () => call<ProjectDto[]>("list_projects"),
  update: (input: UpdateProjectInput) => call<ProjectDto>("update_project", input),
  archive: (input: ArchiveProjectInput) => call<ProjectDto>("archive_project", input),
  listCommissions: (projectId: string) => call<CommissionDto[]>("list_commissions", { projectId }, false),
  createCommission: (input: CreateCommissionInput) => call<CommissionDto>("create_commission", input),
};
