import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import "@fontsource-variable/jetbrains-mono";
import "./index.css";
import App from "./App";

function applyStandaloneClass() {
  const iosStandalone =
    "standalone" in navigator &&
    (navigator as Navigator & { standalone?: boolean }).standalone === true;
  const displayStandalone = window.matchMedia?.("(display-mode: standalone)").matches ?? false;
  document.documentElement.classList.toggle("standalone", iosStandalone || displayStandalone);
}

applyStandaloneClass();
const standaloneQuery = window.matchMedia?.("(display-mode: standalone)");
if (standaloneQuery?.addEventListener) {
  standaloneQuery.addEventListener("change", applyStandaloneClass);
} else if (standaloneQuery?.addListener) {
  standaloneQuery.addListener(applyStandaloneClass);
}

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <App />
  </StrictMode>,
);

if ("serviceWorker" in navigator && import.meta.env.PROD) {
  navigator.serviceWorker.register("/sw.js");
}
