# Virtual folders of the mount
folder-my-drive = My Drive
folder-shared-drives = Shared drives
folder-trash = .Drive Trash
folder-lost-found = .Lost and Found
folder-canceled-uploads = .Canceled uploads
folder-shortcut-targets = .shortcut-targets-by-id

# Mount
conflict-word = conflict
computers-folder = Computers
mount-moved = The Drive folder was opened at { $path }

# Page shown in the browser at the end of sign-in
login-page-title = Google Drive for Linux
login-success = All set! You can close this window and go back to the app.
login-failure = Sign-in failed. Close this window and try again in the app.

# Notifications
notify-title = Google Drive
notify-account = Account: { $email }
notify-lost-found-title = Edit saved to Lost and Found
notify-lost-found-body = Your version of { $name } wasn't uploaded because { $reason }. It was saved to Lost and Found.
notify-lost-found-action = View files
notify-folder-action = See folders
notify-folder-missing-title = A synced folder is missing
notify-folder-missing-body = The folder "{ $name }" wasn't found and stopped syncing. If it moved, choose the new location in Preferences.
notify-folder-no-permission-title = No permission to read a folder
notify-folder-no-permission-body = The folder "{ $name }" stopped syncing because the app can't read it.
notify-folder-moved-title = A synced folder was moved
notify-folder-moved-body = The folder "{ $name }" was moved and stopped syncing. Choose the new location in Preferences.
notify-folder-mass-delete-title = Many files deleted at once
notify-folder-mass-delete-body = { $count } items from "{ $name }" would go to the trash. Nothing was deleted: confirm in Preferences if it was on purpose.
notify-device-ask-title = Back up this device?
notify-device-ask-body = "{ $name }" was connected. Do you want to back up its files to Google Drive?
notify-device-ask-action = Back up
notify-device-done-title = Backup complete
notify-device-done-body = The backup of "{ $name }" is done. You can safely remove the device.
notify-crash-title = The app closed unexpectedly
notify-crash-body = Sending the report helps fix the problem. It doesn't include your files or sign-in details, and you review the text before sending.
notify-crash-action = Send report
notify-device-error-title = Backup failed
notify-device-error-body = The backup of "{ $name }" didn't finish: { $error }
notify-device-unreadable = the device couldn't be read
reason-remote-changed = the file was changed in the cloud
reason-remote-deleted = the file was deleted in the cloud
reason-no-permission = you no longer have permission to edit it
notify-offline-title = No connection
notify-offline-body = Google Drive will sync again when you're back online. Files available offline still open.
notify-signed-out-title = Sign in to the account again
notify-signed-out-body = Google no longer accepts access for this account. Files already on this computer still open, and anything you save is uploaded after you sign in again.
notify-signed-out-action = Sign in
notify-disk-low-title = Disk almost full
notify-disk-low-body = { $free } MB left. Google Drive freed what it could from the cache.
notify-quota-title = Google storage is full
notify-quota-body = New files and edits won't be uploaded until you free up space in your account.
notify-upload-failed-title = Upload failed
notify-upload-failed-body = { $name }: { $error }
notify-shortcut-title = Shortcut created
notify-shortcut-body = Google Docs can't be copied, so the shortcut { $name } was created instead.
notify-paused-title = Syncing is paused
notify-paused-body = { $name } was saved on this computer and will be uploaded when you resume syncing.
notify-paused-action = Resume
notify-canceled-title = Upload canceled
notify-canceled-body = Your version of { $name } is kept in Canceled uploads.
notify-canceled-action = View files
notify-moved-shared-title = File moved to a shared drive
notify-moved-shared-body = { $name } now belongs to the shared drive: whoever can see the file is now whoever has access to that drive.
notify-folder-move-title = The folder could not be moved
notify-folder-move-body = Google doesn't allow moving the folder { $name } between My Drive and a shared drive from here. Do it in Google Drive on the web.
notify-kind-moved-to-shared-drive = Files moved to a shared drive
notify-kind-copied-as-shortcut = Google Docs copied as shortcuts
notify-kind-mount-moved = Drive folder opened in another place
notify-kind-resume-syncing = Files saved while syncing is paused
notify-kind-offline = No internet connection
notify-no-permission-title = Action refused in Drive
notify-no-permission-body = You don't have permission to { $action } "{ $name }".
no-permission-create = create
no-permission-move = move
no-permission-rename = rename
no-permission-delete = delete
notify-quota-almost-title = Storage almost full
notify-quota-almost-body = The account is using { $percent }% of its storage. When it's full, new files and edits stop uploading.
notify-uploads-stuck-title = Uploads stuck
notify-uploads-stuck-body = { $count ->
    [1] One file has been waiting to upload for more than 3 days. See Activity for details.
   *[other] { $count } files have been waiting to upload for more than 3 days. See Activity for details.
}
notify-local-disk-full-title = Computer disk full
notify-local-disk-full-body = There's no room on the disk to save Drive files. Free up space to keep saving.
notify-account-disabled-title = Account blocked for Drive
notify-account-disabled-body = Whoever manages this account blocked access to Drive. Nothing syncs until access is back; contact your administrator.
notify-items-trashed-title = Items moved to the trash
notify-items-trashed-body = { $count ->
    [1] One item left Drive and went to the computer's trash.
   *[other] { $count } items left Drive and went to the computer's trash.
}
notify-kind-quota-almost-full = Storage almost full
notify-kind-uploads-stuck = Uploads stuck for days
notify-kind-items-trashed = Computer folder items moved to the trash
notify-kind-no-permission = Actions refused for lack of permission
notify-new-version-title = New version available
notify-new-version-body = Version { $version } of the app has been released.
notify-new-version-action = See what's new
notify-items-recovered-title = Items recovered
notify-items-recovered-body = { $count ->
    [1] The app's data for this account was damaged. An edit that hadn't been uploaded yet was saved to Lost and Found; move it back if you want it.
   *[other] The app's data for this account was damaged. { $count } edits that hadn't been uploaded yet were saved to Lost and Found; move them back if you want them.
}
notify-suspicious-title = Suspicious activity in your files
notify-suspicious-body = Many files changed in a way that looks like a virus encrypting them. Uploads for this account are paused; previous versions are still in Drive.
notify-suspicious-action = Review
notify-suspicious-recognize = I recognize this activity
notify-suspicious-allow = Allow syncing

