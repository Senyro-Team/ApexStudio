import { useEffect, useState } from "react";
import AppSelect from "./AppSelect";
const sides = [
  "Left",
  "Right",
  "Front",
  "Frontplate",
  "Back",
  "Top",
  "Window Left",
  "Window Right",
  "Window Front",
  "Window Back",
  "Window Top",
];
export default function SlotEditor({
  label,
  value,
  disabled,
  onChange,
}: {
  label: string;
  value: string | null;
  disabled: boolean;
  onChange: (value: string | null) => void;
}) {
  const split = value?.lastIndexOf(" ") ?? -1;
  const side = value?.slice(0, split) ?? "";
  const number = value?.slice(split + 1) ?? "1";
  const [editingNumber, setEditingNumber] = useState(number);
  useEffect(() => setEditingNumber(number), [number, value]);
  return (
    <div className="slot-editor">
      <AppSelect
        aria-label={`Side for ${label}`}
        value={side}
        disabled={disabled}
        onChange={(e) =>
          onChange(
            e.target.value ? `${e.target.value} ${editingNumber || "1"}` : null,
          )
        }
      >
        <option value="">Unassigned</option>
        {sides.map((s) => (
          <option key={s}>{s}</option>
        ))}
      </AppSelect>
      <input
        aria-label={`Number for ${label}`}
        type="number"
        min={1}
        max={100}
        value={value ? editingNumber : ""}
        disabled={disabled || !value}
        onChange={(e) => setEditingNumber(e.target.value)}
        onBlur={() => {
          if (value && editingNumber !== number)
            onChange(`${side} ${editingNumber}`);
        }}
        onKeyDown={(e) => {
          if (e.key === "Enter") {
            e.preventDefault();
            e.currentTarget.blur();
          }
        }}
      />
    </div>
  );
}
