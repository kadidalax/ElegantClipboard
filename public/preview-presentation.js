/** Shared preview window theme / color / corner sync (text + image preview). */
function applyPreviewPresentation(root, payload) {
  if (!root || !payload) return;

  const theme = payload.theme === "dark" ? "dark" : "light";
  root.classList.toggle("dark", theme === "dark");
  root.dataset.theme = theme;

  root.dataset.sharpCorners = payload.sharpCorners ? "true" : "false";

  root.classList.remove("theme-emerald", "theme-cyan", "theme-system");
  const colorTheme = payload.colorTheme;
  if (colorTheme && colorTheme !== "default") {
    root.classList.add(`theme-${colorTheme}`);
  }

  root.style.removeProperty("--system-accent-h");
  root.style.removeProperty("--system-accent-s");
  root.style.removeProperty("--system-accent-l");
  if (colorTheme === "system" && payload.systemAccent) {
    const parts = String(payload.systemAccent).split(" ");
    root.style.setProperty("--system-accent-h", parts[0] || "210");
    root.style.setProperty("--system-accent-s", parts[1] || "65%");
    root.style.setProperty("--system-accent-l", parts[2] || "50%");
  }

  const effect = payload.windowEffect || "none";
  if (effect === "none") {
    root.removeAttribute("data-window-effect");
  } else {
    root.setAttribute("data-window-effect", effect);
  }

  if (payload.uiFontFamily) {
    root.style.setProperty("--ui-font-family", payload.uiFontFamily);
  } else {
    root.style.removeProperty("--ui-font-family");
  }
}
