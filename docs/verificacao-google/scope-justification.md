# Scope justification: `https://www.googleapis.com/auth/drive`

Text for the "How will the scopes be used?" field of the restricted scope verification. The
Console field takes at most 1000 characters: paste the short version there and keep the full text
for the reviewer's questions by e-mail.

## Short version (946 characters, the one in the Console)

GeraSync for Google Drive is an open-source (GPL-3.0) Linux desktop client that shows the user's whole Google Drive (My Drive and shared drives) as a local folder and keeps it in sync. It needs the drive scope to list every file, including those created on the web or by other apps; download a file when the user opens it; upload the user's edits, renames, moves and deletions (to the Drive trash) to existing files; keep files marked offline up to date; back up chosen local folders; and share files and restore versions from the file manager. drive.file only covers files the app created or the user picked one by one, so the user's existing Drive would look empty; drive.readonly cannot upload changes. Everything runs on the user's computer: there is no GeraSync server, tokens are kept only in the system keyring, and there are no ads, analytics or telemetry. Data use follows the Google API Services User Data Policy, including Limited Use.

Features selected in "What features will you use?": **Drive sync client** and **Drive backup**.
Demo video (unlisted): https://youtu.be/d-jxEqwkXvQ

## Full version

---

GeraSync for Google Drive is an open-source (GPL-3.0) desktop client for Linux. It shows the user's
entire Google Drive (My Drive and shared drives) as a folder on the computer and keeps it in sync,
the same way a desktop sync client does on other operating systems. Everything runs on the user's
computer: there is no GeraSync server, and Drive data only travels between the computer and
Google's APIs.

**Features that use the `drive` scope**

1. **Mount the whole Drive as a folder.** GeraSync lists every file and folder in My Drive and in
   the shared drives the user can access, including files created on the web, on phones or by
   other apps, and shows them in the file manager at `~/GeraSync`.
2. **Open files on demand.** When the user opens a file in that folder, GeraSync downloads its
   content; files stay in the cloud until they are opened.
3. **Upload local changes.** When the user edits, renames, moves or deletes a file in the folder,
   GeraSync sends the change to the existing Drive file (deletions go to the Drive trash). These
   are usually files GeraSync did not create.
4. **Available offline.** The user can mark files or folders as available offline; GeraSync keeps
   a local copy and keeps it up to date.
5. **Mirror mode and folder backup.** The user can keep a full copy of My Drive in a local folder,
   and back up chosen computer folders (or removable drives) to Drive.
6. **File manager actions.** Share a file (permissions), view and restore versions (revisions),
   create shortcuts and create a shared drive, from the right-click menu.

**Why narrower scopes are not enough**

- `drive.file` only grants access to files the app created or the user picked one by one. GeraSync
  must list and sync the user's existing Drive as a whole, so the folder would be empty and the
  user's own files could not be opened or updated.
- `drive.readonly` and `drive.metadata.readonly` cannot upload changes, move files to the trash,
  share or restore versions, which are core features.
- `drive.appdata` only covers a hidden app folder, not the user's files.

**Other scopes**

- `openid` and `email`: identify which Google account is signed in, show it in the app and keep
  each account's data apart (a user can connect more than one account).

**Data handling**

- Sign-in uses the OAuth flow for installed apps (loopback `127.0.0.1` and PKCE). Tokens are stored
  only in the system keyring (Secret Service), never in plain-text files.
- File metadata and the content the user opened or pinned are cached on the user's computer only,
  and are removed when the user disconnects the account.
- No user data is sent to any server other than Google's APIs. The only other request is a daily
  check of the latest release version on the public GitHub API, which carries no user data. There
  are no ads, no analytics and no telemetry, and no person reads the user's data.
- Use of the data follows the Google API Services User Data Policy, including the Limited Use
  requirements. Privacy policy: https://gerasync.solusapp.net/privacy.html
