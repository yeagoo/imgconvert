// SPDX-License-Identifier: Apache-2.0
// Copyright (C) 2026 ImgConvert contributors

import { mount } from "svelte";
import "./app.css";
import App from "./App.svelte";

const app = mount(App, {
  target: document.getElementById("app")!,
});

export default app;
