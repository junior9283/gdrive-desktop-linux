# Virtual folders of the mount
folder-my-drive = My Drive
folder-trash = .Drive Trash
folder-lost-found = .Lost and Found
folder-canceled-uploads = .Canceled uploads
folder-shortcut-targets = .shortcut-targets-by-id

# Mount
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
reason-remote-changed = the file was changed in the cloud
reason-remote-deleted = the file was deleted in the cloud
reason-no-permission = you no longer have permission to edit it
notify-offline-title = No connection
notify-offline-body = Google Drive will sync again when you're back online. Files available offline still open.
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
notify-kind-copied-as-shortcut = Google Docs copied as shortcuts
notify-kind-mount-moved = Drive folder opened in another place
notify-kind-resume-syncing = Files saved while syncing is paused
notify-kind-offline = No internet connection

# Signing in and out
login-rejected = The account { $email } isn't allowed on this computer.
logout-pending = There are { $count } items not yet in Drive (pending uploads or copies in Lost and Found). Choose to finish the uploads, move the files to a folder, or discard them; Lost and Found is never deleted.
not-logged-in = No account is connected.
uninstall-signed-in = Disconnect the account before deleting the data on this computer: signing out decides what happens to pending uploads.
path-not-in-drive = { $path } isn't in My Drive.

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
tray-error = { $count ->
    [1] 1 file could not be synced
   *[other] { $count } files could not be synced
}
tray-signed-out = You are not signed in
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
window-notification-settings = Notification preferences
window-uninstall = Uninstall Google Drive
window-share = Share
window-versions = Manage versions

## The "Google Drive" right-click menu in file managers.
menu-open-web = Open in browser
menu-copy-link = Copy link
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
new-document-name = Untitled document
new-spreadsheet-name = Untitled spreadsheet
new-presentation-name = Untitled presentation
