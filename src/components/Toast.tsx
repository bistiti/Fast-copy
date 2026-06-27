import { useEffect } from "react";
import { useStore } from "../store";
import { IconAlert, IconClose } from "./icons";

export function Toast() {
  const toast = useStore((s) => s.toast);
  const showToast = useStore((s) => s.showToast);

  useEffect(() => {
    if (!toast) return;
    const t = setTimeout(() => showToast(null), 6000);
    return () => clearTimeout(t);
  }, [toast, showToast]);

  if (!toast) return null;

  return (
    <div className="toast" role="alert">
      <IconAlert size={16} />
      <span className="toast-msg">{toast}</span>
      <button className="icon-btn tiny" onClick={() => showToast(null)}>
        <IconClose size={13} />
      </button>
    </div>
  );
}
