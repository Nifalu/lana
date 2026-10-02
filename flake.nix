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
              DATABASE_URL = "postgres://lana:lana@127.0.0.1:5432/lana";
            } // lib.optionalAttrs pkgs.stdenv.hostPlatform.isLinux {
              # Runtime lookup for shared libs when running the debug binary
              LD_LIBRARY_PATH = lib.makeLibraryPath linuxGuiLibs;
              # SVG loading support for gdk-pixbuf
              GDK_PIXBUF_MODULE_FILE = "${pkgs.librsvg}/lib/gdk-pixbuf/loaders.cache";
              # glib networking modules (TLS) for WebKitGTK
              GIO_MODULE_DIR = "${pkgs.glib-networking}/lib/gio/modules/";
            };

            shellHook = ''
              echo "lana dev shell – rust $(rustc --version | cut -d' ' -f2), node $(node --version), postgres $(psql --version | awk '{print $3}')"
              echo "quick start: just db-init && just db-start && just db-createdb (once), then just dev"
                        '';
          };
        });
    };
}
