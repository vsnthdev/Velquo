import App from "./App";
import { initI18n } from "./i18n";
import { StrictMode } from "react";
import "./index.css";
import { createRoot } from "react-dom/client";
import { UpdateWindow } from "./pages/UpdateWindow";
import { FileBrowserWindow } from "./pages/FileBrowserWindow";
import { syncLanguageWithRust, listenForLanguageChanges } from "./i18n";

// Resolve the locale before the first paint so the UI never flips language.
initI18n();

// Rust owns the preference (it builds the native menus), so reconcile once the
// webview is up, and follow changes made from any other window.
void syncLanguageWithRust();
void listenForLanguageChanges();

const params = new URLSearchParams(window.location.search);
const windowKind = params.get("window");

createRoot(document.getElementById("root")!).render(
    <StrictMode>
        {windowKind == "file-browser" ? (
            <FileBrowserWindow />
        ) : windowKind == "updater" ? (
            <UpdateWindow />
        ) : (
            <App />
        )}
    </StrictMode>,
);
