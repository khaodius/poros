import { useMountStore } from "../state/mountStore";

/** Warns that drives are unmounted by an action, and what that means for the files on them. */
export function DrivesGoing({ drives, when }: { drives: string[]; when: string }) {
  const busy = useMountStore((state) =>
    state.drives.some(
      (drive) =>
        drives.includes(drive.mountPoint) &&
        (drive.openFiles.length > 0 || drive.uploads.length > 0),
    ),
  );
  if (drives.length === 0) return null;
  return (
    <>
      <p>
        {drives.length === 1 ? "The drive at " : "The drives at "}
        {drives.map((mountPoint, index) => (
          <span key={mountPoint}>
            {index > 0 && ", "}
            <code className="drive-going-path">{mountPoint}</code>
          </span>
        ))}{" "}
        {drives.length === 1 ? "is" : "are"} unmounted when {when}.
      </p>
      {busy && (
        <p>
          Programs still have files open on it, or uploads are pending. They lose the open files,
          and what is not on the server yet is kept on this computer, listed under the drives in
          the status bar.
        </p>
      )}
    </>
  );
}
