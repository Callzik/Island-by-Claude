import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import App from "./App";
import { call, type InitPayload } from "./api";
import "./styles.css";

document.addEventListener("contextmenu", (e) => {
  // the native context menu makes no sense on the island; inputs keep theirs
  if (!(e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement)) e.preventDefault();
});

call<InitPayload>("init").then((boot) => {
  createRoot(document.getElementById("root")!).render(
    <StrictMode>
      <App boot={boot} />
    </StrictMode>,
  );
});
