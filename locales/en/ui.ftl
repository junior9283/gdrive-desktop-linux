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
header-synced = Sync complete
header-error = Sync incomplete
header-syncing = { $count ->
    [one] Syncing 1 item
   *[other] Syncing { $count } items
}
header-offline = No internet connection
header-quota-full = Storage is full
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
view-offline = Offline files
view-shared = Shared with me
view-notifications = Notifications
view-links = Quick links

home-signed-out = Sign in to see your files and follow the sync.

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
shortcut-added = Shortcut added to My Drive

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
storage-manage = Manage storage

more-errors = { $count ->
    [0] Error list
   *[other] Error list ({ $count })
}
more-notification-settings = Notification preferences
more-reset = Reset preferences
more-preferences = Preferences
more-about = About
more-quit = Quit

reset-title = Reset preferences?
reset-body = Everything you changed in the preferences goes back to the default. Settings defined by your administrator still apply.
reset-confirm = Reset
reset-done = Preferences reset

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
onboarding-setup-branding-text = Click Get started and fill in the app name (for example, Drive on Linux) and your email. Under Audience, choose External. Enter the contact email, accept the policy and click Create.
onboarding-setup-scopes-title = Choose the permissions
onboarding-setup-scopes-text = Under Data access, click Add or remove scopes. In the manual scopes box, paste the three below (one at a time or together), click Add to table, then Update and Save.
onboarding-setup-audience-title = Publish the app
onboarding-setup-audience-text = Under Audience, click Publish app and confirm. Don't leave it "in testing": in that mode Google signs you out every 7 days. Published without verification, the app stays private: only people with your Client ID can use it.
onboarding-setup-client-title = Create the credential
onboarding-setup-client-text = Under Application type, choose Desktop app, give it a name and click Create. Copy the client ID and the client secret: you will paste them in the next step.
onboarding-trouble-title = Common problems
onboarding-trouble-access-denied = "access_denied"
onboarding-trouble-access-denied-fix = You clicked Cancel, or the app stayed "in testing" and your account is not a test user. Publish the app (part 5) and try again.
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
pref-next-version = Coming in version { $version }
pref-drive-mode = How My Drive lives on this computer
pref-stream = Stream files
pref-stream-text = Files stay in the cloud and are downloaded when you open them. Uses little disk space.
pref-mirror = Mirror files
pref-mirror-text = One copy of everything stays on this computer and another in the cloud.
pref-drive-folder = Google Drive folder
pref-open-folder = Open the folder
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
pref-error-reports-hint = In this version the app only keeps your choice: nothing is sent.
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
