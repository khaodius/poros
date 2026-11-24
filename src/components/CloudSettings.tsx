import { useCloudProviders } from "../hooks/useCloudProviders";
import type { Settings } from "../lib/settings";
import { saveSettingsSection } from "../state/settingsStore";
import { SettingGroup, TextSetting } from "./settingsFields";
import { useT } from "../i18n";

export function CloudSettingsPage({ settings }: { settings: Settings }) {
  const t = useT();
  const cloud = settings.cloud;
  const statuses = useCloudProviders();
  const set = (change: Partial<Settings["cloud"]>) => void saveSettingsSection("cloud", change);

  return (
    <>
      <SettingGroup title={t("cloud.googleDrive")}>
        <p className="setting-note">
          {statuses?.google.builtIn
            ? t("cloud.thisReleaseSignsInWith")
            : t("cloud.porosSignsInToGoogle")}
        </p>
        <ol className="setting-steps">
          <li>{t("cloud.inGoogleCloudConsoleCreate")}</li>
          <li>{t("cloud.setUpTheOauthConsent")}</li>
          <li>{t("cloud.createAnOauthClientId")}</li>
        </ol>
        <TextSetting
          label={t("cloud.clientId")}
          value={cloud.googleClientId}
          placeholder={t("cloud.endsInAppsGoogleusercontentCom")}
          wide
          onCommit={(googleClientId) => set({ googleClientId })}
        />
        <TextSetting
          label={t("cloud.clientSecret")}
          hint={t("cloud.clientSecretHint")}
          value={cloud.googleClientSecret}
          wide
          onCommit={(googleClientSecret) => set({ googleClientSecret })}
        />
      </SettingGroup>

      <SettingGroup title={t("cloud.onedrive")}>
        <p className="setting-note">
          {statuses?.microsoft.builtIn
            ? t("cloud.thisReleaseSignsInWithIts")
            : t("cloud.porosSignsInToOnedrive")}
        </p>
        <ol className="setting-steps">
          <li>{t("cloud.inTheMicrosoftEntraAdmin")}</li>
          <li>
            {t.rich("cloud.underAuthenticationAddTheMobile", {
              uri: <code>http://localhost</code>,
            })}
          </li>
          <li>
            {t.rich("cloud.underApiPermissionsAddThe", {
              files: <code>Files.ReadWrite.All</code>,
              offline: <code>offline_access</code>,
            })}
          </li>
        </ol>
        <TextSetting
          label={t("cloud.applicationClientId")}
          value={cloud.microsoftClientId}
          placeholder="00000000-0000-0000-0000-000000000000"
          wide
          onCommit={(microsoftClientId) => set({ microsoftClientId })}
        />
        <TextSetting
          label={t("cloud.directoryTenant")}
          hint={t("cloud.directoryTenantHint")}
          value={cloud.microsoftTenant}
          placeholder="common"
          wide
          onCommit={(microsoftTenant) => set({ microsoftTenant })}
        />
      </SettingGroup>

      <SettingGroup title={t("cloud.box")}>
        <p className="setting-note">
          {statuses?.box.builtIn
            ? t("cloud.thisReleaseSignsInWithItsOwn")
            : t("cloud.porosSignsInToBox")}
        </p>
        <ol className="setting-steps">
          <li>{t("cloud.inTheBoxDeveloperConsole")}</li>
          <li>
            {t.rich("cloud.onTheConfigurationTabAdd", { uri: <code>http://localhost</code> })}
          </li>
          <li>{t("cloud.underApplicationScopesKeepRead")}</li>
          <li>{t("cloud.copyTheClientIdAnd")}</li>
        </ol>
        <TextSetting
          label={t("cloud.clientId")}
          value={cloud.boxClientId}
          wide
          onCommit={(boxClientId) => set({ boxClientId })}
        />
        <TextSetting
          label={t("cloud.clientSecret")}
          hint={t("cloud.clientSecretHint2")}
          value={cloud.boxClientSecret}
          wide
          onCommit={(boxClientSecret) => set({ boxClientSecret })}
        />
      </SettingGroup>
    </>
  );
}
