// Backup & profiles > Share a profile: export one profile to a
// `.owprofile.json` file (no USB serial number) and import a profile file
// into any of the keyboard's profiles through the standard restore review.

import { Card } from "../../../components/Layout";
import { useDict } from "../../../i18n";
import type { BackupDemo } from "../demo";
import { ExportPanel } from "./ExportPanel";
import { ImportPanel } from "./ImportPanel";

/** Props of {@link ShareProfile}. */
export interface ShareProfileProps {
  /** The preview's demo actions (browser preview only). */
  demo?: BackupDemo | undefined;
  /** Called after an import was applied (a new safety backup exists). */
  onImported?: (() => void) | undefined;
}

function ignore(): void {
  // Nothing else to refresh.
}

/** The "Share a profile" section. */
export default function ShareProfile({ demo, onImported = ignore }: ShareProfileProps) {
  const dict = useDict();
  const t = dict.backupRestore.share;
  return (
    <Card title={t.title} className="br-share">
      <p className="ow-muted">{t.intro}</p>
      <p className="ow-muted ow-small">{t.noSerial}</p>
      <div className="br-share__grid">
        <ExportPanel demo={demo} />
        <ImportPanel demo={demo} onImported={onImported} />
      </div>
    </Card>
  );
}
