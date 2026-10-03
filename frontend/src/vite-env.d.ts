/// <reference types="svelte" />
/// <reference types="vite/client" />

interface ImportMetaEnv {
  /** Base URL of the routing service, e.g. `http://host:port`. Unset: straight-line stub. */
  readonly VITE_ROUTING_URL?: string;
  /** `true` hides the debug tools. */
  readonly VITE_RELEASE?: string;
}

interface ImportMeta {
  readonly env: ImportMetaEnv;
}
