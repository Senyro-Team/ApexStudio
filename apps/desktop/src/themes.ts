export type ThemeMode = "tinted" | "black";

export interface ThemeColors {
  button: string;
  line: string;
  text: string;
  logo: string;
}

export interface ThemeCustomization {
  mode: ThemeMode;
  colors: ThemeColors;
}

export interface Theme {
  id: string;
  name: string;
  color: string;
  mode: ThemeMode;
  colors?: ThemeColors;
}

export const themes: Theme[] = [
  { id: "violet", name: "Violet", color: "#8c91ff", mode: "tinted" },
  { id: "ocean", name: "Ocean", color: "#63b9ff", mode: "tinted" },
  { id: "forest", name: "Forest", color: "#64d9af", mode: "tinted" },
  { id: "sunset", name: "Sunset", color: "#ffba69", mode: "tinted" },
  { id: "black-blue", name: "Black Blue", color: "#63b9ff", mode: "black" },
  { id: "black-red", name: "Black Red", color: "#ff7285", mode: "black" },
  { id: "black-gold", name: "Black Gold", color: "#f8c56d", mode: "black" },
];

const selectedKey = "apex-studio-theme";
const customKey = "apex-studio-custom-themes";
const overridesKey = "apex-studio-theme-overrides";
const validColor = /^#[0-9a-f]{6}$/i;

function validColors(value: unknown): value is ThemeColors {
  if (!value || typeof value !== "object") return false;
  const colors = value as Record<string, unknown>;
  return ["button", "line", "text", "logo"].every(
    (key) => typeof colors[key] === "string" && validColor.test(colors[key]),
  );
}

function mix(first: string, second: string, ratio: number) {
  const channels = [1, 3, 5].map((start) =>
    Math.round(
      parseInt(first.slice(start, start + 2), 16) * ratio +
        parseInt(second.slice(start, start + 2), 16) * (1 - ratio),
    ),
  );
  return `#${channels.map((channel) => channel.toString(16).padStart(2, "0")).join("")}`;
}

export function defaultColors(theme: Theme): ThemeColors {
  const presetButtons: Record<string, string> = {
    violet: "#525ce0",
    ocean: "#237cc0",
    forest: "#23845f",
    sunset: "#ac681d",
  };
  const presetLines: Record<string, string> = {
    violet: "#2a2c33",
    ocean: "#2a3a46",
    forest: "#2a4036",
    sunset: "#49372a",
  };
  return {
    button: presetButtons[theme.id] ?? mix(theme.color, "#000000", 0.58),
    line:
      presetLines[theme.id] ??
      (theme.mode === "black"
        ? mix(theme.color, "#303030", 0.45)
        : mix(theme.color, "#292b30", 0.23)),
    text: "#f2f2f5",
    logo: theme.color,
  };
}

export function themeColors(theme: Theme): ThemeColors {
  return theme.colors ?? defaultColors(theme);
}

export function loadCustomThemes(): Theme[] {
  try {
    const value: unknown = JSON.parse(localStorage.getItem(customKey) ?? "[]");
    if (!Array.isArray(value)) return [];
    return value.filter(
      (item): item is Theme =>
        !!item &&
        typeof item.id === "string" &&
        item.id.startsWith("custom-") &&
        typeof item.name === "string" &&
        item.name.trim().length > 0 &&
        item.name.length <= 40 &&
        typeof item.color === "string" &&
        validColor.test(item.color) &&
        (item.mode === "tinted" || item.mode === "black") &&
        (item.colors === undefined || validColors(item.colors)),
    );
  } catch {
    return [];
  }
}

export function saveCustomThemes(customThemes: Theme[]) {
  try {
    localStorage.setItem(customKey, JSON.stringify(customThemes));
  } catch {
    // Custom themes remain available for this session.
  }
}

export function loadThemeOverrides(): Record<string, ThemeCustomization> {
  try {
    const value: unknown = JSON.parse(
      localStorage.getItem(overridesKey) ?? "{}",
    );
    if (!value || typeof value !== "object" || Array.isArray(value)) return {};
    return Object.fromEntries(
      Object.entries(value).filter(
        ([id, entry]) =>
          themes.some((theme) => theme.id === id) &&
          !!entry &&
          typeof entry === "object" &&
          ["tinted", "black"].includes((entry as ThemeCustomization).mode) &&
          validColors((entry as ThemeCustomization).colors),
      ),
    );
  } catch {
    return {};
  }
}

