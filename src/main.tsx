import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";

// Tailwind 4 CSS with Design System
import "@/styles/app.css";

// Fonts
import "@fontsource-variable/noto-serif";
import "@fontsource-variable/noto-sans";
import "@fontsource/noto-sans-mono/400.css";
import "@fontsource/noto-sans-mono/500.css";

// i18n Configuration
import "@/i18n";

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
