/// <reference types="@microsoft/msfs-types/pages/vcockpit/core/vcockpit" />
/// <reference types="@microsoft/msfs-types/js/common" />

import { DisplayComponent, FSComponent, type VNode } from "@microsoft/msfs-sdk";

import "./index.css";

class ReplaceMePanel extends DisplayComponent<{}> {
  public render(): VNode {
    return (
      <div class="replace-me">
        <span>Hello from ReplaceMe</span>
      </div>
    );
  }
}

class ReplaceMeInstrument extends BaseInstrument {
  get templateID(): string {
    return "ReplaceMe";
  }

  public connectedCallback(): void {
    super.connectedCallback();
    const mount = document.getElementById("replace-me-mount");
    if (mount) {
      FSComponent.render(<ReplaceMePanel />, mount);
    }
  }

  public disconnectedCallback(): void {
    super.disconnectedCallback();
  }
}

registerInstrument("replace-me-instrument", ReplaceMeInstrument);
