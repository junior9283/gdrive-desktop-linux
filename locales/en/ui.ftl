# Texts of the window screens (gdl-ui), formatted by i18n.js.

service-unavailable = The service is not responding. Trying again…
screen-not-ready = This screen is still being built.
action-close = Close

about-name = gdrive-desktop-linux
about-version = Version { $version }
about-license = Free software, released under the GPL-3.0 license.
about-unofficial = Unofficial Google Drive client: this project is not affiliated with or endorsed by Google.
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
