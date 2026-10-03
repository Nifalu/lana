-- Everyone can help: the recurring helper availability windows are gone.
-- Helpers are matched by their shared live location only (devices.is_helper
-- stays as the app's "I can help" opt-in). Dropping the table also drops its
-- indexes; nothing else references it (it only held a FK to devices).
DROP TABLE IF EXISTS helper_windows;
