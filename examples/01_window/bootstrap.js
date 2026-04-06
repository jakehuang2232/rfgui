function isUnsupportedSafari() {
  const ua = navigator.userAgent;
  const isAppleEngine =
    /Safari/i.test(ua) &&
    !/Chrome|Chromium|CriOS|Edg|EdgiOS|OPR|OPT|Firefox|FxiOS|Android/i.test(ua);
  const isWebKitIOS = /iPhone|iPad|iPod/i.test(ua);
  return isAppleEngine || isWebKitIOS;
}

function bootApi() {
  return window.__RFGUI_BOOT__ ?? {
    hideBootOverlay() {},
    setBootStatus() {},
    setBootError() {},
    formatBootError(_error, fallbackMessage) {
      return fallbackMessage;
    },
  };
}

const UNSUPPORTED_MESSAGE =
  "Safari is not supported for this web example. Please use Chrome or Edge.";

export default function initializer() {
  const unsupportedSafari = isUnsupportedSafari();

  return {
    onStart() {
      if (!unsupportedSafari) {
        return;
      }

      bootApi().setBootError(UNSUPPORTED_MESSAGE);
      throw new Error(UNSUPPORTED_MESSAGE);
    },

    onSuccess() {
      bootApi().setBootStatus("Wasm initialized. Loading fonts…");
    },

    onFailure(error) {
      if (unsupportedSafari) {
        bootApi().setBootError(UNSUPPORTED_MESSAGE);
        return;
      }

      bootApi().setBootError(
        bootApi().formatBootError(error, "Initialization failed.")
      );
    },
  };
}
