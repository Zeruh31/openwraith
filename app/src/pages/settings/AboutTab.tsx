// Settings > About: version, license, source code, third-party licenses,
// the command-line undo and what this version doesn't do.

import { Button } from "../../components/Button";
import { Card, KeyValueList } from "../../components/Layout";
import { useDict } from "../../i18n";
import { revealLegalNotices } from "../../shell/actions";
import { useAppInfo } from "../../state/appInfo";

/** Settings > About. */
export function AboutTab() {
  const dict = useDict();
  const t = dict.settings;
  const info = useAppInfo();
  return (
    <Card title={t.about.title}>
      <KeyValueList
        items={[
          { key: "version", label: t.about.version, value: <span className="ow-num">{info.version}</span> },
          { key: "license", label: t.about.license, value: t.about.licenseValue },
          {
            key: "repo",
            label: t.about.repository,
            value: <span className="ow-mono ow-selectable ow-break">{info.repository}</span>,
          },
        ]}
      />
      <div className="ow-stack st-about">
        <p>{t.about.copyright}</p>
        <p>{t.about.warranty}</p>
        <div>
          <p>{t.about.thirdParty}</p>
          <div className="ow-row">
            <Button size="sm" icon="folder" onClick={revealLegalNotices}>
              {t.about.showLicenses}
            </Button>
          </div>
        </div>
        <p>{dict.common.disclaimer}</p>
        <p>{t.about.privacy}</p>
        <div>
          <p>{t.about.cliUndo}</p>
          <code className="ow-selectable">{t.about.cliUndoCommand}</code>
        </div>
        <p className="ow-muted">{t.about.notIncluded}</p>
        {info.sim ? <p className="ow-muted">{t.about.simulator}</p> : null}
      </div>
    </Card>
  );
}
