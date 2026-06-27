import { pickDestination, setDestination, setTheme } from "../api";
import { useStore } from "../store";
import { formatBytes } from "../utils/format";
import { IconCopy, IconFolder, IconGear, IconMoon, IconSun } from "./icons";

export function TopBar() {
  const destination = useStore((s) => s.destination);
  const freeSpace = useStore((s) => s.freeSpace);
  const theme = useStore((s) => s.theme);
  const setSettingsOpen = useStore((s) => s.setSettingsOpen);

  return (
    <header className="topbar">
      <div className="brand">
        <span className="brand-logo">
          <IconCopy size={18} />
        </span>
        <span className="brand-name">Fast-copy</span>
      </div>

      <div className="dest">
        <span className="dest-label">Destination</span>
        <input
          className="dest-input"
          value={destination}
          placeholder="Select a destination folder…"
          onChange={(e) => void setDestination(e.target.value)}
        />
        <button className="btn ghost" onClick={() => void pickDestination()}>
          <IconFolder size={15} />
          Browse
        </button>
        {freeSpace != null && (
          <span className="dest-free">{formatBytes(freeSpace)} free</span>
        )}
      </div>

      <div className="topbar-actions">
        <button
          className="icon-btn"
          title="Toggle theme"
          onClick={() => void setTheme(theme === "dark" ? "light" : "dark")}
        >
          {theme === "dark" ? <IconSun size={17} /> : <IconMoon size={17} />}
        </button>
        <button
          className="icon-btn"
          title="Settings"
          onClick={() => setSettingsOpen(true)}
        >
          <IconGear size={17} />
        </button>
      </div>
    </header>
  );
}
