# frontend

The frontend framework is **not chosen yet** (candidates: SvelteKit with
`adapter-static`, or similar). This directory holds the placeholder meanwhile.

- `dist/` contains a static `index.html` so `tauri dev` runs before any real
  frontend exists. `tauri.conf.json` points `build.frontendDist` here.
- Once a framework is picked, put the app in this directory (e.g. `frontend/`
  as the npm project root), output its build to `frontend/dist`, and add
  `beforeDevCommand` / `beforeBuildCommand` to `src-tauri/tauri.conf.json`.
