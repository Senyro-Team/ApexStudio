# Browser connection diagnostics

The ERLCX browser check uses Chrome Native Messaging. Apex Studio creates a short lived challenge, opens the configured Chromium profile, and waits for the extension to return that challenge through the registered `com.apexstudio.erlcx` host and the app's named pipe.

The Settings page reports `checking` while waiting, `connected` after a valid one time response, and `failed` if the browser does not answer within 60 seconds or cannot be launched. The handoff bootstrap runs before it dynamically loads the publishing module, so connection checks cannot race the HTML parser or fail on publishing-module imports. Other handoffs load the module after the bootstrap and report module load/start errors.

The native host writes fixed stage labels with timestamps to `%LOCALAPPDATA%\ApexStudio\chrome\bridge-diagnostics.log`. The log excludes extension origins, tokens, message bodies, and account information. `connection_check_received` proves Chrome started the registered host and sent a check; `app_responded` confirms the round trip reached Apex. No new log entry means Chrome did not start the host for that check.

After changing extension files, reload the unpacked extension from `dist/erlcx-extension` in the configured browser profile, then run **Check connection** in Settings. Version 0.3.2 fixes a parser-order error that prevented the bootstrap from starting and includes the missing Group Revenue module in the built extension. The extension ID must match the ID saved in Apex Studio and the native host manifest's allowed origin.
