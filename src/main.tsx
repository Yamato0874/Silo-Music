import React from "react";
import { createRoot } from "react-dom/client";
import "./styles.css";
// стенд без Tauri (только при разработке в браузере): тестовые данные вместо ядра
const boot = async () => {
  if (import.meta.env.DEV && !("__TAURI_INTERNALS__" in window)) await import("./devmock");
  const { default: App } = await import("./App");
  createRoot(document.getElementById("root")!).render(<React.StrictMode><App /></React.StrictMode>);
};
boot();
