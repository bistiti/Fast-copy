import { useEffect } from "react";
import { loadConfig, setupListeners } from "./api";
import { useStore } from "./store";
import { TopBar } from "./components/TopBar";
import { SourcesPanel } from "./components/SourcesPanel";
import { QueuePanel } from "./components/QueuePanel";
import { ProgressDock } from "./components/ProgressDock";
import { SettingsModal } from "./components/SettingsModal";
import { CompletionSummary } from "./components/CompletionSummary";
import { Toast } from "./components/Toast";

export function App() {
  const theme = useStore((s) => s.theme);
  const settingsOpen = useStore((s) => s.settingsOpen);
  const phase = useStore((s) => s.phase);
  const summary = useStore((s) => s.summary);

  // One-time init: load persisted config and wire up IPC event listeners.
  useEffect(() => {
    void loadConfig();
    void setupListeners();
  }, []);

  // Reflect the theme choice on the root element for CSS variables.
  useEffect(() => {
    document.documentElement.dataset.theme = theme;
  }, [theme]);

  return (
    <div className="app">
      <TopBar />
      <main className="layout">
        <SourcesPanel />
        <QueuePanel />
      </main>
      <ProgressDock />

      {settingsOpen && <SettingsModal />}
      {phase === "done" && summary && <CompletionSummary summary={summary} />}
      <Toast />
    </div>
  );
}
