# Texts of the window screens (gdl-ui), formatted by i18n.js.

service-unavailable = The service is not responding. Trying again…
screen-not-ready = This screen is still being built.
action-close = Close

about-name = gdrive-desktop-linux
about-version = Version { $version }
about-license = Free software, released under the GPL-3.0 license.
about-unofficial = Unofficial Google Drive client: this project is not affiliated with or endorsed by Google.
about-third-party = Third-party software ({ $count })
about-license-file = license file in the package
about-project = Project page: { $url }

action-pause = Pause
action-resume = Resume

header-signed-out = You are not signed in
header-starting = Connecting to your account…
header-synced = Sync complete
header-error = Sync incomplete
header-syncing = { $count ->
    [one] Syncing 1 item
   *[other] Syncing { $count } items
}
header-offline = No internet connection
header-quota-full = Storage is full
header-needs-sign-in = Sign in to the account again
header-paused = Sync paused
header-last-sync-now = Last synced less than a minute ago
header-last-sync = Last synced { $ago }
header-never-synced = Not synced yet
header-remaining = About { $time } left
header-error-detail = { $count ->
    [one] 1 item could not be synced
   *[other] { $count } items could not be synced
}
header-offline-detail = Changes will be uploaded when the connection is back
header-quota-detail = Free up Drive storage to keep syncing

showing = Showing { $from }–{ $to } of { $total }

action-cancel = Cancel
action-retry = Try again
sign-in = Sign in
search-placeholder = Search in Google Drive
account-menu = Account
more-options = More options

view-start = Home
view-activity = Sync activity
view-folders = Connected folders
view-offline = Offline files
view-shared = Shared with me
view-notifications = Notifications
view-links = Quick links

home-signed-out = Sign in to see your files and follow the sync.
home-starting = Connecting to your accounts. This only takes a few seconds.

card-attention = Needs attention
card-attention-none = Nothing needs your attention right now.
card-attention-errors = { $count ->
    [one] 1 item could not be synced.
   *[other] { $count } items could not be synced.
}
card-attention-open = See the error list
card-offline = Files available offline
card-offline-text = { $count ->
    [0] No offline items
    [one] 1 offline item ({ $size })
   *[other] { $count } offline items ({ $size })
}
card-offline-open = See offline files
card-folders = Connected folders
card-folders-none = No folder from this computer is syncing.
card-folders-text = { $count ->
    [one] 1 folder syncing.
   *[other] { $count } folders syncing.
}
card-folders-attention = { $count ->
    [one] 1 folder needs attention.
   *[other] { $count } folders need attention.
}
card-folders-open = See folders
card-drives = Shared drives
card-drives-none = You are not a member of any shared drive.
card-drives-text = { $count ->
    [one] 1 shared drive
   *[other] { $count } shared drives
}
card-drives-off = { $off ->
    [one] { $count } shared drives (1 turned off)
   *[other] { $count } shared drives ({ $off } turned off)
}
card-drives-open = See drives
view-drives = Shared drives
drives-empty = You are not a member of any shared drive. Create one or ask to be added.
drives-off = Turned off
drives-open-folder = Open in folder
card-shared = Shared with me
card-shared-text = { $count ->
    [0] Nobody shared files with you
    [one] 1 item shared with you
   *[other] { $count } items shared with you
}
card-shared-open = See shared items
card-sync = Sync status
card-sync-open = See activity
card-gemini = Gemini
card-gemini-text = Chat about your Drive files with Gemini, on the web.
card-gemini-open = Open Gemini

activity-empty = No sync activity yet.
activity-upload-paused = Paused
activity-upload-queued = Waiting to upload
activity-uploading = Uploading
activity-uploading-percent = Uploading ({ $percent }%)
activity-uploaded = Uploaded
activity-upload-canceling = Canceling the upload
activity-upload-canceled = Upload canceled
activity-upload-failed = Upload failed
activity-downloading = Downloading
activity-downloaded = Downloaded
activity-trashing = Moving to trash
activity-trashed = In the trash
activity-restoring = Restoring
activity-restored = Restored
column-name = Name
column-size = Size
column-date = Date
column-status = Status
page-size = Items per page
page-previous = Previous page
page-next = Next page
row-menu = Item actions
row-open-folder = Open in file manager
row-open-web = Open on the web
row-share = Share
row-copy-link = Copy link
row-cancel-upload = Cancel upload
link-copied = Link copied
upload-canceled = Upload canceled. The local version is in "Canceled uploads".
upload-finished-first = The upload finished before it could be canceled.
upload-in-use = The file is still open in another program. Close it and try again.

