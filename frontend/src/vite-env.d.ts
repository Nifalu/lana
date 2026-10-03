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
   * Server base URL baked into the build, e.g. `http://192.168.1.20:8080`.
   * Inside Tauri it replaces the shell's default (http://127.0.0.1:8080, which
   * on a phone points at the phone itself). Unset: keep the stored URL.
   */
  readonly VITE_SERVER_URL?: string;
  /** `true` hides the debug tools. */
  readonly VITE_RELEASE?: string;
}

interface ImportMeta {
  readonly env: ImportMetaEnv;
}
