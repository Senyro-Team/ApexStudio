import { afterEach, expect, it, vi } from "vitest";
import { useState } from "react";
import { cleanup, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import AppSelect from "./AppSelect";

afterEach(cleanup);

it("opens a styled menu and changes the native select value", async () => {
  const user = userEvent.setup();
  const onChange = vi.fn();
  function Selection() {
    const [value, setValue] = useState("");
    return (
      <label>
        Project
        <AppSelect value={value} onChange={(event) => {
          onChange(event);
          setValue(event.target.value);
        }}>
          <option value="">All projects</option>
          <option value="p1">Metro project</option>
        </AppSelect>
      </label>
    );
  }
  render(<Selection />);
  const select = screen.getByLabelText("Project") as HTMLSelectElement;
  await user.click(select);
  expect(screen.getByRole("listbox")).toBeTruthy();
  await user.click(within(screen.getByRole("listbox")).getByRole("option", { name: "Metro project" }));
  expect(select.value).toBe("p1");
  expect(onChange).toHaveBeenCalledTimes(1);
  expect(screen.queryByRole("listbox")).toBeNull();
});

it("supports keyboard selection and closes with Escape", async () => {
  const user = userEvent.setup();
  render(
    <AppSelect aria-label="Metric" defaultValue="views">
      <option value="views">Views</option>
      <option value="likes">Likes</option>
    </AppSelect>,
  );
  const select = screen.getByLabelText("Metric") as HTMLSelectElement;
  select.focus();
  await user.keyboard("{ArrowDown}{Enter}");
  expect(select.value).toBe("likes");
  await user.keyboard("{Enter}{Escape}");
  expect(screen.queryByRole("listbox")).toBeNull();
});
