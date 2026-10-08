import { useCloudProviders } from "../hooks/useCloudProviders";
import type { Settings } from "../lib/settings";
import { saveSettingsSection } from "../state/settingsStore";
import { SettingGroup, TextSetting } from "./settingsFields";

export function CloudSettingsPage({ settings }: { settings: Settings }) {
  const cloud = settings.cloud;
  const statuses = useCloudProviders();
  const set = (change: Partial<Settings["cloud"]>) => void saveSettingsSection("cloud", change);

  return (
    <>
      <SettingGroup title="Google Drive">
        <p className="setting-note">
          {statuses?.google.builtIn
            ? "This release signs in with its own Google app. To use yours instead:"
            : "Poros signs in to Google Drive with a Google app you register:"}
        </p>
        <ol className="setting-steps">
          <li>In Google Cloud Console, create a project and enable the Google Drive API.</li>
          <li>
            Set up the OAuth consent screen. While the app is in testing, add your Google account as
            a test user.
          </li>
          <li>Create an OAuth client ID of type Desktop app.</li>
        </ol>
        <TextSetting
          label="Client ID"
          value={cloud.googleClientId}
          placeholder="Ends in apps.googleusercontent.com"
          wide
          onCommit={(googleClientId) => set({ googleClientId })}
        />
        <TextSetting
          label="Client secret"
          hint="Google issues one with every desktop app client."
          value={cloud.googleClientSecret}
          wide
          onCommit={(googleClientSecret) => set({ googleClientSecret })}
        />
      </SettingGroup>

      <SettingGroup title="OneDrive">
        <p className="setting-note">
          {statuses?.microsoft.builtIn
            ? "This release signs in with its own Microsoft app. To use yours instead:"
            : "Poros signs in to OneDrive with a Microsoft app you register:"}
        </p>
        <ol className="setting-steps">
          <li>
            In the Microsoft Entra admin center, register an app for accounts in any organizational
            directory and personal Microsoft accounts.
          </li>
          <li>
            Under Authentication, add the Mobile and desktop applications platform with the redirect
            URI <code>http://localhost</code> and allow public client flows.
          </li>
          <li>
            Under API permissions, add the Microsoft Graph delegated permissions{" "}
            <code>Files.ReadWrite.All</code> and <code>offline_access</code>.
          </li>
        </ol>
        <TextSetting
          label="Application (client) ID"
          value={cloud.microsoftClientId}
          placeholder="00000000-0000-0000-0000-000000000000"
          wide
          onCommit={(microsoftClientId) => set({ microsoftClientId })}
        />
        <TextSetting
          label="Directory (tenant)"
          hint="Leave empty for any account. Enter a directory ID to limit sign-in to one organization."
          value={cloud.microsoftTenant}
          placeholder="common"
          wide
          onCommit={(microsoftTenant) => set({ microsoftTenant })}
        />
      </SettingGroup>
    </>
  );
}
