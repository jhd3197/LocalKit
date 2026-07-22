import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { applyTheme, getTheme } from "./stores/settings";
import "./index.css";

// Stamp the theme before first render — the settings store seeds
// synchronously (injection / localStorage mirror), so there's no flash.
applyTheme(getTheme());

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>
);
