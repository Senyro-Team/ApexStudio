# Browser connection diagnostics

The ERLCX browser check uses Chrome Native Messaging. Apex Studio creates a short lived challenge, opens the configured Chromium profile, and waits for the extension to return that challenge through the registered `com.apexstudio.erlcx` host and the app's named pipe.

The Settings page reports `checking` while waiting, `connected` after a valid one time response, and `failed` if the browser does not answer within 60 seconds or cannot be launched. A connection check runs from the small handoff bootstrap before the publishing modules load, so errors in unrelated handoff modules cannot prevent the native messaging check. The page reports native messaging errors and detects when its module script cannot load or start for other handoffs.

The native host writes fixed stage labels with timestamps to `%LOCALAPPDATA%\ApexStudio\chrome\bridge-diagnostics.log`. The log excludes extension origins, tokens, message bodies, and account information. `connection_check_received` proves Chrome started the registered host and sent a check; `app_responded` confirms the round trip reached Apex. No new log entry means Chrome did not start the host for that check.

After changing extension files, reload the unpacked extension from `dist/erlcx-extension` in the configured browser profile, then run **Check connection** in Settings. The extension ID must match the ID saved in Apex Studio and the native host manifest's allowed origin.
