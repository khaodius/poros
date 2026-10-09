import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { App } from "./App";
import { DragPreview } from "./components/DragPreview";
import { DRAG_PREVIEW_WINDOW } from "./lib/ipc";
import { applyRememberedTheme } from "./state/themeStore";
import "./styles/tokens.css";
import "./styles/app.css";

// Windows open hidden until the first render; this makes that render match the app's look.
applyRememberedTheme();

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    {getCurrentWindow().label === DRAG_PREVIEW_WINDOW ? <DragPreview /> : <App />}
  </StrictMode>,
);