offline-empty = No files are available offline.
offline-manage = Manage offline files

shared-empty = Nothing has been shared with you.
shared-by = { $owner } · { $date }
shared-add-shortcut = Add shortcut to Drive
shortcut-title = Add shortcut to Drive
shortcut-text = Choose the folder for the shortcut. The item stays where it is; the shortcut only points to it.
shortcut-places = Places
shortcut-no-subfolders = This folder has no subfolders.
shortcut-no-places = There is no folder where this account can create the shortcut.
shortcut-add = Add shortcut
shortcut-missing = No item to create the shortcut for.

notifications-empty = No notifications.
notification-dismiss = Dismiss

link-drive-web = Google Drive on the web
link-faq = Frequently asked questions
link-help = Help
link-privacy = Privacy Policy
link-terms = Terms of use

account-usage = Using { $used } of { $total }
account-usage-unlimited = Using { $used }
account-disconnect = Disconnect account
account-add = Add another account
account-sign-in-again = Sign in again
account-needs-sign-in = Needs to sign in again
account-disconnect-all = Disconnect all accounts
disconnect-all-title = Disconnect all accounts?
disconnect-all-body = All accounts ({ $count }) will be disconnected from this computer.
disconnect-all-done = Accounts disconnected
onboarding-resume-title = Sign in to the account again
onboarding-resume-text = The sign-in for { $account } has expired. Sign in again with the same account: the files on this computer stay here, and whatever was queued continues once you are back in.
onboarding-step-add = Add account
onboarding-step-resume = Sign in again
onboarding-add-title = Add another Google account
onboarding-add-text = Your browser will open on the Google sign-in page. Sign in with the other account: it gets its own folder, and both sync at the same time. The first account's Client ID is reused.
onboarding-login-failed-already-connected = This account is already connected.
onboarding-login-failed-other-account = You signed in with a different account. Sign in with the account that needs to sign in again.
onboarding-login-failed-too-many-accounts = The limit of 4 accounts has been reached. Disconnect one before adding another.
onboarding-login-failed-add-account-disabled = Your administrator does not allow adding another account.
storage-manage = Manage storage

more-errors = { $count ->
    [0] Error list
   *[other] Error list ({ $count })
}
more-notification-settings = Notification preferences
more-shared-drives = Manage shared drives
more-reset = Reset preferences
more-preferences = Preferences
more-about = About
more-quit = Quit

reset-title = Reset preferences?
reset-body = Everything you changed in the preferences goes back to the default. Settings defined by your administrator still apply.
reset-confirm = Reset
reset-done = Preferences reset
more-feedback = Send feedback
more-diagnostics = Generate diagnostic info
more-verbose-on = Turn on detailed logging
more-verbose-off = Turn off detailed logging
more-reset-local = Reset local data
diagnostics-done = Diagnostic package saved. Its folder was opened.
verbose-on-done = Detailed logging turned on for 24 hours
verbose-off-done = Detailed logging turned off
reset-local-title = Reset local data?
reset-local-body = The app deletes what it stored on this computer about the account and reads Drive again. Offline files will be downloaded again. Your files in Drive and in your computer folders don't change.
reset-local-done = Local data reset. Drive is being read again.

quit-title = Quit Google Drive?
quit-body = { $count ->
    [one] 1 item is still waiting to sync. It will sync the next time the app opens.
   *[other] { $count } items are still waiting to sync. They will sync the next time the app opens.
}

