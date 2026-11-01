# Security

Please report a vulnerability privately rather than in a public issue: through
[a private advisory](https://github.com/sachesi/chord/security/advisories/new) on GitHub,
or by mail to sachesi <xsachesi@pm.me>. Say what you found, how to reproduce it and which
version you ran; a fix is worked out with you before anything is published.

Only the latest release gets fixes.

## What counts

Chord opens text files that anyone may have written. It starts no programs, has no
plugins, and reaches no network of its own, so opening a file should do no more than show
it. The parts where a mistake matters most:

- Opening a file, including its encoding, its line endings and the language chosen for its
  syntax colours. A file that makes Chord run code, or read or write anything but itself,
  is a vulnerability.
- Saving, which must write only to the file the user chose, keep that file's permissions,
  and leave the file whole if it fails. A way to lead it elsewhere through a link or a
  race, or a save that lets other users read what they could not read before, is a
  vulnerability.
- Files named to a running Chord. A second `chord FILE` hands its files to the window used
  last over your session bus, which only your own processes reach; a request from your own
  session doing what that session could do anyway is not a vulnerability.

A file that shows wrongly or opens slowly is a bug; please file it as an ordinary issue. A
flaw in GtkSourceView, GTK or GLib belongs with that project.
