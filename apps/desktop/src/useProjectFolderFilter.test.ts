import { expect, it } from "vitest";
import { descendantFolderIds, projectFolderLabels } from "./useProjectFolderFilter";

const folders = [
  { id: "root", parentId: null, name: "Liveries" },
  { id: "ems", parentId: "root", name: "EMS" },
  { id: "special", parentId: "ems", name: "Special" },
  { id: "patrol", parentId: "root", name: "Patrol" },
];

it("creates complete folder labels and includes descendants in a folder filter", () => {
  expect(projectFolderLabels(folders)).toEqual([
    { id: "root", label: "Liveries" },
    { id: "ems", label: "Liveries / EMS" },
    { id: "special", label: "Liveries / EMS / Special" },
    { id: "patrol", label: "Liveries / Patrol" },
  ]);
  expect([...descendantFolderIds("ems", folders)].sort()).toEqual(["ems", "special"]);
});

it("keeps folder labels finite if corrupted data contains a parent cycle", () => {
  expect(projectFolderLabels([
    { id: "a", parentId: "b", name: "A" },
    { id: "b", parentId: "a", name: "B" },
  ])).toHaveLength(2);
});