# Signing in and out
login-rejected = The account { $email } isn't allowed on this computer.
shared-drives-pending = A drive you turned off has files that have not been uploaded yet. Wait for the uploads to finish (or keep the drive on) and save again.
logout-pending = There are { $count } items not yet in Drive (pending uploads or copies in Lost and Found). Choose to finish the uploads, move the files to a folder, or discard them; Lost and Found is never deleted.
reset-local-offline = No connection to Google Drive. Local data can only be reset while the computer is online.
not-logged-in = No account is connected.
uninstall-signed-in = Disconnect the account before deleting the data on this computer: signing out decides what happens to pending uploads.
path-not-in-drive = { $path } isn't in My Drive or in a shared drive.

# Provisional CLI
cli-client-id = Client ID:
cli-client-secret = Client secret:
cli-client-guide = To create your own Client ID, follow docs/guia-client-id.md.
cli-open-url = Open this address in your browser to sign in:
cli-login-ok = Account connected: { $email }
cli-login-failed = Couldn't sign in: { $error }
cli-service-gone = The service closed the connection.
cli-uninstall-confirm = Uninstall Google Drive from this computer? [y/N]
cli-uninstall-unknown = Could not tell how the app was installed (neither by package nor by install.sh): run the uninstall from the installed program.
cli-uninstall-cancelled = Uninstall canceled: the administrator password was not confirmed.
cli-uninstall-failed = Removing the package failed ({ $error }).
cli-uninstall-no-service = Warning: the service did not respond, so the folder, the autostart entry and the data may have been left behind. { $error }
cli-uninstall-done = Google Drive was uninstalled.
cli-no-service = The Google Drive service isn't running ({ $error }).
cli-status-account = Account: { $account }
cli-status-mount = Folder: { $path }
cli-status-pending = Pending uploads: { $count }
cli-status-paused = Syncing paused
cli-status-offline = No connection
cli-status-last-sync = Last sync: { $when }
cli-status-errors = Failures: { $count }
cli-not-pointer = { $path } isn't a Google Docs pointer.
cli-menu-link = Item link (copy it from here: there is no clipboard program)
cli-menu-failed = Couldn't do that
notify-cache-moving-title = Moving the cache
notify-cache-moving-body = Downloaded files are moving to { $path }. Drive opens when this finishes.
notify-cache-moved-title = Cache moved
notify-cache-moved-body = Downloaded files are now in { $path }.
notify-link-copied-title = Link copied
notify-link-restricted = Only people with access can open this link.
notify-link-anyone = Anyone with the link can { $role }.
notify-link-domain = Anyone at { $domain } with the link can { $role }.
link-role-reader = view
link-role-commenter = comment
link-role-writer = edit
tray-synced = Everything is up to date
tray-syncing = { $count ->
    [1] Syncing 1 file
   *[other] Syncing { $count } files
}
tray-offline = { $count ->
    [0] No internet connection
   *[other] No internet connection ({ $count } to sync)
}
tray-paused = { $count ->
    [0] Paused
   *[other] Paused ({ $count } to sync)
}
tray-quota-full = Storage is full
tray-needs-sign-in = Sign in to the account again
tray-error = { $count ->
    [1] 1 file could not be synced
   *[other] { $count } files could not be synced
}
tray-signed-out = You are not signed in
tray-starting = Connecting to your account…
tray-accounts = Connected with { $count } accounts
tray-no-service = The Google Drive service is not running
tray-open-folder = Open the Google Drive folder
tray-open-web = Open Google Drive in the browser
tray-search = Search
tray-pause = Pause syncing
tray-resume = Resume syncing
tray-offline-files = Offline files
tray-errors = { $count ->
    [0] Error list
   *[other] Error list ({ $count })
}
tray-preferences = Preferences
tray-about = About
tray-feedback = Send feedback
tray-diagnostics = Generate diagnostic info
tray-quit = Quit
tray-sign-in = Sign in
tray-start-service = Start the service
tray-hotkey-search = Open Google Drive search

