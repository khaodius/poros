import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { App } from "./App";
import { applyRememberedTheme } from "./state/themeStore";
import "./styles/tokens.css";
import "./styles/app.css";

// Windows open hidden until the first render; this makes that render match the app's look.
applyRememberedTheme();

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <App />
  </StrictMode>,
);
