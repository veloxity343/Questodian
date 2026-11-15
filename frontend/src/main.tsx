import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { BrowserRouter } from "react-router-dom";
import App from "./App";
import { AppProvider } from "./context/app";
import { DiceProvider } from "./context/dice";
import { DialogProvider, DrawerProvider, ToastProvider } from "./context/ui";
import "@fontsource/alegreya/600.css";
import "@fontsource/alegreya/700.css";
import "@fontsource/alegreya/400-italic.css";
import "@fontsource/alegreya-sans/400.css";
import "@fontsource/alegreya-sans/400-italic.css";
import "@fontsource/alegreya-sans/500.css";
import "@fontsource/alegreya-sans/700.css";
import "./styles/app.css";
import "./styles/dm.css";

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <BrowserRouter>
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
