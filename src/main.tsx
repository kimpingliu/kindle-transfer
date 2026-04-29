/**
 * Frontend application entrypoint for the Kindle desktop tool.
 *
 * The Tauri shell will load this bundle. The React tree is intentionally small
 * and self-contained so backend `invoke` hooks can be added later without
 * refactoring the UI structure.
 */

import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import "./index.css";

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
