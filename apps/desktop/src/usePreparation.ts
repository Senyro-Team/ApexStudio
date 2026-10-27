import { useEffect, useState } from "react";
import { liveryPreparations, type PreparationProgress } from "./bridge";
export default function usePreparation(tokens: string[]) {
  const key = tokens.join(",");
  const [state, setState] = useState<Record<string, PreparationProgress>>({});
  useEffect(() => {
    let active = true;
    const ids = key.split(",").filter(Boolean);
    if (!ids.length) return;
    const refresh = async () => {
      try {
        const rows = await liveryPreparations(ids);
        if (active)
          setState((old) => {
            const next = Object.fromEntries(
              rows.map((r) => [r.token, r.progress]),
            );
            return JSON.stringify(old) === JSON.stringify(next) ? old : next;
          });
      } catch {
        /* Older browser previews have no native staging. */
      }
    };
    void refresh();
    const timer = setInterval(() => void refresh(), 500);
    return () => {
      active = false;
      clearInterval(timer);
    };
  }, [key]);
  return state;
}
