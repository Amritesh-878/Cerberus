/* @refresh reload */
import { render } from "solid-js/web";

import { App } from "./App";
import "./index.css";
import { blockReloads } from "./reloads";

const root = document.getElementById("root");
if (!root) throw new Error("index.html is missing the #root element");

// Dev builds keep reload, since an edited file reloads the page anyway.
if (import.meta.env.PROD) blockReloads(window);

render(() => <App />, root);
