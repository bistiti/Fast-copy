// Reusable throbber. Use inline (with optional label) anywhere the user has to
// wait on a blocking operation — scanning, benchmarking, etc.

interface Props {
  size?: number;
  label?: string;
  /** Center it in a flex column for empty-state / overlay use. */
  block?: boolean;
}

export function Spinner({ size = 16, label, block = false }: Props) {
  const ring = (
    <span
      className="spinner spin"
      style={{ width: size, height: size, borderWidth: Math.max(2, size / 8) }}
    />
  );

  if (block) {
    return (
      <div className="spinner-block">
        {ring}
        {label && <span className="spinner-label">{label}</span>}
      </div>
    );
  }

  return (
    <span className="spinner-inline">
      {ring}
      {label && <span className="spinner-label">{label}</span>}
    </span>
  );
}
