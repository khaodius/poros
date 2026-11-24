import { Fragment } from "react";
import { useT } from "../i18n";
import type { DriveView } from "../lib/types";
import { useMountStore } from "../state/mountStore";

/** What unmounts the drives. */
export type DrivesGoingWhen = "tabCloses" | "vaultLocks" | "quit";

const UNMOUNTED_WHEN = {
  tabCloses: "drives.unmountedWhenTheTabCloses",
  vaultLocks: "drives.unmountedWhenTheVaultLocks",
  quit: "drives.unmountedWhenPorosQuits",
} as const;

/** Warns that drives are unmounted by an action, and what that means for the files on them. */
export function DrivesGoing({ drives, when }: { drives: string[]; when: DrivesGoingWhen }) {
  const t = useT();
  const isBusy = (drive: DriveView) =>
    drives.includes(drive.mountPoint) && (drive.openFiles.length > 0 || drive.uploads.length > 0);
  const busy = useMountStore((state) => state.drives.some(isBusy));
  // What a vault's drive keeps is its decrypted contents.
  const busyVault = useMountStore((state) =>
    state.drives.some((drive) => isBusy(drive) && drive.protocol === "cryptomator"),
  );
  if (drives.length === 0) return null;
  const paths = drives.map((mountPoint, index) => (
    <Fragment key={mountPoint}>
      {index > 0 && t("common.listSeparator")}
      <code className="drive-going-path">{mountPoint}</code>
    </Fragment>
  ));
  return (
    <>
      <p>{t.rich(UNMOUNTED_WHEN[when], { count: drives.length, paths })}</p>
      {busy && (
        <p>
          {busyVault
            ? t("drives.busyDrivesLoseOpenFilesWithVault")
            : t("drives.busyDrivesLoseOpenFiles")}
        </p>
      )}
    </>
  );
}
