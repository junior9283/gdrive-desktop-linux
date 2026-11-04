# Answer about the security assessment (CASA)

Text to send if Google asks whether GeraSync needs a CASA security assessment. It states our
reading of the rule; the decision belongs to Google.

---

GeraSync for Google Drive is a native desktop application for Linux, distributed as open-source
packages (GPL-3.0, source at https://github.com/junior9283/gdrive-desktop-linux).

- It has **no server-side component**. The developer runs no backend, database, proxy or analytics
  service, and no restricted-scope data is ever transmitted to, stored on or processed by any
  server other than Google's own APIs.
- The OAuth flow is the installed-app flow (loopback redirect to `127.0.0.1`, PKCE). Access and
  refresh tokens are stored only on the user's computer, in the operating system keyring (Secret
  Service).
- Drive metadata and file content are cached only on the user's own computer, and are deleted
  when the user disconnects the account.
- The only other network request is a daily check of the latest release version on the public
  GitHub API, which carries no user data.

Our understanding of the Google API Services User Data Policy is that the security assessment
applies to apps that access restricted-scope data from or through a server, and that apps which
keep the data only on the user's device are not in that group. We would be glad to provide any
further information or to complete the assessment if Google determines it is required.

Contact: gerasync@googlegroups.com
