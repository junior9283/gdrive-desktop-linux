# Virtual folders of the mount
folder-my-drive = My Drive
folder-trash = .Drive Trash
folder-lost-found = .Lost and Found
folder-shortcut-targets = .shortcut-targets-by-id

# Mount
mount-moved = The Drive folder was opened at { $path }

# Page shown in the browser at the end of sign-in
login-page-title = Google Drive for Linux
login-success = All set! You can close this window and go back to the app.
login-failure = Sign-in failed. Close this window and try again in the app.

# Notifications
notify-title = Google Drive
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

# Signing in and out
login-rejected = The account { $email } isn't allowed on this computer.
logout-pending = There are { $count } items not yet in Drive (pending uploads or copies in Lost and Found). Choose to finish the uploads, move the files to a folder, or discard them; Lost and Found is never deleted.
not-logged-in = No account is connected.
path-not-in-drive = { $path } isn't in My Drive.

# Provisional CLI
cli-client-id = Client ID:
cli-client-secret = Client secret:
cli-client-guide = To create your own Client ID, follow docs/guia-client-id.md.
cli-open-url = Open this address in your browser to sign in:
cli-login-ok = Account connected: { $email }
cli-login-failed = Couldn't sign in: { $error }
cli-service-gone = The service closed the connection.
cli-no-service = The Google Drive service isn't running ({ $error }).
cli-status-account = Account: { $account }
cli-status-mount = Folder: { $path }
cli-status-pending = Pending uploads: { $count }
cli-status-paused = Syncing paused
cli-status-offline = No connection
cli-status-last-sync = Last sync: { $when }
cli-status-errors = Failures: { $count }
cli-not-pointer = { $path } isn't a Google Docs pointer.
