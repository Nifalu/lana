{
  description = "lana – Hack am Rhein challenge #3 (Tauri + Rust + SQLite)";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
  };

  outputs = { self, nixpkgs }:
    let
      systems = [
        "x86_64-linux"
        "aarch64-linux"
        "x86_64-darwin"
        "aarch64-darwin"
      ];
      forAllSystems = nixpkgs.lib.genAttrs systems;
    in
    {
      devShells = forAllSystems (system:
        let
          pkgs = nixpkgs.legacyPackages.${system};
          lib = pkgs.lib;

          # GUI/system libraries for building and running Tauri on Linux.
          # Reused for both linking (pkg-config) and the runtime LD_LIBRARY_PATH.
          # On macOS Tauri uses the system WebKit, so nothing extra is needed.
          linuxGuiLibs = with pkgs; [
            glib
            gtk3
            webkitgtk_4_1
            libsoup_3
            cairo
            pango
            gdk-pixbuf
            librsvg
            gobject-introspection
            glib-networking # TLS support inside WebKitGTK
            gst_all_1.gstreamer # WebKitGTK media pipeline (SOS audio in the webview)
            gst_all_1.gst-plugins-base # appsink etc.
            gst_all_1.gst-plugins-good # autoaudiosink etc.
            gst_all_1.gst-plugins-bad # WebVTT encoder, fakevideosink (WebKit probes these at startup)
            dbus
            openssl
            curl
            wget
          ];

          commonDeps = with pkgs; [
            # Rust toolchain (pinned by flake.lock via nixpkgs)
            rustc
            cargo
            rustfmt
            clippy
            rust-analyzer

            # Tauri CLI (tauri dev / tauri build)
            cargo-tauri

            # Frontend tooling – extend once the framework is chosen
            nodejs

            # Postgres with PostGIS (GIS work) – CLI + local server for dev
            (if pkgs.stdenv.hostPlatform.isLinux then
              pkgs.postgresql.withPackages (p: [ p.postgis ])
            else pkgs.postgresql)

            # Misc
            just
            pkg-config
          ];
        in
        {
          default = pkgs.mkShell {
            buildInputs = lib.optionals pkgs.stdenv.hostPlatform.isLinux linuxGuiLibs;
            nativeBuildInputs = commonDeps;

            env = {
              # Local dev database (create with: just db-init && just db-start && just db-createdb)
              DATABASE_URL = "postgres://lana:lana@127.0.0.1:5433/lana";
            } // lib.optionalAttrs pkgs.stdenv.hostPlatform.isLinux {
              # Runtime lookup for shared libs when running the debug binary
              LD_LIBRARY_PATH = lib.makeLibraryPath linuxGuiLibs;
              # SVG loading support for gdk-pixbuf
              GDK_PIXBUF_MODULE_FILE = "${pkgs.librsvg}/lib/gdk-pixbuf/loaders.cache";
              # glib networking modules (TLS) for WebKitGTK
              GIO_MODULE_DIR = "${pkgs.glib-networking}/lib/gio/modules/";

              # Mesa/GPU on non-NixOS hosts: nix-built Mesa only searches
              # /run/opengl-driver (a NixOS-only symlink) for its DRI/GBM
              # drivers, so WebKit's WebProcess aborts with
              # "Could not create default EGL display: EGL_BAD_PARAMETER".
              # Point it into the nix store, let glvnd find Mesa's vendor
              # JSON, and keep the WebKit sandbox off — it would otherwise
              # hide these paths from the render process. Dev-only tradeoff.
              GBM_BACKENDS_PATH = "${pkgs.mesa}/lib/gbm";
              LIBGL_DRIVERS_PATH = "${pkgs.mesa}/lib/dri";
              __EGL_VENDOR_LIBRARY_FILENAMES = "${pkgs.mesa}/share/glvnd/egl_vendor.d/50_mesa.json";
              WEBKIT_DISABLE_SANDBOX_THIS_IS_DANGEROUS = "1";
              # Where GStreamer scans for plugins (non-NixOS: must be explicit)
              GST_PLUGIN_SYSTEM_PATH_1_0 = lib.makeSearchPath "lib/gstreamer-1.0" [
                pkgs.gst_all_1.gstreamer
                pkgs.gst_all_1.gst-plugins-base
                pkgs.gst_all_1.gst-plugins-good
                pkgs.gst_all_1.gst-plugins-bad
              ];
            };

            shellHook = ''
              echo "lana dev shell – rust $(rustc --version | cut -d' ' -f2), node $(node --version), postgres $(psql --version | awk '{print $3}')"
              echo "quick start: just db-init && just db-start && just db-createdb (once), then just dev"
            '';
          };
        });
    };
}
