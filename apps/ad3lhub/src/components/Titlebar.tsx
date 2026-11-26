import { getCurrentWindow, type Window } from "@tauri-apps/api/window";
import { useRef } from "react";
import logo from "../assets/logo.png";

function tryWindow(): Window | null {
  try {
    return getCurrentWindow();
  } catch {
    return null;
  }
}

export function Titlebar() {
  const winRef = useRef<Window | null>(null);
  if (winRef.current === null) {
    winRef.current = tryWindow();
  }
  const dragging = useRef(false);

  const win = winRef.current;

  return (
    <header
      className="titlebar"
      data-tauri-drag-region
      onPointerDown={(e) => {
        if (e.button !== 0 || !win) return;
        const t = e.target as HTMLElement;
        if (t.closest(".titlebar-controls")) return;
        if (dragging.current) return;
        dragging.current = true;
        void win.startDragging().finally(() => {
          dragging.current = false;
        });
      }}
    >
      <div className="titlebar-brand" data-tauri-drag-region>
        <img className="titlebar-mark" src={logo} alt="" draggable={false} />
        <span className="titlebar-label">Ad3lHub</span>
      </div>
      <div className="titlebar-controls">
        <button
          type="button"
          className="titlebar-btn"
          aria-label="Minimize"
          onClick={() => void win?.minimize()}
        >
          <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden>
            <path d="M1 5h8" stroke="currentColor" strokeWidth="1.2" />
          </svg>
        </button>
        <button
          type="button"
          className="titlebar-btn"
          aria-label="Maximize"
          onClick={() =>
            void win?.isMaximized().then((m) => (m ? win.unmaximize() : win.maximize()))
          }
        >
          <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden>
            <rect
              x="1.5"
              y="1.5"
              width="7"
              height="7"
              fill="none"
              stroke="currentColor"
              strokeWidth="1.2"
            />
          </svg>
        </button>
        <button
          type="button"
          className="titlebar-btn titlebar-btn-close"
          aria-label="Close"
          onClick={() => void win?.close()}
        >
          <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden>
            <path d="M2 2l6 6M8 2L2 8" stroke="currentColor" strokeWidth="1.2" />
          </svg>
        </button>
      </div>
    </header>
  );
}