disconnect-title = Disconnect account?
disconnect-body = The account { $account } will be disconnected from this computer.
disconnect-offline-warning = The offline files and the cache of this account will be removed from this computer.
disconnect-pending = { $count ->
    [one] 1 file has not been synced yet. What should happen to it?
   *[other] { $count } files have not been synced yet. What should happen to them?
}
disconnect-finish = Finish syncing, then disconnect
disconnect-move = { $count ->
    [one] Move the unsynced file to a folder
   *[other] Move the { $count } unsynced files to a folder
}
disconnect-discard = { $count ->
    [one] Delete the unsynced file
   *[other] Delete the { $count } unsynced files
}
disconnect-discard-sure = I understand these files will be lost
lost-found-pick-folder = Choose the folder to move Lost and Found to
lost-found-moved = { $count ->
    [one] 1 item moved.
   *[other] { $count } items moved.
}
disconnect-pick-folder = Choose the folder for the unsynced files
disconnect-progress = Disconnecting…
disconnect-done = Account disconnected

# First run
onboarding-steps = Steps
onboarding-step-setup = Create the Client ID
onboarding-setup-title = Create your Client ID in Google Cloud
onboarding-setup-text = In this version the app has no Google sign-in of its own: you create yours, at no cost. It takes about 10 minutes and only needs to be done once. Follow the parts in order; each button opens the right page in your browser. Already have a Client ID? Click Continue.
onboarding-setup-open = Open in Google Cloud
onboarding-setup-copy = Copy
onboarding-setup-copied = Copied
onboarding-setup-project-title = Create a project
onboarding-setup-project-text = Sign in with the Google account you will use in the app. Name the project (for example, Drive on Linux) and click Create. In the next parts, check at the top of the page that this project is selected.
onboarding-setup-drive-api-title = Enable the Google Drive API
onboarding-setup-drive-api-text = On the Google Drive API page, click Enable. Without it the app cannot see your files.
onboarding-setup-branding-title = Set up the consent screen
onboarding-setup-branding-text = Click Get started and fill in the app name (for example, Drive on Linux) and your support email. Under Audience, choose External; enter the contact email, accept the policy and click Create. Then, on the Branding page: do not upload a logo (with a logo, Google requires a lengthy verification; if you already uploaded one, click Remove). Under App domain, paste the home page, privacy policy and terms below; under Authorized domains, click Add domain and paste the domain. Click Save at the bottom of the page. Without these fields, Google will not let you publish the app (part 5).
onboarding-setup-field-homepage = Home page
onboarding-setup-field-privacy = Privacy policy
onboarding-setup-field-terms = Terms of service
onboarding-setup-field-domain = Authorized domain
onboarding-setup-scopes-title = Choose the permissions
onboarding-setup-scopes-text = In Data access, click Add or remove scopes. At the bottom of the panel, in the Manually add scopes box, paste the three below (separated by commas), click Add to table and check that all three are selected. Click Update and, back on the page, Save at the bottom; without Save, nothing is stored. In the end, email and openid show under Non-sensitive scopes and drive under Your restricted scopes.
onboarding-setup-audience-title = Publish the app
onboarding-setup-audience-text = In Audience, click Publish app and then Confirm. If you see "To publish the app, complete setup on the branding page", go back to part 3 and fill in the App domain fields. After publishing, Google shows "Your app needs to be verified": you can ignore it, the app works without verification (up to 100 accounts). Do not leave it "in testing": in that mode Google signs you out every 7 days, and only accounts added under Test users can sign in.
onboarding-setup-client-title = Create the credential
onboarding-setup-client-text = Under Application type, choose Desktop app, give it a name and click Create. Copy the client ID and the client secret and paste them in the next step. Keep them only in the app: do not send them to anyone.
onboarding-trouble-title = Common problems
onboarding-trouble-publish-blocked = "To publish the app, complete setup on the branding page"
onboarding-trouble-publish-blocked-fix = The home page, privacy policy or authorized domain is missing, or a logo was uploaded. Fill them in and remove the logo in part 3, click Save and try publishing again.
onboarding-trouble-access-denied = "access_denied"
onboarding-trouble-access-denied-fix = You clicked Cancel, or the app is still "in testing" and your account is not under Test users. Publish the app (part 5) or add your account in Audience → Test users and try again.
onboarding-trouble-redirect-mismatch = "redirect_uri_mismatch"
onboarding-trouble-redirect-mismatch-fix = The credential is not a Desktop app. Create a new one (part 6).
onboarding-trouble-weekly-logout = Sign-in stops working after a week
onboarding-trouble-weekly-logout-fix = The app stayed "in testing". Publish the app (part 5) and sign in again.
onboarding-trouble-api-disabled = "Google Drive API has not been used in project…"
onboarding-trouble-api-disabled-fix = The Drive API is not enabled in the project (part 2).
onboarding-waiting-unverified = If you see "Google hasn't verified this app", click Advanced and then Go to the app: the warning appears because the app is yours and was not reviewed by Google. Your data only travels between this computer and Google.
onboarding-step-sign-in = Sign in
onboarding-step-waiting = Browser sign-in
onboarding-step-intro = Get to know the app
onboarding-step-files = Your files
onboarding-step-offline = Working offline
onboarding-step-folders = Computer folders
onboarding-folders-title = Sync folders from this computer
onboarding-folders-text = The folders you check go to "Computers" in Google Drive, and changes sync both ways.
onboarding-folders-none = None of the usual folders (Desktop, Documents, Pictures, Videos) was found.
onboarding-folders-later = You can choose other folders later, in Preferences > My computer.
onboarding-skip = Skip
onboarding-back = Back
onboarding-continue = Continue
onboarding-finish = Done
onboarding-sign-in-title = Sign in with your Google account
onboarding-sign-in-text = Paste the client ID and client secret you created in the previous step. Don't have them yet? Click Back and follow the guide.
onboarding-client-guide = Open the full guide on the project website
onboarding-client-id = Client ID
onboarding-client-secret = Client secret
onboarding-client-note = Both are kept in the system keyring, together with your sign-in.
onboarding-missing-client = Fill in the Client ID and the client secret to continue.
onboarding-login-failed = Sign-in did not work: { $error }
onboarding-waiting-title = Finish signing in in your browser
onboarding-waiting-text = Your browser opened the Google sign-in page. Sign in and allow access; this window moves on by itself. The request is valid for 5 minutes.
onboarding-waiting-other = Browser did not open, or want to use another one? Generate the link and paste it wherever you like.
onboarding-generate-link = Generate link
onboarding-link-copied = Link copied
onboarding-intro-title = Your Drive, right on your computer
onboarding-intro-stream-title = Files when you need them
onboarding-intro-stream-text = Everything shows up in a folder, but only what you open takes up space.
onboarding-intro-sync-title = Always up to date
onboarding-intro-sync-text = What you change here reaches Drive, and what changes in Drive shows up here.
onboarding-intro-offline-title = Offline too
onboarding-intro-offline-text = Keep the files you always want at hand stored on your computer.
onboarding-files-title = See your files in Dolphin
onboarding-files-text = Google Drive is a folder at { $path }. Open it in Dolphin and use it like any other.
onboarding-open-folder = Open the folder
onboarding-offline-title = Files available offline
onboarding-offline-text = To use a file or folder without internet, run in a terminal:
onboarding-offline-command = gdrive-desktop-linux pin <path>
onboarding-offline-list = Offline items are listed under "Offline files" in the main window.

