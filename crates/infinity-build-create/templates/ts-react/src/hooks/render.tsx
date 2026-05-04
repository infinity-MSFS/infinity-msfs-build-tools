import type React from "react";
import { createRoot } from "react-dom/client";
import * as Defaults from "./defaults";

export const render = (Slot: React.ReactElement) => {
	const root = createRoot(Defaults.getRenderTarget());
	root.render(Slot);
};
