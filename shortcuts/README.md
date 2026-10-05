# Rowla shortcuts

Each shortcut opens one local URL: `rowla://sort`, `rowla://show`, or `rowla://hide`.
The shortcuts do not run shell commands, request window titles, or contact a web service.
The `.plist` files contain the editable source. The `.shortcut` files are signed import files.

Open Preferences → Menubar and select an **Install Shortcut…** button.
Review the two actions in Shortcuts, then select **Add Shortcut**.
You can assign a keyboard shortcut or run the saved shortcut from Spotlight.

To change an import file, edit its source and convert the plist to binary format
with a `.shortcut` filename. Use `shortcuts sign --mode anyone --input INPUT --output OUTPUT`
to sign it. Signing uses Apple's Shortcuts service and needs the corresponding account access.
Review and exercise the signed file before publication.