# Preferences
pref-tab-computer = My computer
pref-tab-drive = Google Drive
pref-tab-settings = Settings
pref-computer-empty = No folder from this computer is backed up to Google Drive.
pref-add-folder = Add folder
pref-computer-text = The folders you choose here go to "Computers" in Google Drive, and changes sync both ways.
pref-devices = USB devices and memory cards
pref-ask-devices = Ask whether to back up devices when they're connected
pref-devices-empty = No devices connected.
pref-device-always = Backed up every time it's connected
pref-device-ignored = Ignored
pref-device-busy = Backing up…
pref-device-backup = Back up
pref-device-ignore = Ignore this device
pref-device-forget = Remove
device-backup-title = Back up "{ $name }"?
device-backup-text = The device's files go to "Computers" in Google Drive. Nothing is deleted from the device.
device-backup-remember = Back up every time this device is connected
device-backup-started = Backing up "{ $name }"
folder-open = Open folder
folder-relocate = Choose new location
folder-stop = Stop syncing
folder-stop-title = Stop syncing "{ $name }"?
folder-stop-body = The files stay on this computer and in Google Drive; only changes stop syncing.
folder-stopped = The folder "{ $name }" is no longer synced.
folder-state-stopped-missing = Stopped: the folder wasn't found.
folder-state-stopped-no-permission = Stopped: no permission to read the folder.
folder-state-stopped-moved = Stopped: the folder was moved.
folder-state-confirm-deletes = { $count ->
    [one] Waiting for you to confirm moving 1 item to the trash.
   *[other] Waiting for you to confirm moving { $count } items to the trash.
}
folder-confirm-deletes = Move to trash
folder-config-title = Add folder
folder-config-account = Account
folder-config-summary = { $files ->
    [one] 1 file, { $size }
   *[other] { $files } files, { $size }
}
folder-config-sync = Sync with Google Drive
folder-config-merge = Merge with the folder of the same name already in "Computers"
folder-config-done = Done
folder-added = The folder "{ $name }" is being synced.
folder-refused-missing = The folder doesn't exist.
folder-refused-unreadable = The folder couldn't be read.
folder-refused-nested = This folder is already inside a synced folder or the Google Drive folder.
folder-refused-contains = This folder contains a synced folder or the Google Drive folder.
folder-refused-network = Network folders can't be synced.
folder-refused-folders-disabled = Your administrator turned off syncing folders from this computer.
folder-refused-devices-disabled = Your administrator turned off device backup.
folder-refused-mirror-disabled = Your administrator turned off mirroring My Drive.
folder-refused-mirror-pending = Files in the mirror are still uploading. Try again when they finish.
pref-mirror-name = My Drive
mirror-config-title = Mirror files
mirror-config-text = A copy of all of My Drive stays in this folder on your computer, available even without internet. Shared drives stay in the Google Drive folder.
mirror-config-folder = My Drive folder
mirror-config-pick = Choose folder
mirror-config-summary = { $size } will be downloaded; { $free } free on the disk.
mirror-no-space = There is not enough space.
mirror-config-pending = { $count ->
    [one] 1 item has not been uploaded yet and keeps uploading.
   *[other] { $count } items have not been uploaded yet and keep uploading.
}
mirror-config-done = Mirror
mirror-on = My Drive is being mirrored on this computer.
stream-config-title = Stream files
stream-config-text = Files go back to the cloud and the folder { $path } stops syncing.
stream-config-remove = Move the old folder to the trash
stream-config-done = Stream
stream-on = My Drive files are back in the cloud.
pref-drive-mode = How My Drive lives on this computer
pref-stream = Stream files
pref-stream-text = Files stay in the cloud and are downloaded when you open them. Uses little disk space.
pref-mirror = Mirror files
pref-mirror-text = One copy of everything stays on this computer and another in the cloud.
pref-drive-folder = Google Drive folder
pref-open-folder = Open the folder
pref-shared-drives = Manage shared drives
pref-section-location = Where your files are
pref-mount = Streaming location
pref-sidebar = Show in Dolphin's Places
pref-section-cache = Cache
pref-cache-path = Cache folder
pref-change = Change
pref-reset = Reset
pref-pick-folder = Choose the folder
pref-default = Default
pref-cache-limit = Cache limit (GB)
pref-cache-limit-hint = Empty: the app decides. From 1 to { $max } GB.
pref-cache-storage = See cache storage
pref-section-system = System
pref-autostart = Start the app when you sign in
pref-metered = Pause syncing on metered connections
pref-theme = Theme
pref-theme-system = System
pref-theme-light = Light
pref-section-network = Network
pref-proxy = Proxy
pref-proxy-auto = Detect automatically (from the system)
pref-proxy-direct = Direct connection
pref-proxy-manual = Manual
pref-proxy-server = Proxy address
pref-bandwidth-rx = Limit download (KB/s)
pref-bandwidth-tx = Limit upload (KB/s)
pref-section-other = Other
pref-hotkey = Search shortcut
pref-hotkey-hint = Click the field and press the key combination.
pref-error-reports = Send error reports automatically
pref-error-reports-hint = After a crash, the app offers to open the report, already filled in, in your browser, without tokens or emails; you send it yourself.
pref-account = Account
pref-managed = Managed by your administrator
pref-pending = Pending changes
pref-save = Save
pref-restart-needed = Some changes only take effect after the app restarts.
pref-restart-now = Restart now
pref-saved = Preferences saved
pref-invalid-cache = Use a whole number from 1 to { $max }.
pref-invalid-bandwidth = Use a whole number greater than zero.
pref-invalid-proxy = Use an address like http://server:port (http, https, socks5 or socks5h).

