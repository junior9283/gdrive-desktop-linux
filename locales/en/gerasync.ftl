# GeraSync (`--cfg gerasync`) texts that replace the ones in `main.ftl` and `ui.ftl`: the app name
# and the sign-in without a Client ID. Texts about the Google service stay as they are.

# ui.ftl
about-name = GeraSync
about-unofficial = GeraSync is an unofficial Google Drive client and is not affiliated with or endorsed by Google.
quit-title = Quit GeraSync?
onboarding-files-text = GeraSync is a folder at { $path }. Open it in Dolphin and use it like any other.
onboarding-offline-command = gerasync pin <path>
onboarding-sign-in-text = Click Continue: your browser opens the Google sign-in page.
onboarding-waiting-unverified = If you see "Google hasn't verified this app", click Advanced and then Go to GeraSync: Google's verification is not finished yet. Your data only travels between this computer and Google.
pref-drive-folder = GeraSync folder
onboarding-summary-mount = GeraSync folder
folder-refused-nested = This folder is already inside a synced folder or the GeraSync folder.
folder-refused-contains = This folder contains a synced folder or the GeraSync folder.
mirror-config-text = A copy of all of My Drive stays in this folder on your computer, available even without internet. Shared drives stay in the GeraSync folder.
uninstall-title = Uninstall GeraSync?
uninstall-body = The programs are removed from this computer and the GeraSync folder no longer appears. Your files in Google Drive, in the cloud, are not touched.
uninstall-done = GeraSync uninstalled
share-missing = Open this window from the GeraSync menu in your file manager.
versions-missing = Open this window from the GeraSync menu in your file manager.

# main.ftl
login-page-title = GeraSync
notify-title = GeraSync
notify-offline-body = GeraSync will sync again when you're back online. Files available offline still open.
notify-disk-low-body = { $free } MB left. GeraSync freed what it could from the cache.
cli-uninstall-confirm = Uninstall GeraSync from this computer? [y/N]
cli-uninstall-done = GeraSync was uninstalled.
cli-no-service = The GeraSync service isn't running ({ $error }).
tray-no-service = The GeraSync service is not running
tray-open-folder = Open the GeraSync folder
tray-hotkey-search = Open GeraSync search
window-home = GeraSync
window-uninstall = Uninstall GeraSync
