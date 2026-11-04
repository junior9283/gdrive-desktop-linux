# Demo video script (restricted scope verification)

Upload the recording to YouTube as **Unlisted** and paste the link in the verification form.
Record the whole screen, with no cuts between signing in and the first feature. Narration or
captions in English.

## Before recording

- Install the `gerasync` package from the latest release (built with the official Client ID).
- Set the app and the browser to English: start the app with `LANG=en_US.UTF-8` and set the
  Google account language (or the browser language) to English, so the consent screen is in
  English.
- Open the Google Cloud Console on the project's **Clients** page in another tab, to show the
  same Client ID that appears in the sign-in URL.
- Have a few files in My Drive created on the web (a text file, a document, a folder).

## Scenes

1. **The app.** Show the GeraSync home page (`https://junior9283.github.io/gerasync/`) and the
   release download. Say: "GeraSync is a desktop client for Google Drive on Linux."
2. **Sign in.** Open GeraSync. On the welcome screen click **Continue** until the browser opens the Google sign-in page.
3. **Consent screen.** In the browser, before granting:
   - zoom on the address bar and show the `client_id=` parameter;
   - switch to the Cloud Console tab and show that it is the same Client ID;
   - back on the consent screen, show the app name "GeraSync for Google Drive" and each requested
     permission (see, edit, create and delete all your Google Drive files; email address).
   Click **Continue** to grant.
4. **Mount (`drive`: list).** Open `~/GeraSync` in the file manager. Show My Drive with the files
   created on the web, and a shared drive. Say: "GeraSync lists the user's whole Drive, including
   files it did not create, which is why `drive.file` is not enough."
5. **Open on demand (`drive`: read).** Double-click the text file: it downloads and opens.
6. **Upload a local change (`drive`: write).** Edit the text file and save. Open
   drive.google.com and show the new content and modification time.
7. **Available offline.** Right-click a folder → **Available offline**. Show the status emblem
   change.
8. **Folder backup.** In GeraSync preferences, add a computer folder to sync. Show its files
   appearing in Google Drive on the web.
9. **Disconnect.** Account menu → **Disconnect account**. Say: "Disconnecting removes the token and the
   local cache. Access can also be revoked at myaccount.google.com/permissions."

Length: about 3 to 5 minutes.