# Notification preferences
notification-settings-title = Notification preferences
notification-settings-text = Choose which notices the app shows. Serious errors and account notices always appear.
notification-settings-empty = No notice can be turned off.

# Quick search
search-recent = Recent
search-none = No items match "{ $query }".
search-failed = The search didn't work: { $error }

# Error list
errors-title = Sync errors
errors-empty = No sync errors.
errors-done = Done
errors-count = { $count ->
    [one] 1 item
   *[other] { $count } items
}
error-category-quota = Storage full
error-category-quota-text = Free up space in Google Drive or get more storage: uploads resume afterwards.
error-category-upload-limit = Daily upload limit reached
error-category-upload-limit-text = Google Drive accepts up to 750 GB of uploads per day. Uploads resume tomorrow.
error-category-download-limit = Download limit reached
error-category-download-limit-text = The file was downloaded too many times in a short period. Google Drive allows it again within 24 hours; ask the owner for a copy if you need it sooner.
error-category-folder-too-deep = Folder nested too deep
error-category-folder-too-deep-text = Google Drive allows up to 100 levels of folders. Move the item to a less nested folder.
error-category-shared-drive-full = Folder or shared drive full
error-category-shared-drive-full-text = A Google Drive folder holds up to 500,000 items. Move or delete items from it, or use another folder.
error-category-no-permission = No permission to edit
error-category-no-permission-text = Your version was kept in Lost and Found.
error-category-too-large = File too large
error-category-too-large-text = The file is larger than Google Drive accepts.
error-category-cloud-missing = The item no longer exists in Google Drive
error-category-cloud-missing-text = It was deleted or is no longer shared with you.
error-category-local-missing = The file no longer exists on this computer
error-category-local-missing-text = It was deleted or moved before it was uploaded.
error-category-lost-and-found = Kept in Lost and Found
error-category-lost-and-found-text = Local versions that couldn't be uploaded, kept for you to review.
error-category-unknown = Other errors
error-category-unknown-text = Try again; if it keeps happening, check each item's technical error.

