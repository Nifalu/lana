/// <reference types="svelte" />
/// <reference types="vite/client" />

interface ImportMetaEnv {
  /**
   * Base URL of the lana API (routing, nearest cool spot). Unset:
   * `https://lana.heitzli.ch`. In `npm run dev` use `/heitzli` to go through
   * the Vite proxy (see vite.config.ts), or point it at a dead port to test
   * the offline fallback.
   */
  readonly VITE_API_URL?: string;
  /**
   * Middleware base URL. Unset or empty: the deployed middleware
   * (https://lana-mw.heitzli.ch). Set it to test against another one, e.g. a
   * local stack (`http://127.0.0.1:8090`) or a Mac on the LAN for a phone
   * (`http://192.168.1.20:8090`). Inside Tauri it is applied on every start.
   */
  readonly VITE_SERVER_URL?: string;
  /** `true` hides the debug tools. */
  readonly VITE_RELEASE?: string;
}

interface ImportMeta {
  readonly env: ImportMetaEnv;
}
