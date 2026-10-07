import { selectTransport } from "@skyla/ipc";
import { applyAppearance, storedAppearance } from "@skyla/ui";
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { App } from "./App";
import "./styles.css";

// Before the first paint, so the window never flashes the wrong appearance.
applyAppearance(storedAppearance());

const transport = selectTransport({ forceMock: import.meta.env.VITE_IPC === "mock" });
const root = document.getElementById("root");
if (!root) throw new Error("index.html is missing #root");

createRoot(root).render(
  <StrictMode>
    <App transport={transport} />
  </StrictMode>,
);
