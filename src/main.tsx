import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import Mini from "./pages/Mini";
import "./index.css";

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    {window.location.hash.startsWith("#mini") ? <Mini /> : <App />}
  </React.StrictMode>,
);
