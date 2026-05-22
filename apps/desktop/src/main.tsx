import { StrictMode, useEffect, useState } from "react";
import { createRoot } from "react-dom/client";

import App from "./App";
import {
  DesktopSnapshot,
  getDesktopCommandFetcher,
  getMockDesktopSnapshot,
  getUnavailableDesktopSnapshot,
  isTauriRuntime,
  loadDesktopSnapshot,
} from "./commandAdapter";

const root = document.getElementById("root");

if (!root) {
  throw new Error("Root element #root is missing");
}

createRoot(root).render(
  <StrictMode>
    <DesktopRoot />
  </StrictMode>,
);

function DesktopRoot() {
  const commandFetcher = getDesktopCommandFetcher();
  const [snapshot, setSnapshot] = useState<DesktopSnapshot>(() =>
    isTauriRuntime()
      ? {
          ...getUnavailableDesktopSnapshot("Loading local desktop commands."),
          loading: true,
        }
      : getMockDesktopSnapshot(),
  );

  useEffect(() => {
    let active = true;

    loadDesktopSnapshot()
      .then((loadedSnapshot) => {
        if (active) {
          setSnapshot(loadedSnapshot);
        }
      })
      .catch((error) => {
        if (active) {
          const message = error instanceof Error ? error.message : "desktop command loading failed";
          setSnapshot(
            isTauriRuntime()
              ? getUnavailableDesktopSnapshot(`Desktop command loading failed: ${message}.`)
              : {
                  ...getMockDesktopSnapshot(),
                  commandSurface: {
                    detail: `Preview shell: ${message}.`,
                  },
                },
          );
        }
      });

    return () => {
      active = false;
    };
  }, []);

  return <App snapshot={snapshot} fetchCommand={commandFetcher} />;
}