# Window titles (gdl-ui)
window-home = Google Drive
window-onboarding = Sign in
window-preferences = Preferences
window-search = Search
window-errors = Error list
window-offline = Offline files
window-about = About
window-feedback = Send feedback
window-notification-settings = Notification preferences
window-uninstall = Uninstall Google Drive
window-share = Share
window-versions = Manage versions
window-shared-drives = Shared drives
window-shortcut = Add shortcut to Drive

## The "Google Drive" right-click menu in file managers.
menu-open-web = Open in browser
menu-copy-link = Copy link
menu-add-shortcut = Add shortcut to Drive
menu-open-docs = Open with Google Docs
menu-open-sheets = Open with Google Sheets
menu-open-slides = Open with Google Slides
menu-share = Share
menu-versions = Manage versions
menu-offline = Available offline
menu-online-only = Online only
menu-new-document = New Google Doc
menu-new-spreadsheet = New Google Sheet
menu-new-presentation = New Google Slides
menu-refresh = Refresh folder
menu-trash = View items in trash
menu-new-shared-drive = New shared drive
menu-manage-shared-drives = Manage shared drives
menu-sync-folder = Sync this folder with Google Drive
new-document-name = Untitled document
new-spreadsheet-name = Untitled spreadsheet
new-presentation-name = Untitled presentation
