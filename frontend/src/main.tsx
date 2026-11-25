import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { BrowserRouter } from "react-router-dom";
import App from "./App";
import { AppProvider } from "./context/app";
import { DiceProvider } from "./context/dice";
import { DialogProvider, DrawerProvider, ToastProvider } from "./context/ui";
import "./styles/fonts.css";
import "./styles/app.css";
import "./styles/dm.css";

/** The router's base: "" when served at the site's top, "/Questodian" for the web version. */
const basename = import.meta.env.BASE_URL.replace(/\/$/, "");

function render() {
  createRoot(document.getElementById("root")!).render(
    <StrictMode>
      <BrowserRouter basename={basename}>
        <ToastProvider>
          <DialogProvider>
            <DrawerProvider>
              <AppProvider>
                <DiceProvider>
                  <App />
                </DiceProvider>
              </AppProvider>
            </DrawerProvider>
          </DialogProvider>
        </ToastProvider>
      </BrowserRouter>
    </StrictMode>,
  );
}

// The web version (`make pwa`) answers the API itself, in this browser: start that first.
if (import.meta.env.MODE === "web") {
  import("./web/boot")
    .then(({ boot }) => boot())
    .then(render, (e: unknown) => {
      document.getElementById("root")!.textContent = `Questodian can't start in this browser: ${e instanceof Error ? e.message : e}`;
    });
} else {
  render();
}
