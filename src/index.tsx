import App from "./App";
import { StrictMode } from "react";
import "./index.css";
import { createRoot } from "react-dom/client";
import { FileBrowserWindow } from "./pages/FileBrowserWindow";

const params = new URLSearchParams(window.location.search);
const isFileBrowserWindow = params.get("window") === "file-browser";

createRoot(document.getElementById("root")!).render(
    <StrictMode>
        {isFileBrowserWindow ? <FileBrowserWindow /> : <App />}
    </StrictMode>,
);