# Offline files
offline-title = Offline files
offline-usage = Disk usage
offline-usage-offline = Offline
offline-usage-cache = Cache
offline-usage-free = Free
offline-state-on = Available offline
offline-state-mixed = Some content offline
offline-remove = Remove offline access
offline-removed = Offline access removed
offline-folder-empty = Nothing in this folder is available offline.
offline-clear-cache = Clear cache
offline-cache-cleared = { $size } freed
offline-clear-offline = Clear offline files
offline-clear-title = Clear offline files?
offline-clear-body = They will no longer be available without internet and will leave this computer. Nothing is deleted from Google Drive.
offline-clear-confirm = Clear
offline-cleared = Offline files removed from this computer
offline-cache-settings = Cache settings
offline-refresh = Refresh

uninstall-open = Uninstall…
uninstall-title = Uninstall Google Drive?
uninstall-body = The programs are removed from this computer and the Drive folder no longer appears. Your files in Google Drive, in the cloud, are not touched.
uninstall-purge = Also delete the data on this computer
uninstall-purge-hint = The connected account, the cache, offline files, thumbnails and preferences. Copies kept in Lost and Found stay.
uninstall-confirm = Uninstall
uninstall-progress = Uninstalling… If the app came from a package, the system asks for the administrator password.
uninstall-waiting = Finishing pending uploads before disconnecting…
uninstall-done = Google Drive uninstalled
uninstall-done-body = The programs were removed from this computer.

