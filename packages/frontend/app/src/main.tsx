import { createRoot } from "react-dom/client";
import { Provider } from "jotai";
import { App } from "./App";
import "./styles/global.css";
import "./styles/omnivyx-tokens.css";
import "./styles/omnivyx-theme-agent.css";

createRoot(document.getElementById("root")!).render(
  <Provider>
    <App />
  </Provider>
);
