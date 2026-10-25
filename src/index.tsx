import App from "./App";
import { StrictMode } from "react";
import "./index.css";
import { createRoot } from "react-dom/client";
import { UpdateWindow } from "./pages/UpdateWindow";
import { FileBrowserWindow } from "./pages/FileBrowserWindow";

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
