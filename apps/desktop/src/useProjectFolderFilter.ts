import { useEffect, useMemo, useState } from "react";
import { listProjectFolderOptions, type AssetFolderOption } from "./bridge";

export function projectFolderLabels(folders: AssetFolderOption[]) {
  const byId = new Map(folders.map((folder) => [folder.id, folder]));
  const labelFor = (id: string) => {
    const labels: string[] = [];
    const visited = new Set<string>();
    let current = byId.get(id);
    while (current && !visited.has(current.id)) {
      visited.add(current.id);
      labels.push(current.name);
      current = current.parentId ? byId.get(current.parentId) : undefined;
    }
    return labels.reverse().join(" / ");
  };
  return folders
    .map((folder) => ({ id: folder.id, label: labelFor(folder.id) }))
    .sort((a, b) => a.label.localeCompare(b.label, undefined, { sensitivity: "base" }));
}

export function descendantFolderIds(folderId: string, folders: AssetFolderOption[]) {
  const children = new Map<string, string[]>();
  for (const folder of folders) {
    if (!folder.parentId) continue;
    const siblings = children.get(folder.parentId) ?? [];
    siblings.push(folder.id);
    children.set(folder.parentId, siblings);
  }
  const descendants = new Set<string>();
  const pending = [folderId];
  while (pending.length) {
    const current = pending.pop()!;
    if (descendants.has(current)) continue;
    descendants.add(current);
    pending.push(...(children.get(current) ?? []));
  }
  return descendants;
}

export default function useProjectFolderFilter(projectId: string) {
  const [folderId, setFolderId] = useState("");
  const [folders, setFolders] = useState<AssetFolderOption[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState("");
  const [attempt, setAttempt] = useState(0);

  useEffect(() => {
    let active = true;
    setFolderId("");
    setFolders([]);
    setError("");
    if (!projectId) {
      setLoading(false);
      return () => {
        active = false;
      };
    }
    setLoading(true);
    void listProjectFolderOptions(projectId)
      .then((items) => {
        if (!active) return;
        setFolders(items);
        setFolderId((current) => items.some((folder) => folder.id === current) ? current : "");
      })
      .catch((cause) => {
        if (active) setError(cause instanceof Error ? cause.message : String(cause));
      })
      .finally(() => {
        if (active) setLoading(false);
      });
    return () => {
      active = false;
    };
  }, [projectId, attempt]);

  const options = useMemo(() => projectFolderLabels(folders), [folders]);
  const matchingFolderIds = useMemo(
    () => folderId ? descendantFolderIds(folderId, folders) : null,
    [folderId, folders],
  );

  return {
    folderId,
    setFolderId,
    options,
    matchingFolderIds,
    loading,
    error,
    retry: () => setAttempt((value) => value + 1),
  };
}