share-missing = Open this window from the Google Drive menu in your file manager.
share-title = Share "{ $name }"
share-cannot = You can only see who has access: changing sharing needs edit permission.
share-add = Add people
share-add-placeholder = Add people by email
share-role = Role
share-role-of = Role of { $name }
share-role-reader = Viewer
share-role-commenter = Commenter
share-role-writer = Editor
share-role-owner = Owner
share-role-remove = Remove access
share-notify = Notify people by email
share-message = Message (optional)
share-send = Send
share-invalid = Check these emails: { $emails }
share-added = { $count ->
    [one] Access given to 1 person
   *[other] Access given to { $count } people
}
share-removed = Access removed
share-people = People with access
share-general = General access
share-general-role = Role of anyone with the link
share-general-restricted = Restricted
share-general-restricted-text = Only people with access can open with the link.
share-general-domain = { $domain }
share-general-domain-text = Anyone at { $domain } with the link can open it.
share-general-anyone = Anyone with the link
share-general-anyone-text = Anyone on the internet with the link can open it.
share-copy-link = Copy link
share-done = Done

versions-missing = Open this window from the Google Drive menu in your file manager.
versions-title = Versions of "{ $name }"
versions-text = Older versions may be deleted after 30 days or 100 versions, except the ones kept forever.
versions-current = Current
versions-kept = Kept forever
versions-actions = Version actions
versions-download = Download
versions-downloaded = Saved to { $path }
versions-restore = Restore
versions-restore-title = Restore this version?
versions-restore-body = The file goes back to its content from { $date }. The current version stays in the list.
versions-restored = Version restored; it uploads as a new version
versions-keep = Keep forever
versions-unkeep = Don't keep forever
versions-kept-done = This version will be kept forever
versions-unkept-done = This version may be deleted over time

shared-drives-title = Shared drives
shared-drives-text = Choose which shared drives appear in the "Shared drives" folder on this computer. The ones you turn off disappear from the folder and stop syncing.
shared-drives-empty = This account has no shared drives.
shared-drives-new = New shared drive
shared-drives-name = Shared drive name
shared-drives-create = Create
shared-drives-created = Shared drive "{ $name }" created
shared-drives-saved = Changes saved

feedback-title = Send feedback
feedback-intro = Tell us what happened or what could be better. The text opens in the browser as a GitHub issue of the project, so you can review it before sending.
feedback-kind = Type
feedback-kind-problem = Problem
feedback-kind-idea = Suggestion
feedback-text = Description
feedback-placeholder = Describe the problem or the suggestion
feedback-include-diagnostics = Include diagnostic info
feedback-public = GitHub issues are public: don't write passwords or personal data.
feedback-send = Send
feedback-attach = Please attach the file { $file } to this issue. It was just saved and its folder was opened; you can open it to check what's inside before attaching it.
feedback-cut = The text was too long and was cut. Finish the description in the browser.
onboarding-step-summary = Summary
onboarding-summary-title = Everything is ready to sync
onboarding-summary-text = Check what you chose. To change the folders, use Back; everything else is in Preferences.
onboarding-summary-account = Account
onboarding-summary-mode = My Drive
onboarding-summary-stream = Streaming (files stay in the cloud)
onboarding-summary-mirror = Mirror (a copy stays on this computer)
onboarding-summary-mount = Google Drive folder
onboarding-summary-folders = Computer folders
onboarding-summary-no-folders = None
onboarding-take-tour = Take the tour
tour-status-title = Sync status
tour-status-text = See whether everything is in sync and what is left to upload, and pause or resume.
tour-search-title = Search
tour-search-text = Find Drive files by name or by content.
tour-nav-title = Activity and folders
tour-nav-text = Activity, connected folders, offline files, shared items and notifications are here.
tour-account-title = Your account
tour-account-text = See your storage, switch accounts or add another one.
tour-more-title = More options
tour-more-text = Preferences, the error list, feedback and everything else the app offers.
tour-next = Next
tour-done = Done
pref-take-tour = Take the tour of the main window
