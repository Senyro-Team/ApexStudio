// Mirrors the serialized DTOs in crates/apex-domain. Update both as a versioned contract.
export interface ProjectDto {
  id: string;
  name: string;
  description: string | null;
  tags: string[];
  created_at: string;
  updated_at: string;
  archived_at: string | null;
  revision: number;
}

export interface AppErrorDto {
  code: string;
  message: string;
  retryable: boolean;
  details: string | null;
}

export interface CreateProjectInput {
  name: string;
  description?: string | null;
  tags?: string[];
}

export interface UpdateProjectInput {
  id: string;
  expected_revision: number;
  name: string;
  description: string | null;
  tags: string[];
}

export interface ArchiveProjectInput {
  id: string;
  expected_revision: number;
}

export interface CreateCommissionInput {
  project_id: string;
  name: string;
  client_name: string | null;
  organisation_name: string | null;
  commission_type: string | null;
  description: string | null;
  requirements: string[];
  due_date: string | null;
}

export interface CommissionDto {
  id: string;
  project_id: string;
  display_id: string;
  name: string;
  created_at: string;
  due_date: string | null;
  status: string;
  priority: string | null;
  revision_count: number;
  revision: number;
}

export interface CommandEnvelope<T> {
  version: number;
  request_id: string;
  payload: T;
}