export function saveThemeOverrides(
  overrides: Record<string, ThemeCustomization>,
) {
  try {
    localStorage.setItem(overridesKey, JSON.stringify(overrides));
  } catch {
    // Theme edits remain available for this session.
  }
}

export function savedTheme(customThemes: Theme[]): string {
  try {
    const id = localStorage.getItem(selectedKey);
    return (
      [...themes, ...customThemes].find((theme) => theme.id === id)?.id ??
      "violet"
    );
  } catch {
    return "violet";
  }
}

export function saveTheme(id: string) {
  try {
    localStorage.setItem(selectedKey, id);
  } catch {
    // The theme still applies for this session when storage is unavailable.
  }
}

export function applyTheme(theme: Theme, customization: ThemeCustomization) {
  const root = document.documentElement;
  root.dataset.theme = theme.id;
  root.dataset.themeMode = customization.mode;
  const properties = [
    "--app-background",
    "--surface",
    "--surface-raised",
    "--line",
    "--border",
    "--accent",
    "--accent-hover",
    "--accent-strong",
    "--accent-strong-hover",
    "--accent-selected",
    "--button-text",
    "--focus",
    "--selection",
    "--text",
    "--muted",
  ];
  properties.forEach((property) => root.style.removeProperty(property));
  const { button, line, text } = customization.colors;
  const luminance = [1, 3, 5]
    .map((start) => parseInt(button.slice(start, start + 2), 16))
    .map((channel) => {
      const value = channel / 255;
      return value <= 0.04045
        ? value / 12.92
        : ((value + 0.055) / 1.055) ** 2.4;
    });
  const lightness =
    luminance[0] * 0.2126 + luminance[1] * 0.7152 + luminance[2] * 0.0722;
  root.style.setProperty("--accent", button);
  root.style.setProperty("--accent-strong", button);
  root.style.setProperty(
    "--button-text",
    lightness > 0.18 ? "#111111" : "#ffffff",
  );
  root.style.setProperty("--line", line);
  root.style.setProperty("--border", line);
  root.style.setProperty("--text", text);
  root.style.setProperty(
    "--muted",
    `color-mix(in srgb, ${text} 70%, var(--app-background))`,
  );
  root.style.setProperty(
    "--accent-hover",
    `color-mix(in srgb, ${button} 72%, white)`,
  );
  root.style.setProperty(
    "--accent-strong-hover",
    `color-mix(in srgb, ${button} 82%, white)`,
  );
  root.style.setProperty("--focus", button);
  root.style.setProperty(
    "--selection",
    `color-mix(in srgb, ${button} 42%, black)`,
  );
  if (customization.mode === "black") return;
  root.style.setProperty(
    "--accent-selected",
    `color-mix(in srgb, ${button} 24%, #25272d)`,
  );
  if (!theme.id.startsWith("custom-")) return;

  root.style.setProperty(
    "--app-background",
    `color-mix(in srgb, ${button} 8%, #111315)`,
  );
  root.style.setProperty(
    "--surface",
    `color-mix(in srgb, ${button} 11%, #191b1e)`,
  );
  root.style.setProperty(
    "--surface-raised",
    `color-mix(in srgb, ${button} 14%, #202227)`,
  );
}

export function themeFavicon(color: string): Promise<string> {
  return new Promise((resolve, reject) => {
    const image = new Image();
    image.onload = () => {
      const canvas = document.createElement("canvas");
      canvas.width = 128;
      canvas.height = 128;
      const context = canvas.getContext("2d");
      if (!context) return reject(new Error("Canvas unavailable"));
      context.drawImage(image, 0, 0, 128, 128);
      context.globalCompositeOperation = "source-in";
      context.fillStyle = color;
      context.fillRect(0, 0, 128, 128);
      resolve(canvas.toDataURL("image/png"));
    };
    image.onerror = () => reject(new Error("Logo unavailable"));
    image.src = "/brand/apex.png";
  });
}
